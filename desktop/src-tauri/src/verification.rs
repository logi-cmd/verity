// SPDX-License-Identifier: MPL-2.0

use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tauri_plugin_dialog::DialogExt;
use uuid::Uuid;
use verity_core::{
    AgentRepairSession, AgentRepairStatus, CleanupPreview, CleanupReceipt, CleanupSession,
    CleanupSessionStatus, DiagnosticReport, RunPlan, RunProgressEvent, RunSession, SessionStatus,
    VerificationReceipt, AGENT_REPAIR_SCHEMA, CLEANUP_SESSION_SCHEMA, DIAGNOSTIC_REPORT_SCHEMA,
    RUN_SESSION_SCHEMA,
};

struct SessionEntry {
    plan: RunPlan,
    state: Mutex<RunSession>,
    cancelled: AtomicBool,
}

struct AgentSessionEntry {
    repository_root: String,
    state: Mutex<AgentRepairSession>,
    cancelled: Arc<AtomicBool>,
}

struct CleanupSessionEntry {
    state: Mutex<CleanupSession>,
    receipts: Mutex<Vec<CleanupReceipt>>,
    cancelled: Arc<AtomicBool>,
}

fn sessions() -> &'static Mutex<HashMap<String, Arc<SessionEntry>>> {
    static SESSIONS: OnceLock<Mutex<HashMap<String, Arc<SessionEntry>>>> = OnceLock::new();
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn agent_sessions() -> &'static Mutex<HashMap<String, Arc<AgentSessionEntry>>> {
    static SESSIONS: OnceLock<Mutex<HashMap<String, Arc<AgentSessionEntry>>>> = OnceLock::new();
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cleanup_sessions() -> &'static Mutex<HashMap<String, Arc<CleanupSessionEntry>>> {
    static SESSIONS: OnceLock<Mutex<HashMap<String, Arc<CleanupSessionEntry>>>> = OnceLock::new();
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn apply_progress_event(state: &mut RunSession, event: RunProgressEvent) {
    let mut event_progress = event.progress;
    event_progress.event_kind = event.kind.clone();
    if let Some(existing) = state
        .phase_progress
        .iter()
        .find(|item| item.phase == event_progress.phase)
    {
        event_progress.started_at = existing.started_at.clone();
        if let (Ok(started), Ok(heartbeat)) = (
            DateTime::parse_from_rfc3339(&existing.started_at),
            DateTime::parse_from_rfc3339(&event_progress.heartbeat_at),
        ) {
            event_progress.elapsed_ms = heartbeat
                .signed_duration_since(started)
                .num_milliseconds()
                .max(0) as u64;
        }
    }
    if event.kind == verity_core::RunProgressEventKind::Completed
        && event_progress.total_units.is_none()
    {
        event_progress.completed_units = Some(1);
        event_progress.total_units = Some(1);
        event_progress.unit = Some("phase".into());
        event_progress.indeterminate = false;
    }
    state.current_phase = Some(event_progress.phase.clone());
    state.message = event.message;
    state.updated_at = event_progress.heartbeat_at.clone();
    state.progress = Some(event_progress.clone());
    if let Some(existing) = state
        .phase_progress
        .iter_mut()
        .find(|item| item.phase == event_progress.phase)
    {
        *existing = event_progress;
    } else {
        state.phase_progress.push(event_progress);
    }
    if let Some(observation) = event.observation {
        state.observations.push(observation);
        if state.observations.len() > 16 {
            state.observations.drain(..state.observations.len() - 16);
        }
    }
}

#[tauri::command]
pub(crate) async fn pick_repository(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let folder = app
        .dialog()
        .file()
        .set_title("Choose a trusted-source repository")
        .blocking_pick_folder();
    match folder {
        Some(path) => path
            .into_path()
            .map(|value| Some(value.display().to_string()))
            .map_err(|error| error.to_string()),
        None => Ok(None),
    }
}

#[tauri::command]
pub(crate) async fn inspect_repository(repo_root: String) -> Result<RunPlan, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut plan = verity_adapters::inspect_repository(Path::new(&repo_root))
            .map_err(|error| error.to_string())?;
        verity_runner::assess_plan_environment(&mut plan);
        Ok(plan)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub(crate) fn runtime_doctor() -> verity_core::RuntimeCapability {
    verity_runner::runtime_doctor()
}

#[tauri::command]
pub(crate) async fn start_docker_desktop() -> Result<verity_core::RuntimeCapability, String> {
    tauri::async_runtime::spawn_blocking(|| {
        verity_runner::start_docker_desktop(std::time::Duration::from_secs(60))
    })
    .await
    .map_err(|_| "docker_desktop_start_failed".to_string())?
}

#[tauri::command]
pub(crate) fn create_run_session(
    repo_root: String,
    target_id: String,
) -> Result<RunSession, String> {
    let mut plan = verity_adapters::inspect_repository(Path::new(&repo_root))
        .map_err(|error| error.to_string())?;
    verity_runner::assess_plan_environment(&mut plan);
    let target = plan
        .targets
        .iter()
        .find(|target| target.id == target_id)
        .ok_or_else(|| {
            "Target is no longer present in the current repository snapshot.".to_string()
        })?;
    if target.plan_status != verity_core::PlanStatus::Complete {
        return Err(target
            .blockers
            .first()
            .map(|blocker| blocker.summary.clone())
            .unwrap_or_else(|| "Target is blocked.".into()));
    }
    let now = Utc::now().to_rfc3339();
    let state = RunSession {
        schema: RUN_SESSION_SCHEMA.into(),
        id: Uuid::new_v4().to_string(),
        status: SessionStatus::AwaitingConsent,
        repository_root: plan.repository_root.clone(),
        target_id,
        current_phase: None,
        message: "Review the plan and confirm execution.".into(),
        progress: None,
        phase_progress: Vec::new(),
        observations: Vec::new(),
        started_at: None,
        updated_at: now,
        receipt_id: None,
        error: None,
        failure_origin: None,
        failure_code: None,
    };
    let entry = Arc::new(SessionEntry {
        plan,
        state: Mutex::new(state.clone()),
        cancelled: AtomicBool::new(false),
    });
    sessions()
        .lock()
        .map_err(|_| "Session registry is unavailable.".to_string())?
        .insert(state.id.clone(), entry);
    Ok(state)
}

#[tauri::command]
pub(crate) async fn execute_run_session(
    session_id: String,
    confirmed: bool,
) -> Result<VerificationReceipt, String> {
    if !confirmed {
        return Err("Execution requires explicit confirmation.".into());
    }
    let entry = sessions()
        .lock()
        .map_err(|_| "Session registry is unavailable.".to_string())?
        .get(&session_id)
        .cloned()
        .ok_or_else(|| "Run session was not found.".to_string())?;
    {
        let mut state = entry
            .state
            .lock()
            .map_err(|_| "Run state is unavailable.".to_string())?;
        if !matches!(state.status, SessionStatus::AwaitingConsent) {
            return Err("Run session is not awaiting execution.".into());
        }
        state.status = SessionStatus::Running;
        state.started_at = Some(Utc::now().to_rfc3339());
        state.updated_at = Utc::now().to_rfc3339();
        state.message = "Creating an isolated repository snapshot.".into();
    }
    let plan = entry.plan.clone();
    let target = entry
        .state
        .lock()
        .map_err(|_| "Run state is unavailable.".to_string())?
        .target_id
        .clone();
    let session = session_id.clone();
    let progress_entry = entry.clone();
    let native = plan
        .targets
        .iter()
        .find(|item| item.id == target)
        .is_some_and(|item| item.commands.iter().any(|command| command.native));
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut report = |event| {
            if let Ok(mut state) = progress_entry.state.lock() {
                apply_progress_event(&mut state, event);
            }
        };
        if native {
            verity_runner::execute_target_confirmed_native(
                &plan,
                &target,
                &session,
                &progress_entry.cancelled,
                &mut report,
            )
        } else {
            verity_runner::execute_target(
                &plan,
                &target,
                &session,
                &progress_entry.cancelled,
                &mut report,
            )
        }
    })
    .await
    .map_err(|error| error.to_string())?;
    match result {
        Ok(receipt) => {
            let mut state = entry
                .state
                .lock()
                .map_err(|_| "Run state is unavailable.".to_string())?;
            state.status = match receipt.result {
                verity_core::TargetResult::Verified => SessionStatus::Verified,
                verity_core::TargetResult::StartedUnverified => SessionStatus::StartedUnverified,
                _ => SessionStatus::Blocked,
            };
            state.current_phase = Some(verity_core::RunPhase::Receipt);
            state.message = match state.status {
                SessionStatus::Verified => "Machine verification completed.".into(),
                SessionStatus::StartedUnverified => {
                    "The target started, but no complete machine oracle was available.".into()
                }
                _ => "Execution ended with a blocker.".into(),
            };
            state.receipt_id = Some(receipt.id.clone());
            state.updated_at = Utc::now().to_rfc3339();
            Ok(receipt)
        }
        Err(error) => {
            let mut state = entry
                .state
                .lock()
                .map_err(|_| "Run state is unavailable.".to_string())?;
            state.status = if entry.cancelled.load(Ordering::SeqCst) {
                SessionStatus::Cancelled
            } else {
                SessionStatus::Blocked
            };
            state.error = Some(error.to_string());
            state.message = error.to_string();
            let (origin, code) = match &error {
                verity_runner::RunnerError::SnapshotChanged => (
                    verity_core::BlockerOrigin::User,
                    "repository_changed_during_snapshot",
                ),
                verity_runner::RunnerError::RuntimeUnavailable(_) => {
                    (verity_core::BlockerOrigin::Runtime, "runtime_unavailable")
                }
                verity_runner::RunnerError::TargetBlocked(_) => (
                    verity_core::BlockerOrigin::VerityPlan,
                    "target_plan_blocked",
                ),
                verity_runner::RunnerError::Cancelled => {
                    (verity_core::BlockerOrigin::User, "execution_cancelled")
                }
                _ => (
                    verity_core::BlockerOrigin::VerityPlan,
                    "runner_internal_error",
                ),
            };
            state.failure_origin = Some(origin);
            state.failure_code = Some(code.into());
            state.updated_at = Utc::now().to_rfc3339();
            Err(error.to_string())
        }
    }
}

#[tauri::command]
pub(crate) fn read_run_session(session_id: String) -> Result<RunSession, String> {
    let entry = sessions()
        .lock()
        .map_err(|_| "Session registry is unavailable.".to_string())?
        .get(&session_id)
        .cloned()
        .ok_or_else(|| "Run session was not found.".to_string())?;
    let state = entry
        .state
        .lock()
        .map_err(|_| "Run state is unavailable.".to_string())?
        .clone();
    Ok(state)
}

#[tauri::command]
pub(crate) fn cancel_run_session(session_id: String) -> Result<RunSession, String> {
    let entry = sessions()
        .lock()
        .map_err(|_| "Session registry is unavailable.".to_string())?
        .get(&session_id)
        .cloned()
        .ok_or_else(|| "Run session was not found.".to_string())?;
    entry.cancelled.store(true, Ordering::SeqCst);
    let mut state = entry
        .state
        .lock()
        .map_err(|_| "Run state is unavailable.".to_string())?;
    state.message = "Cancellation requested. Verity is stopping the current process.".into();
    state.updated_at = Utc::now().to_rfc3339();
    Ok(state.clone())
}

#[tauri::command]
pub(crate) fn list_receipts() -> Result<Vec<VerificationReceipt>, String> {
    verity_runner::list_receipts().map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn read_receipt(receipt_id: String) -> Result<VerificationReceipt, String> {
    verity_runner::read_receipt(&receipt_id).map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn verify_receipt(receipt_id: String) -> Result<bool, String> {
    let receipt = verity_runner::read_receipt(&receipt_id).map_err(|error| error.to_string())?;
    verity_runner::verify_receipt(&receipt).map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn export_receipt(
    app: tauri::AppHandle,
    receipt_id: String,
) -> Result<Option<String>, String> {
    let receipt = verity_runner::read_receipt(&receipt_id).map_err(|error| error.to_string())?;
    let target = app
        .dialog()
        .file()
        .set_title("Export verification receipt")
        .set_file_name(format!("verity-receipt-{}.json", receipt.id))
        .blocking_save_file();
    let Some(target) = target else {
        return Ok(None);
    };
    let path = target.into_path().map_err(|error| error.to_string())?;
    fs::write(
        &path,
        serde_json::to_vec_pretty(&receipt).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(Some(path.display().to_string()))
}

#[tauri::command]
pub(crate) fn preview_cleanup(
    repo_root: String,
    receipt_id: String,
) -> Result<CleanupPreview, String> {
    let mut plan = verity_adapters::inspect_repository(Path::new(&repo_root))
        .map_err(|error| error.to_string())?;
    verity_runner::assess_plan_environment(&mut plan);
    let receipt = verity_runner::read_receipt(&receipt_id).map_err(|error| error.to_string())?;
    verity_runner::preview_cleanup(&plan, &receipt).map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn start_cleanup(
    repo_root: String,
    receipt_id: String,
    candidate_ids: Vec<String>,
) -> Result<CleanupSession, String> {
    let mut plan = verity_adapters::inspect_repository(Path::new(&repo_root))
        .map_err(|error| error.to_string())?;
    verity_runner::assess_plan_environment(&mut plan);
    let baseline = verity_runner::read_receipt(&receipt_id).map_err(|error| error.to_string())?;
    if baseline.result != verity_core::TargetResult::Verified {
        return Err("cleanup_requires_verified_baseline".into());
    }
    let preview =
        verity_runner::preview_cleanup(&plan, &baseline).map_err(|error| error.to_string())?;
    let now = Utc::now().to_rfc3339();
    let session = CleanupSession {
        schema: CLEANUP_SESSION_SCHEMA.into(),
        id: Uuid::new_v4().to_string(),
        repository_root: plan.repository_root.clone(),
        target_id: baseline.target_id.clone(),
        baseline_receipt_id: baseline.id.clone(),
        status: CleanupSessionStatus::Planning,
        candidates: preview.candidates,
        verified_candidate_ids: Vec::new(),
        started_at: now.clone(),
        updated_at: now,
        error_code: None,
    };
    let entry = Arc::new(CleanupSessionEntry {
        state: Mutex::new(session.clone()),
        receipts: Mutex::new(Vec::new()),
        cancelled: Arc::new(AtomicBool::new(false)),
    });
    cleanup_sessions()
        .lock()
        .map_err(|_| "cleanup_registry_unavailable".to_string())?
        .insert(session.id.clone(), entry.clone());
    let session_id = session.id.clone();
    tauri::async_runtime::spawn(async move {
        if let Ok(mut state) = entry.state.lock() {
            state.status = CleanupSessionStatus::Revalidating;
            state.updated_at = Utc::now().to_rfc3339();
        }
        let cancel = entry.cancelled.clone();
        let id = session_id.clone();
        let result = tauri::async_runtime::spawn_blocking(move || {
            verity_runner::run_cleanup(&plan, &baseline, &candidate_ids, Some(&id), &cancel)
        })
        .await;
        match result {
            Ok(Ok((completed, receipts))) => {
                if let Ok(mut state) = entry.state.lock() {
                    *state = completed;
                }
                if let Ok(mut stored) = entry.receipts.lock() {
                    *stored = receipts;
                }
            }
            Ok(Err(error)) => {
                if let Ok(mut state) = entry.state.lock() {
                    state.status = if entry.cancelled.load(Ordering::SeqCst) {
                        CleanupSessionStatus::Cancelled
                    } else {
                        CleanupSessionStatus::Blocked
                    };
                    state.error_code = Some(error.to_string());
                    state.updated_at = Utc::now().to_rfc3339();
                }
            }
            Err(error) => {
                if let Ok(mut state) = entry.state.lock() {
                    state.status = CleanupSessionStatus::InternalError;
                    state.error_code = Some(error.to_string());
                    state.updated_at = Utc::now().to_rfc3339();
                }
            }
        }
    });
    Ok(session)
}

#[tauri::command]
pub(crate) fn read_cleanup_session(session_id: String) -> Result<CleanupSession, String> {
    if let Some(entry) = cleanup_sessions()
        .lock()
        .map_err(|_| "cleanup_registry_unavailable".to_string())?
        .get(&session_id)
        .cloned()
    {
        return entry
            .state
            .lock()
            .map_err(|_| "cleanup_state_unavailable".to_string())
            .map(|state| state.clone());
    }
    verity_runner::read_cleanup_session(&session_id).map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn cancel_cleanup(session_id: String) -> Result<CleanupSession, String> {
    let entry = cleanup_sessions()
        .lock()
        .map_err(|_| "cleanup_registry_unavailable".to_string())?
        .get(&session_id)
        .cloned()
        .ok_or_else(|| "cleanup_session_not_found".to_string())?;
    entry.cancelled.store(true, Ordering::SeqCst);
    let mut state = entry
        .state
        .lock()
        .map_err(|_| "cleanup_state_unavailable".to_string())?;
    state.updated_at = Utc::now().to_rfc3339();
    Ok(state.clone())
}

#[tauri::command]
pub(crate) fn list_cleanup_receipts(session_id: String) -> Result<Vec<CleanupReceipt>, String> {
    if let Some(entry) = cleanup_sessions()
        .lock()
        .map_err(|_| "cleanup_registry_unavailable".to_string())?
        .get(&session_id)
        .cloned()
    {
        return entry
            .receipts
            .lock()
            .map_err(|_| "cleanup_receipts_unavailable".to_string())
            .map(|items| items.clone());
    }
    verity_runner::read_cleanup_receipts(&session_id).map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn export_cleanup_patch(
    app: tauri::AppHandle,
    session_id: String,
    receipt_id: String,
) -> Result<Option<String>, String> {
    let receipt = verity_runner::read_cleanup_receipts(&session_id)
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|item| item.id == receipt_id)
        .ok_or_else(|| "cleanup_receipt_not_found".to_string())?;
    let target = app
        .dialog()
        .file()
        .set_title("Export verified cleanup patch")
        .set_file_name(format!("verity-cleanup-{}.patch", receipt.id))
        .blocking_save_file();
    let Some(target) = target else {
        return Ok(None);
    };
    let path = target.into_path().map_err(|error| error.to_string())?;
    verity_runner::export_cleanup_patch(&receipt, &path).map_err(|error| error.to_string())?;
    Ok(Some(path.display().to_string()))
}

#[tauri::command]
pub(crate) fn apply_cleanup(
    repo_root: String,
    session_id: String,
    receipt_id: String,
    confirmed: bool,
) -> Result<(), String> {
    if !confirmed {
        return Err("cleanup_writeback_requires_confirmation".into());
    }
    let receipt = verity_runner::read_cleanup_receipts(&session_id)
        .map_err(|error| error.to_string())?
        .into_iter()
        .find(|item| item.id == receipt_id)
        .ok_or_else(|| "cleanup_receipt_not_found".to_string())?;
    verity_runner::apply_cleanup_receipt(&receipt, Path::new(&repo_root))
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn list_agent_capabilities() -> Vec<verity_core::AgentRepairCapability> {
    verity_runner::agent_capabilities()
}

#[tauri::command]
pub(crate) async fn test_agent_capability(
    provider: String,
) -> Result<verity_core::AgentRepairCapability, String> {
    tauri::async_runtime::spawn_blocking(move || verity_runner::test_agent_capability(&provider))
        .await
        .map_err(|_| "agent_capability_test_failed".to_string())?
}

#[tauri::command]
pub(crate) fn launch_agent_desktop(provider: String) -> Result<bool, String> {
    verity_runner::launch_agent_desktop(&provider)
}

#[tauri::command]
pub(crate) fn start_agent_repair(
    repo_root: String,
    target_id: String,
    provider: String,
) -> Result<AgentRepairSession, String> {
    let plan = verity_adapters::inspect_repository(Path::new(&repo_root))
        .map_err(|_| "agent_repository_inspection_failed".to_string())?;
    if !plan.targets.iter().any(|target| target.id == target_id) {
        return Err("agent_target_not_found".into());
    }
    let now = Utc::now().to_rfc3339();
    let state = AgentRepairSession {
        schema: AGENT_REPAIR_SCHEMA.into(),
        id: Uuid::new_v4().to_string(),
        provider: provider.clone(),
        status: AgentRepairStatus::Running,
        target_id: target_id.clone(),
        started_at: now.clone(),
        updated_at: now,
        output: None,
        verification_result: None,
        receipt_id: None,
        error_code: None,
    };
    let entry = Arc::new(AgentSessionEntry {
        repository_root: repo_root,
        state: Mutex::new(state.clone()),
        cancelled: Arc::new(AtomicBool::new(false)),
    });
    agent_sessions()
        .lock()
        .map_err(|_| "agent_session_store_failed".to_string())?
        .insert(state.id.clone(), entry.clone());
    let repair_id = state.id.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result = verity_runner::run_agent_repair(
            &plan,
            &target_id,
            &provider,
            &repair_id,
            entry.cancelled.clone(),
        );
        if let Ok(mut current) = entry.state.lock() {
            current.updated_at = Utc::now().to_rfc3339();
            match result {
                Ok(repair) => {
                    current.status = AgentRepairStatus::Completed;
                    current.output = Some(repair.output);
                    current.verification_result = Some(repair.result);
                    current.receipt_id = Some(repair.receipt_id);
                }
                Err(code) if code == "agent_repair_cancelled" => {
                    current.status = AgentRepairStatus::Cancelled;
                    current.error_code = Some(code);
                }
                Err(code) => {
                    current.status = AgentRepairStatus::Rejected;
                    current.error_code = Some(code);
                }
            }
        }
    });
    Ok(state)
}

#[tauri::command]
pub(crate) fn read_agent_repair(session_id: String) -> Result<AgentRepairSession, String> {
    let entry = agent_sessions()
        .lock()
        .map_err(|_| "agent_session_store_failed".to_string())?
        .get(&session_id)
        .cloned()
        .ok_or_else(|| "agent_session_not_found".to_string())?;
    let state = entry
        .state
        .lock()
        .map_err(|_| "agent_session_store_failed".to_string())?
        .clone();
    Ok(state)
}

#[tauri::command]
pub(crate) fn cancel_agent_repair(session_id: String) -> Result<AgentRepairSession, String> {
    let entry = agent_sessions()
        .lock()
        .map_err(|_| "agent_session_store_failed".to_string())?
        .get(&session_id)
        .cloned()
        .ok_or_else(|| "agent_session_not_found".to_string())?;
    entry.cancelled.store(true, Ordering::SeqCst);
    let mut state = entry
        .state
        .lock()
        .map_err(|_| "agent_session_store_failed".to_string())?;
    state.status = AgentRepairStatus::Cancelled;
    state.updated_at = Utc::now().to_rfc3339();
    Ok(state.clone())
}

#[tauri::command]
pub(crate) fn apply_agent_repair(session_id: String) -> Result<bool, String> {
    let entry = agent_sessions()
        .lock()
        .map_err(|_| "agent_session_store_failed".to_string())?
        .get(&session_id)
        .cloned()
        .ok_or_else(|| "agent_session_not_found".to_string())?;
    let output = {
        let state = entry
            .state
            .lock()
            .map_err(|_| "agent_session_store_failed".to_string())?;
        if state.status != AgentRepairStatus::Completed {
            return Err("agent_repair_not_verified".into());
        }
        state
            .output
            .clone()
            .ok_or_else(|| "agent_repair_output_missing".to_string())?
    };
    verity_runner::apply_verified_agent_repair(Path::new(&entry.repository_root), &output)
}

#[tauri::command]
pub(crate) fn export_agent_patch(
    app: tauri::AppHandle,
    session_id: String,
) -> Result<Option<String>, String> {
    let entry = agent_sessions()
        .lock()
        .map_err(|_| "agent_session_store_failed".to_string())?
        .get(&session_id)
        .cloned()
        .ok_or_else(|| "agent_session_not_found".to_string())?;
    let output = entry
        .state
        .lock()
        .map_err(|_| "agent_session_store_failed".to_string())?
        .output
        .clone()
        .ok_or_else(|| "agent_repair_output_missing".to_string())?;
    let destination = app
        .dialog()
        .file()
        .set_title("Export verified Agent patch")
        .set_file_name("verity-agent-repair.patch")
        .blocking_save_file();
    let Some(destination) = destination else {
        return Ok(None);
    };
    let path = destination.into_path().map_err(|error| error.to_string())?;
    fs::write(&path, output.unified_diff).map_err(|error| error.to_string())?;
    Ok(Some(path.display().to_string()))
}

#[tauri::command]
pub(crate) fn export_agent_task_pack(
    app: tauri::AppHandle,
    repo_root: String,
    target_id: String,
    blocker: Option<Value>,
) -> Result<Option<String>, String> {
    let plan = verity_adapters::inspect_repository(Path::new(&repo_root))
        .map_err(|error| error.to_string())?;
    let target = plan
        .targets
        .iter()
        .find(|target| target.id == target_id)
        .ok_or_else(|| "Target was not found.".to_string())?;
    let task = json!({ "schema": "verity-agent-task-pack.v1", "inspectionFingerprint": plan.inspection_fingerprint, "target": target, "blocker": blocker, "instructions": "Work only inside the supplied repository snapshot. Do not ask the user questions. Return one JSON object containing a unified diff and evidence. Never report success; Verity will re-run the deterministic oracle." });
    let destination = app
        .dialog()
        .file()
        .set_title("Export one-shot Agent task pack")
        .set_file_name("verity-agent-task-pack.json")
        .blocking_save_file();
    let Some(destination) = destination else {
        return Ok(None);
    };
    let path = destination.into_path().map_err(|error| error.to_string())?;
    fs::write(
        &path,
        serde_json::to_vec_pretty(&task).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(Some(path.display().to_string()))
}

#[tauri::command]
pub(crate) fn copy_agent_task(
    repo_root: String,
    target_id: String,
    blocker: Option<Value>,
) -> Result<String, String> {
    let plan = verity_adapters::inspect_repository(Path::new(&repo_root))
        .map_err(|_| "agent_repository_inspection_failed".to_string())?;
    let target = plan
        .targets
        .iter()
        .find(|target| target.id == target_id)
        .ok_or_else(|| "agent_target_not_found".to_string())?;
    let task = json!({ "schema": "verity-agent-task-pack.v1", "inspectionFingerprint": plan.inspection_fingerprint, "target": target, "blocker": blocker, "instructions": "Work only inside the supplied repository snapshot. Do not ask the user questions. Return one JSON object containing a unified diff, evidence, and base file hashes. Never report success; Verity will re-run deterministic verification." });
    let text = serde_json::to_string_pretty(&task)
        .map_err(|_| "agent_task_pack_serialize_failed".to_string())?;
    arboard::Clipboard::new()
        .and_then(|mut clipboard| clipboard.set_text(text))
        .map_err(|_| "agent_task_clipboard_failed".to_string())?;
    Ok("agent_task_copied".into())
}

#[tauri::command]
pub(crate) fn preview_diagnostic_report() -> DiagnosticReport {
    verity_runner::diagnostic_report()
}

#[tauri::command]
pub(crate) fn export_diagnostic_report(
    app: tauri::AppHandle,
    report: DiagnosticReport,
) -> Result<Option<String>, String> {
    if report.schema != DIAGNOSTIC_REPORT_SCHEMA {
        return Err("diagnostic_report_schema_invalid".into());
    }
    let destination = app
        .dialog()
        .file()
        .set_title("Export local Verity diagnostic report")
        .set_file_name("verity-diagnostic-report.json")
        .blocking_save_file();
    let Some(destination) = destination else {
        return Ok(None);
    };
    let path = destination
        .into_path()
        .map_err(|_| "diagnostic_export_path_invalid".to_string())?;
    fs::write(
        &path,
        serde_json::to_vec_pretty(&report)
            .map_err(|_| "diagnostic_report_serialize_failed".to_string())?,
    )
    .map_err(|_| "diagnostic_export_failed".to_string())?;
    Ok(Some(path.display().to_string()))
}

#[tauri::command]
pub(crate) fn copy_diagnostic_issue_summary(report: DiagnosticReport) -> Result<bool, String> {
    if report.schema != DIAGNOSTIC_REPORT_SCHEMA {
        return Err("diagnostic_report_schema_invalid".into());
    }
    let agents = report
        .agents
        .iter()
        .map(|agent| format!("{}/{:?}:{:?}", agent.provider, agent.channel, agent.status))
        .collect::<Vec<_>>()
        .join(", ");
    let summary = format!(
        "Verity {} diagnostics\nHost: {}/{}\nRuntime: {:?} ({})\nAgents: {}",
        report.app_version,
        report.host_os,
        report.host_arch,
        report.runtime_status,
        report.runtime_reason_code,
        agents
    );
    arboard::Clipboard::new()
        .and_then(|mut clipboard| clipboard.set_text(summary))
        .map_err(|_| "diagnostic_clipboard_failed".to_string())?;
    Ok(true)
}

fn secret_entry(project_fingerprint: &str, name: &str) -> Result<keyring::Entry, String> {
    if name.trim().is_empty() || name.len() > 128 {
        return Err("Secret name is invalid.".into());
    }
    keyring::Entry::new(
        &format!("dev.verity.local.{project_fingerprint}"),
        name.trim(),
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn save_project_secret(
    project_fingerprint: String,
    name: String,
    value: String,
) -> Result<bool, String> {
    secret_entry(&project_fingerprint, &name)?
        .set_password(&value)
        .map_err(|error| error.to_string())?;
    Ok(true)
}

#[tauri::command]
pub(crate) fn has_project_secret(
    project_fingerprint: String,
    name: String,
) -> Result<bool, String> {
    Ok(secret_entry(&project_fingerprint, &name)?
        .get_password()
        .is_ok())
}

#[tauri::command]
pub(crate) fn delete_project_secret(
    project_fingerprint: String,
    name: String,
) -> Result<bool, String> {
    let entry = secret_entry(&project_fingerprint, &name)?;
    match entry.delete_credential() {
        Ok(()) => Ok(true),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(error) => Err(error.to_string()),
    }
}

#[tauri::command]
pub(crate) fn open_external_url(url: String) -> Result<bool, String> {
    let value = url.trim();
    if !(value.starts_with("https://") || value.starts_with("http://")) {
        return Err("Only HTTP and HTTPS URLs are allowed.".into());
    }
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("rundll32");
        command.arg("url.dll,FileProtocolHandler").arg(value);
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(value);
        command
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(value);
        command
    };
    command.spawn().map_err(|error| error.to_string())?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use verity_core::{RunObservation, RunPhase, RunProgress, RunProgressEventKind};

    fn event(index: usize) -> RunProgressEvent {
        let at = format!("2026-08-10T00:00:{index:02}Z");
        RunProgressEvent {
            kind: RunProgressEventKind::Observation,
            progress: RunProgress {
                phase: RunPhase::Build,
                event_kind: RunProgressEventKind::Heartbeat,
                completed_units: None,
                total_units: None,
                unit: None,
                indeterminate: true,
                command: vec!["test".into()],
                command_source: None,
                working_directory: ".".into(),
                network: None,
                execution_environment: "test".into(),
                started_at: at.clone(),
                elapsed_ms: index as u64,
                heartbeat_at: at.clone(),
            },
            observation: Some(RunObservation {
                at,
                phase: RunPhase::Build,
                kind: "process_output".into(),
                text: format!("line-{index}"),
            }),
            message: "running".into(),
        }
    }

    #[test]
    fn session_keeps_only_the_latest_sixteen_observations() {
        let mut session = RunSession {
            schema: RUN_SESSION_SCHEMA.into(),
            id: "session".into(),
            status: SessionStatus::Running,
            repository_root: ".".into(),
            target_id: "target".into(),
            current_phase: None,
            message: String::new(),
            progress: None,
            phase_progress: Vec::new(),
            observations: Vec::new(),
            started_at: None,
            updated_at: String::new(),
            receipt_id: None,
            error: None,
            failure_origin: None,
            failure_code: None,
        };
        for index in 0..20 {
            apply_progress_event(&mut session, event(index));
        }
        assert_eq!(session.observations.len(), 16);
        assert_eq!(session.observations.first().unwrap().text, "line-4");
        assert_eq!(
            session.progress.as_ref().unwrap().event_kind,
            RunProgressEventKind::Observation
        );
    }

    #[test]
    fn completed_phase_preserves_start_time_and_reports_real_completion() {
        let mut session = RunSession {
            schema: RUN_SESSION_SCHEMA.into(),
            id: "session".into(),
            status: SessionStatus::Running,
            repository_root: ".".into(),
            target_id: "target".into(),
            current_phase: None,
            message: String::new(),
            progress: None,
            phase_progress: Vec::new(),
            observations: Vec::new(),
            started_at: None,
            updated_at: String::new(),
            receipt_id: None,
            error: None,
            failure_origin: None,
            failure_code: None,
        };
        let mut started = event(0);
        started.kind = RunProgressEventKind::Started;
        started.observation = None;
        let mut completed = event(3);
        completed.kind = RunProgressEventKind::Completed;
        completed.observation = None;

        apply_progress_event(&mut session, started);
        apply_progress_event(&mut session, completed);

        let progress = session.progress.unwrap();
        assert_eq!(progress.started_at, "2026-08-10T00:00:00Z");
        assert_eq!(progress.elapsed_ms, 3_000);
        assert_eq!(progress.completed_units, Some(1));
        assert_eq!(progress.total_units, Some(1));
        assert!(!progress.indeterminate);
    }
}
