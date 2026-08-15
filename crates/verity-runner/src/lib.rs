// SPDX-License-Identifier: MPL-2.0

use chrono::Utc;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand_core::OsRng;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use thiserror::Error;
use uuid::Uuid;
use verity_core::{
    copyable_files, fingerprint_repository, BlockerOrigin, CapabilityCheck, CapabilityState,
    CommandEvidence, EnvironmentStatus, NetworkPolicy, OracleReceipt, PhaseReceipt, PlanStatus,
    ProjectStack, ReceiptVerification, RunObservation, RunPhase, RunPlan, RunProgress,
    RunProgressEvent, RunProgressEventKind, RuntimeCapability, RuntimeStatus, SnapshotLimits,
    TargetResult, VerificationReceipt, RECEIPT_SCHEMA, RECEIPT_VERIFICATION_SCHEMA,
    RUNTIME_CAPABILITY_SCHEMA,
};

mod agents;
mod cleanup;
mod diagnostics;
mod environment;

pub use agents::{
    agent_capabilities, apply_verified_agent_repair, launch_agent_desktop, run_agent_repair,
    test_agent_capability, VerifiedAgentRepair,
};
pub use cleanup::{
    apply_cleanup_receipt, export_cleanup_patch, preview_cleanup, read_cleanup_receipts,
    read_cleanup_session, run_cleanup, CleanupError,
};
pub use diagnostics::{diagnostic_json, diagnostic_report};
pub use environment::{runtime_doctor, start_docker_desktop};

fn unavailable_check(reason: &str) -> CapabilityCheck {
    CapabilityCheck {
        state: CapabilityState::Unavailable,
        version: String::new(),
        reason_code: reason.into(),
    }
}

fn not_checked(reason: &str) -> CapabilityCheck {
    CapabilityCheck {
        state: CapabilityState::NotChecked,
        version: String::new(),
        reason_code: reason.into(),
    }
}

pub fn target_runtime_capability(target: &verity_core::RunTarget) -> RuntimeCapability {
    if target.stack == ProjectStack::Compose
        || target.commands.iter().all(|command| !command.native)
    {
        return runtime_doctor();
    }
    let mut versions = Vec::new();
    let mut missing = Vec::new();
    for program in target
        .commands
        .iter()
        .map(|command| command.program.as_str())
    {
        if versions
            .iter()
            .any(|(name, _): &(String, String)| name == program)
            || missing.iter().any(|name: &String| name == program)
        {
            continue;
        }
        let args: &[&str] = if program.eq_ignore_ascii_case("powershell") {
            &[
                "-NoProfile",
                "-Command",
                "$PSVersionTable.PSVersion.ToString()",
            ]
        } else {
            &["--version"]
        };
        if let Some(version) = host_command_version(program, args) {
            versions.push((program.to_string(), version));
        } else {
            missing.push(program.to_string());
        }
    }
    let ready = missing.is_empty() && !versions.is_empty();
    let reason = if ready {
        "native_toolchain_ready".to_string()
    } else {
        format!("native_toolchain_missing:{}", missing.join(","))
    };
    let cli = if ready {
        CapabilityCheck {
            state: CapabilityState::Available,
            version: versions
                .iter()
                .map(|(name, version)| format!("{name} {version}"))
                .collect::<Vec<_>>()
                .join("; "),
            reason_code: "native_toolchain_ready".into(),
        }
    } else {
        unavailable_check(&reason)
    };
    RuntimeCapability {
        schema: RUNTIME_CAPABILITY_SCHEMA.into(),
        provider: "confirmed_native".into(),
        status: if ready {
            RuntimeStatus::Ready
        } else {
            RuntimeStatus::CapabilityIncomplete
        },
        installed: !versions.is_empty(),
        launchable: ready,
        cli,
        engine: not_checked("container_engine_not_required"),
        buildkit: not_checked("buildkit_not_required"),
        internal_network: not_checked("container_network_not_required"),
        resource_limits: CapabilityCheck {
            state: if ready {
                CapabilityState::Available
            } else {
                CapabilityState::Unknown
            },
            version: "process_group_and_timeout".into(),
            reason_code: if ready {
                "native_process_limits_ready"
            } else {
                "native_process_limits_unchecked"
            }
            .into(),
        },
        reason_code: reason,
    }
}

pub fn assess_plan_environment(plan: &mut RunPlan) {
    for target in &mut plan.targets {
        let capability = target_runtime_capability(target);
        target.environment_status = match capability.status {
            RuntimeStatus::Ready => EnvironmentStatus::Ready,
            RuntimeStatus::CapabilityIncomplete
            | RuntimeStatus::NotInstalled
            | RuntimeStatus::Stopped => EnvironmentStatus::Missing,
            RuntimeStatus::BuildkitUnavailable | RuntimeStatus::DaemonUnreachable => {
                EnvironmentStatus::Incompatible
            }
            RuntimeStatus::Starting | RuntimeStatus::Error => EnvironmentStatus::Unchecked,
        };
        target.environment_reason_code = capability.reason_code;
    }
}

#[derive(Debug, Error)]
pub enum RunnerError {
    #[error("target was not found: {0}")]
    TargetNotFound(String),
    #[error("target is not ready: {0}")]
    TargetBlocked(String),
    #[error("container runtime is unavailable: {0}")]
    RuntimeUnavailable(String),
    #[error("execution was cancelled")]
    Cancelled,
    #[error("execution timed out: {0}")]
    Timeout(String),
    #[error("process failed: {0}")]
    Process(String),
    #[error("filesystem operation failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("snapshot failed: {0}")]
    Snapshot(#[from] verity_core::FingerprintError),
    #[error("repository files changed while the isolated snapshot was being created")]
    SnapshotChanged,
    #[error("receipt signature is invalid")]
    InvalidSignature,
    #[error("unsupported receipt schema: {0}")]
    UnsupportedReceiptSchema(String),
}

#[derive(Debug)]
struct ProcessResult {
    code: Option<i32>,
    output: String,
    duration_ms: u64,
}

pub fn verity_data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Verity")
        .join("v1")
}

fn host_command_version(program: &str, args: &[&str]) -> Option<String> {
    let resolved = resolve_host_program(program);
    let output = Command::new(resolved).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Some(if stdout.is_empty() { stderr } else { stdout })
}

fn resolve_host_program(program: &str) -> PathBuf {
    if program == "godot" {
        std::env::var_os("GODOT4_PATH")
            .filter(|path| Path::new(path).is_file())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(program))
    } else {
        #[cfg(windows)]
        {
            if !program.contains('\\') && !program.contains('/') {
                if let Ok(output) = Command::new("where.exe").arg(program).output() {
                    if output.status.success() {
                        let paths = String::from_utf8_lossy(&output.stdout)
                            .lines()
                            .map(str::trim)
                            .filter(|line| !line.is_empty())
                            .map(PathBuf::from)
                            .collect::<Vec<_>>();
                        if let Some(path) = paths.iter().find(|path| {
                            path.extension()
                                .and_then(|extension| extension.to_str())
                                .is_some_and(|extension| {
                                    matches!(
                                        extension.to_ascii_lowercase().as_str(),
                                        "exe" | "cmd" | "bat" | "com"
                                    )
                                })
                        }) {
                            return path.clone();
                        }
                        if let Some(path) = paths.first() {
                            return path.clone();
                        }
                    }
                }
            }
        }
        PathBuf::from(program)
    }
}

fn apply_runtime_environment(process: &mut Command, program: &str) {
    if let Some(path) = std::env::var_os("GODOT4_PATH").filter(|path| Path::new(path).is_file()) {
        process.env("GODOT4_PATH", path);
    }
    if Path::new(program)
        .file_stem()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("docker"))
    {
        for key in [
            "USERPROFILE",
            "APPDATA",
            "LOCALAPPDATA",
            "PROGRAMDATA",
            "ProgramFiles",
            "ProgramFiles(x86)",
            "HOMEDRIVE",
            "HOMEPATH",
            "DOCKER_CONFIG",
        ] {
            if let Some(value) = std::env::var_os(key) {
                process.env(key, value);
            }
        }
    }
}

fn snapshot(plan: &RunPlan, session_id: &str) -> Result<PathBuf, RunnerError> {
    snapshot_with_progress(plan, session_id, |_| {})
}

fn snapshot_with_progress<F: FnMut(RunProgressEvent)>(
    plan: &RunPlan,
    session_id: &str,
    mut progress: F,
) -> Result<PathBuf, RunnerError> {
    let source = Path::new(&plan.repository_root);
    let target = verity_data_dir()
        .join("sessions")
        .join(session_id)
        .join("snapshot");
    fs::create_dir_all(&target)?;
    let files = copyable_files(source, SnapshotLimits::default())?;
    let total = files.len() as u64;
    let started = Instant::now();
    let started_at = Utc::now().to_rfc3339();
    let mut last_heartbeat = Instant::now();
    let mut copied_bytes = 0_u64;
    for (index, path) in files.into_iter().enumerate() {
        let relative = path
            .strip_prefix(source)
            .map_err(|error| RunnerError::Process(error.to_string()))?;
        let destination = target.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        copied_bytes = copied_bytes.saturating_add(fs::copy(path, destination)?);
        if last_heartbeat.elapsed() >= Duration::from_secs(2) {
            let completed = index as u64 + 1;
            progress(RunProgressEvent {
                kind: RunProgressEventKind::Heartbeat,
                progress: RunProgress {
                    phase: RunPhase::Detect,
                    event_kind: RunProgressEventKind::Heartbeat,
                    completed_units: Some(completed),
                    total_units: Some(total),
                    unit: Some("files".into()),
                    indeterminate: false,
                    command: Vec::new(),
                    command_source: None,
                    working_directory: source.display().to_string(),
                    network: Some(NetworkPolicy::None),
                    execution_environment: "local isolated snapshot".into(),
                    started_at: started_at.clone(),
                    elapsed_ms: started.elapsed().as_millis() as u64,
                    heartbeat_at: Utc::now().to_rfc3339(),
                },
                observation: None,
                message: format!(
                    "Copied {completed}/{total} files · {:.1} MiB",
                    copied_bytes as f64 / 1_048_576.0
                ),
            });
            last_heartbeat = Instant::now();
        }
    }
    Ok(target)
}

fn redact_output(output: &str) -> String {
    output
        .lines()
        .take(240)
        .map(|line| {
            let lower = line.to_ascii_lowercase();
            if ["token=", "password=", "secret=", "api_key=", "apikey="]
                .iter()
                .any(|needle| lower.contains(needle))
            {
                "[redacted secret-bearing output]".to_string()
            } else {
                line.chars().take(600).collect()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn redact_observation_line(line: &str) -> Option<String> {
    let cleaned = line
        .chars()
        .filter(|character| !character.is_control() || *character == '\t')
        .take(600)
        .collect::<String>();
    let cleaned = cleaned.trim();
    if cleaned.is_empty() {
        return None;
    }
    let lower = cleaned.to_ascii_lowercase();
    if ["token=", "password=", "secret=", "api_key=", "apikey="]
        .iter()
        .any(|needle| lower.contains(needle))
    {
        Some("[redacted secret-bearing output]".into())
    } else {
        Some(cleaned.into())
    }
}

fn progress_snapshot(
    phase: RunPhase,
    command: Vec<String>,
    command_source: Option<CommandEvidence>,
    cwd: &Path,
    network: Option<NetworkPolicy>,
    execution_environment: &str,
    started_at: &str,
    elapsed_ms: u64,
    completed_units: Option<u64>,
    total_units: Option<u64>,
) -> RunProgress {
    RunProgress {
        phase,
        event_kind: RunProgressEventKind::Heartbeat,
        completed_units,
        total_units,
        unit: total_units.map(|_| "commands".into()),
        indeterminate: total_units.is_none(),
        command,
        command_source,
        working_directory: cwd.display().to_string(),
        network,
        execution_environment: execution_environment.into(),
        started_at: started_at.into(),
        elapsed_ms,
        heartbeat_at: Utc::now().to_rfc3339(),
    }
}

fn emit_phase_event<F: FnMut(RunProgressEvent)>(
    progress: &mut F,
    phase: RunPhase,
    kind: RunProgressEventKind,
    message: &str,
    cwd: &Path,
    execution_environment: &str,
) {
    let now = Utc::now().to_rfc3339();
    progress(RunProgressEvent {
        kind,
        progress: progress_snapshot(
            phase,
            Vec::new(),
            None,
            cwd,
            None,
            execution_environment,
            &now,
            0,
            None,
            None,
        ),
        observation: None,
        message: message.into(),
    });
}

fn run_process_with_progress<F: FnMut(RunProgressEvent)>(
    program: &str,
    args: &[String],
    cwd: &Path,
    timeout: Duration,
    cancelled: &AtomicBool,
    observe_running_for: Option<Duration>,
    phase: RunPhase,
    command_source: Option<CommandEvidence>,
    network: Option<NetworkPolicy>,
    execution_environment: &str,
    completed_units: Option<u64>,
    total_units: Option<u64>,
    progress: &mut F,
) -> Result<ProcessResult, RunnerError> {
    if cancelled.load(Ordering::SeqCst) {
        return Err(RunnerError::Cancelled);
    }
    let log_dir = verity_data_dir().join("tmp");
    fs::create_dir_all(&log_dir)?;
    let log_path = log_dir.join(format!("{}.log", Uuid::new_v4()));
    let stdout = fs::File::create(&log_path)?;
    let stderr = stdout.try_clone()?;
    let started = Instant::now();
    let started_at = Utc::now().to_rfc3339();
    let command = std::iter::once(program.to_string())
        .chain(args.iter().cloned())
        .collect::<Vec<_>>();
    let initial = progress_snapshot(
        phase.clone(),
        command.clone(),
        command_source.clone(),
        cwd,
        network.clone(),
        execution_environment,
        &started_at,
        0,
        completed_units,
        total_units,
    );
    progress(RunProgressEvent {
        kind: RunProgressEventKind::Started,
        progress: initial,
        observation: None,
        message: format!("Running {program}"),
    });
    let mut process = Command::new(program);
    process
        .args(args)
        .current_dir(cwd)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env(
            "SYSTEMROOT",
            std::env::var_os("SYSTEMROOT").unwrap_or_default(),
        )
        .env("TEMP", std::env::temp_dir())
        .env("TMP", std::env::temp_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    apply_runtime_environment(&mut process, program);
    let mut child = process
        .spawn()
        .map_err(|error| RunnerError::Process(format!("{program}: {error}")))?;

    let mut reader = fs::File::open(&log_path)?;
    let mut offset = 0_u64;
    let mut pending = Vec::new();
    let mut last_heartbeat = Instant::now();
    let mut emit_log = |final_read: bool, progress: &mut F| -> Result<bool, RunnerError> {
        reader.seek(SeekFrom::Start(offset))?;
        let mut chunk = Vec::new();
        reader.read_to_end(&mut chunk)?;
        offset += chunk.len() as u64;
        pending.extend_from_slice(&chunk);
        let split_at = if final_read {
            pending.len()
        } else {
            pending
                .iter()
                .rposition(|byte| *byte == b'\n')
                .map_or(0, |index| index + 1)
        };
        if split_at == 0 {
            return Ok(false);
        }
        let complete = pending.drain(..split_at).collect::<Vec<_>>();
        let text = String::from_utf8_lossy(&complete);
        if let Some(line) = text.lines().filter_map(redact_observation_line).last() {
            let snapshot = progress_snapshot(
                phase.clone(),
                command.clone(),
                command_source.clone(),
                cwd,
                network.clone(),
                execution_environment,
                &started_at,
                started.elapsed().as_millis() as u64,
                completed_units,
                total_units,
            );
            progress(RunProgressEvent {
                kind: RunProgressEventKind::Observation,
                progress: snapshot,
                observation: Some(RunObservation {
                    at: Utc::now().to_rfc3339(),
                    phase: phase.clone(),
                    kind: "process_output".into(),
                    text: line,
                }),
                message: format!("Running {program}"),
            });
            return Ok(true);
        }
        Ok(false)
    };

    let mut observed_running = false;
    let status = loop {
        if cancelled.load(Ordering::SeqCst) {
            terminate_process_tree(&mut child);
            let snapshot = progress_snapshot(
                phase.clone(),
                command.clone(),
                command_source.clone(),
                cwd,
                network.clone(),
                execution_environment,
                &started_at,
                started.elapsed().as_millis() as u64,
                completed_units,
                total_units,
            );
            progress(RunProgressEvent {
                kind: RunProgressEventKind::Cancelled,
                progress: snapshot,
                observation: None,
                message: "Execution cancelled".into(),
            });
            let _ = fs::remove_file(&log_path);
            return Err(RunnerError::Cancelled);
        }
        if started.elapsed() > timeout {
            terminate_process_tree(&mut child);
            let _ = emit_log(true, progress);
            let _ = fs::remove_file(&log_path);
            return Err(RunnerError::Timeout(program.into()));
        }
        if observe_running_for.is_some_and(|duration| started.elapsed() >= duration) {
            emit_log(true, progress)?;
            terminate_process_tree(&mut child);
            observed_running = true;
            break None;
        }
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if last_heartbeat.elapsed() >= Duration::from_secs(2) {
            emit_log(false, progress)?;
            let snapshot = progress_snapshot(
                phase.clone(),
                command.clone(),
                command_source.clone(),
                cwd,
                network.clone(),
                execution_environment,
                &started_at,
                started.elapsed().as_millis() as u64,
                completed_units,
                total_units,
            );
            progress(RunProgressEvent {
                kind: RunProgressEventKind::Heartbeat,
                progress: snapshot,
                observation: None,
                message: format!(
                    "{} · {}s elapsed",
                    command.join(" "),
                    started.elapsed().as_secs()
                ),
            });
            last_heartbeat = Instant::now();
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    emit_log(true, progress)?;
    let mut bytes = Vec::new();
    fs::File::open(&log_path)?
        .take(256 * 1024)
        .read_to_end(&mut bytes)?;
    let _ = fs::remove_file(&log_path);
    let result = ProcessResult {
        code: if observed_running {
            Some(0)
        } else {
            status.and_then(|value| value.code())
        },
        output: redact_output(&String::from_utf8_lossy(&bytes)),
        duration_ms: started.elapsed().as_millis() as u64,
    };
    let completed = progress_snapshot(
        phase,
        command,
        command_source,
        cwd,
        network,
        execution_environment,
        &started_at,
        result.duration_ms,
        total_units.or(completed_units),
        total_units,
    );
    progress(RunProgressEvent {
        kind: if result.code == Some(0) {
            RunProgressEventKind::Completed
        } else {
            RunProgressEventKind::Blocked
        },
        progress: completed,
        observation: None,
        message: if observed_running {
            format!("{program} remained active for the bounded launch observation")
        } else if result.code == Some(0) {
            format!("{program} completed")
        } else {
            format!("{program} failed")
        },
    });
    Ok(result)
}

fn launch_observation_window(stack: &ProjectStack, phase: &RunPhase) -> Option<Duration> {
    (phase == &RunPhase::Launch && stack != &ProjectStack::Compose)
        .then_some(Duration::from_secs(8))
}

fn terminate_process_tree(child: &mut std::process::Child) {
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

struct DockerSessionGuard {
    network: String,
    cache: String,
    container: String,
}

impl DockerSessionGuard {
    fn new(session_id: &str) -> Self {
        Self {
            network: format!("verity-{session_id}"),
            cache: format!("verity-cache-{session_id}"),
            container: format!("verity-app-{session_id}"),
        }
    }
}

impl Drop for DockerSessionGuard {
    fn drop(&mut self) {
        let cleanup = |args: &[&str]| {
            let _ = Command::new("docker")
                .args(args)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        };
        cleanup(&["rm", "-f", &self.container]);
        cleanup(&["network", "rm", &self.network]);
        cleanup(&["volume", "rm", &self.cache]);
    }
}

struct ComposeSessionGuard {
    cwd: PathBuf,
    project: String,
}

impl Drop for ComposeSessionGuard {
    fn drop(&mut self) {
        let _ = Command::new("docker")
            .args([
                "compose",
                "--project-name",
                &self.project,
                "down",
                "--volumes",
                "--remove-orphans",
            ])
            .current_dir(&self.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn run_process(
    program: &str,
    args: &[String],
    cwd: &Path,
    timeout: Duration,
    cancelled: &AtomicBool,
) -> Result<ProcessResult, RunnerError> {
    if cancelled.load(Ordering::SeqCst) {
        return Err(RunnerError::Cancelled);
    }
    let log_dir = verity_data_dir().join("tmp");
    fs::create_dir_all(&log_dir)?;
    let log_path = log_dir.join(format!("{}.log", Uuid::new_v4()));
    let stdout = fs::File::create(&log_path)?;
    let stderr = stdout.try_clone()?;
    let started = Instant::now();
    let mut process = Command::new(program);
    process
        .args(args)
        .current_dir(cwd)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env(
            "SYSTEMROOT",
            std::env::var_os("SYSTEMROOT").unwrap_or_default(),
        )
        .env("TEMP", std::env::temp_dir())
        .env("TMP", std::env::temp_dir())
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    apply_runtime_environment(&mut process, program);
    let mut child = process
        .spawn()
        .map_err(|error| RunnerError::Process(format!("{program}: {error}")))?;
    let status = loop {
        if cancelled.load(Ordering::SeqCst) {
            terminate_process_tree(&mut child);
            return Err(RunnerError::Cancelled);
        }
        if started.elapsed() > timeout {
            terminate_process_tree(&mut child);
            return Err(RunnerError::Timeout(program.into()));
        }
        if let Some(status) = child.try_wait()? {
            break status;
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let mut bytes = Vec::new();
    fs::File::open(&log_path)?
        .take(256 * 1024)
        .read_to_end(&mut bytes)?;
    let _ = fs::remove_file(&log_path);
    Ok(ProcessResult {
        code: status.code(),
        output: redact_output(&String::from_utf8_lossy(&bytes)),
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

fn stack_image(stack: &ProjectStack) -> Option<&'static str> {
    match stack {
        ProjectStack::Node => Some("node:22-bookworm-slim"),
        ProjectStack::Rust => Some("rust:1-bookworm"),
        ProjectStack::Python => Some("ghcr.io/astral-sh/uv:python3.13-bookworm-slim"),
        ProjectStack::Go => Some("golang:1.24-bookworm"),
        ProjectStack::Deno => Some("denoland/deno:2.4.0"),
        ProjectStack::Bun => Some("oven/bun:1.2.18"),
        ProjectStack::StaticWeb => Some("node:22-bookworm-slim"),
        ProjectStack::Java | ProjectStack::Kotlin => Some("maven:3.9-eclipse-temurin-21"),
        ProjectStack::DotNet => Some("mcr.microsoft.com/dotnet/sdk:8.0-bookworm-slim"),
        ProjectStack::Php => Some("composer:2"),
        ProjectStack::Ruby => Some("ruby:3.3-bookworm"),
        ProjectStack::Godot | ProjectStack::Compose | ProjectStack::C | ProjectStack::Cpp => None,
    }
}

fn image_identity(image: &str) -> String {
    host_command_version(
        "docker",
        &[
            "image",
            "inspect",
            "--format",
            "{{if .RepoDigests}}{{index .RepoDigests 0}}{{else}}{{.Id}}{{end}}",
            image,
        ],
    )
    .unwrap_or_else(|| image.to_string())
}

fn container_toolchain(stack: &ProjectStack, image: &str, cancelled: &AtomicBool) -> Vec<String> {
    let (program, args) = match stack {
        ProjectStack::Node => ("node", vec!["--version"]),
        ProjectStack::Rust => ("rustc", vec!["--version"]),
        ProjectStack::Python => ("python", vec!["--version"]),
        ProjectStack::Go => ("go", vec!["version"]),
        ProjectStack::Deno => ("deno", vec!["--version"]),
        ProjectStack::Bun => ("bun", vec!["--version"]),
        ProjectStack::StaticWeb => ("node", vec!["--version"]),
        ProjectStack::Java | ProjectStack::Kotlin => ("java", vec!["--version"]),
        ProjectStack::DotNet => ("dotnet", vec!["--info"]),
        ProjectStack::Php => ("php", vec!["--version"]),
        ProjectStack::Ruby => ("ruby", vec!["--version"]),
        ProjectStack::Godot | ProjectStack::Compose | ProjectStack::C | ProjectStack::Cpp => {
            return Vec::new()
        }
    };
    let mut docker_args = vec![
        "run".into(),
        "--rm".into(),
        "--network".into(),
        "none".into(),
    ];
    if stack == &ProjectStack::Php {
        docker_args.extend(["--entrypoint".into(), String::new()]);
    }
    docker_args.extend([image.into(), program.into(), args[0].into()]);
    run_process(
        "docker",
        &docker_args,
        Path::new("."),
        Duration::from_secs(30),
        cancelled,
    )
    .ok()
    .filter(|result| result.code == Some(0))
    .map(|result| vec![result.output.trim().to_string()])
    .unwrap_or_default()
}

fn docker_host_path(path: &Path) -> String {
    let value = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string();
    if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    value.strip_prefix(r"\\?\").unwrap_or(&value).to_string()
}

fn docker_command(
    snapshot: &Path,
    stack: &ProjectStack,
    planned: &verity_core::PlannedCommand,
    session_id: &str,
) -> Result<Vec<String>, RunnerError> {
    let image = stack_image(stack).ok_or_else(|| {
        RunnerError::RuntimeUnavailable("This target requires the confirmed native runner.".into())
    })?;
    let source = docker_host_path(snapshot);
    let workdir = if planned.relative_cwd.is_empty() {
        "/workspace".into()
    } else {
        format!("/workspace/{}", planned.relative_cwd.replace('\\', "/"))
    };
    let mut args = vec![
        "run".into(),
        "--rm".into(),
        "--cpus".into(),
        "2".into(),
        "--memory".into(),
        "4g".into(),
        "--pids-limit".into(),
        "512".into(),
        "--mount".into(),
        format!("type=bind,source={source},target=/workspace"),
        "--workdir".into(),
        workdir,
    ];
    if planned.network == NetworkPolicy::None {
        args.extend(["--network".into(), "none".into()]);
    }
    if planned.network == NetworkPolicy::InternalOnly {
        args.extend(["--network".into(), format!("verity-{session_id}")]);
    }
    let cache = format!("verity-cache-{session_id}");
    match stack {
        ProjectStack::Rust => args.extend([
            "--mount".into(),
            format!("type=volume,source={cache},target=/usr/local/cargo"),
        ]),
        ProjectStack::Go => args.extend([
            "--mount".into(),
            format!("type=volume,source={cache},target=/go/pkg/mod"),
        ]),
        _ => {}
    }
    if stack == &ProjectStack::Php {
        args.extend(["--entrypoint".into(), String::new()]);
    }
    args.push(image.into());
    args.push(planned.program.clone());
    args.extend(planned.args.clone());
    Ok(args)
}

fn hash_text(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hex::encode(hasher.finalize())
}

fn phase_receipt(
    planned: &verity_core::PlannedCommand,
    result: &ProcessResult,
    started: chrono::DateTime<Utc>,
) -> PhaseReceipt {
    PhaseReceipt {
        phase: planned.phase.clone(),
        command: std::iter::once(planned.program.clone())
            .chain(planned.args.clone())
            .collect(),
        command_source: planned.evidence.clone(),
        started_at: started.to_rfc3339(),
        finished_at: Utc::now().to_rfc3339(),
        duration_ms: result.duration_ms,
        exit_code: result.code,
        network: planned.network.clone(),
        output_sha256: hash_text(&result.output),
        output_excerpt: result.output.clone(),
        success: result.code == Some(0),
    }
}

fn classify_command_failure(
    planned: &verity_core::PlannedCommand,
    output: &str,
) -> verity_core::PlanBlocker {
    let lower = output.to_ascii_lowercase();
    let lock_mismatch = (lower.contains("lock file")
        && (lower.contains("out of date")
            || lower.contains("not up to date")
            || lower.contains("frozen")
            || lower.contains("from lock file")))
        || lower.contains("package.json and package-lock.json")
        || lower.contains("err_pnpm_outdated_lockfile")
        || lower.contains("lockfile would have been modified");
    let generated_argument_invalid = planned.program == "docker"
        && planned.args.iter().any(|arg| arg == "--project-name")
        && lower.contains("unknown flag: --project-name");
    let (origin, code, summary) = if generated_argument_invalid {
        (
            BlockerOrigin::VerityPlan,
            "generated_command_invalid",
            "The generated execution command is invalid",
        )
    } else if lock_mismatch {
        (
            BlockerOrigin::Repository,
            "lockfile_out_of_sync",
            "Dependency lock file is out of sync",
        )
    } else if lower.contains("cannot find module")
        || lower.contains("module not found")
        || lower.contains("no matching package")
        || lower.contains("could not find a version that satisfies")
        || lower.contains("failed to resolve package")
    {
        (
            BlockerOrigin::Repository,
            "dependency_missing",
            "A declared dependency could not be resolved",
        )
    } else if lower.contains("cannot open shared object file")
        || lower.contains("library not loaded")
        || lower.contains("dll was not found")
        || lower.contains("pkg-config") && lower.contains("not found")
    {
        (
            BlockerOrigin::Runtime,
            "system_library_missing",
            "A required system library is missing",
        )
    } else if (lower.contains("not found")
        || lower.contains("is not recognized")
        || lower.contains("no such file or directory"))
        && matches!(planned.phase, RunPhase::Detect | RunPhase::Acquire)
    {
        (
            BlockerOrigin::VerityPlan,
            "planned_tool_missing",
            "The generated plan omitted a required toolchain or component",
        )
    } else if lower.contains("incompatible")
        || lower.contains("unsupported rust version")
        || lower.contains("requires rustc")
        || lower.contains("requires node")
    {
        (
            BlockerOrigin::Runtime,
            "toolchain_incompatible",
            "The selected toolchain is incompatible with the repository",
        )
    } else {
        (
            BlockerOrigin::Repository,
            "repository_command_failed",
            "A repository-declared command failed",
        )
    };
    verity_core::PlanBlocker {
        phase: planned.phase.clone(),
        origin,
        code: code.into(),
        summary: summary.into(),
        detail: output.into(),
        evidence: vec![planned.evidence.clone()],
    }
}

fn docker_html_oracle(
    image: &str,
    network: &str,
    container: &str,
    cancelled: &AtomicBool,
) -> Result<ProcessResult, RunnerError> {
    let script = format!("fetch('http://{container}:4173/').then(async r=>{{const b=await r.text();if(r.status!==200||!/<body[\\s>]/i.test(b)||b.trim().length<=32)process.exit(2);console.log('HTTP 200 with non-empty HTML body')}}).catch(()=>process.exit(3))");
    let started = Instant::now();
    let mut last = ProcessResult {
        code: Some(3),
        output: "Application was not reachable on the internal verification network.".into(),
        duration_ms: 0,
    };
    while started.elapsed() < Duration::from_secs(20) {
        last = run_process(
            "docker",
            &[
                "run".into(),
                "--rm".into(),
                "--network".into(),
                network.into(),
                image.into(),
                "node".into(),
                "-e".into(),
                script.clone(),
            ],
            Path::new("."),
            Duration::from_secs(8),
            cancelled,
        )?;
        if last.code == Some(0) {
            return Ok(last);
        }
        std::thread::sleep(Duration::from_millis(350));
    }
    Ok(last)
}

fn key_path() -> PathBuf {
    verity_data_dir()
        .join("identity")
        .join("local-signing-key.hex")
}
fn signing_key() -> Result<SigningKey, RunnerError> {
    let path = key_path();
    if let Ok(raw) = fs::read_to_string(&path) {
        let bytes =
            hex::decode(raw.trim()).map_err(|error| RunnerError::Process(error.to_string()))?;
        let array: [u8; 32] = bytes
            .try_into()
            .map_err(|_| RunnerError::Process("invalid local signing key".into()))?;
        return Ok(SigningKey::from_bytes(&array));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let key = SigningKey::generate(&mut OsRng);
    fs::write(path, hex::encode(key.to_bytes()))?;
    Ok(key)
}

fn sign_receipt(receipt: &mut VerificationReceipt) -> Result<(), RunnerError> {
    let key = signing_key()?;
    sign_receipt_with_key(receipt, &key)
}

fn sign_receipt_with_key(
    receipt: &mut VerificationReceipt,
    key: &SigningKey,
) -> Result<(), RunnerError> {
    receipt.local_signature.clear();
    receipt.local_public_key = hex::encode(key.verifying_key().to_bytes());
    let bytes = serde_json::to_vec(receipt)?;
    receipt.local_signature = hex::encode(key.sign(&bytes).to_bytes());
    Ok(())
}

pub fn verify_receipt(receipt: &VerificationReceipt) -> Result<bool, RunnerError> {
    let mut unsigned = receipt.clone();
    let signature =
        hex::decode(&unsigned.local_signature).map_err(|_| RunnerError::InvalidSignature)?;
    unsigned.local_signature.clear();
    let public =
        hex::decode(&unsigned.local_public_key).map_err(|_| RunnerError::InvalidSignature)?;
    let key = VerifyingKey::from_bytes(
        &public
            .try_into()
            .map_err(|_| RunnerError::InvalidSignature)?,
    )
    .map_err(|_| RunnerError::InvalidSignature)?;
    let sig = Signature::from_bytes(
        &signature
            .try_into()
            .map_err(|_| RunnerError::InvalidSignature)?,
    );
    Ok(key.verify(&serde_json::to_vec(&unsigned)?, &sig).is_ok())
}

fn receipt_reason_code(
    receipt_schema: &str,
    signature_valid: bool,
    snapshot_fingerprint_matches: bool,
    repository_fingerprint_matches: bool,
    repository_name_matches: bool,
    result: &TargetResult,
) -> &'static str {
    if receipt_schema != RECEIPT_SCHEMA {
        "unsupported-schema"
    } else if !signature_valid {
        "tampered"
    } else if !snapshot_fingerprint_matches {
        "stale"
    } else if !repository_fingerprint_matches {
        if repository_name_matches {
            "stale"
        } else {
            "wrong-repository"
        }
    } else if result != &TargetResult::Verified {
        "not-verified"
    } else {
        "accepted"
    }
}

pub fn verify_receipt_for_repository(
    receipt: &VerificationReceipt,
    repository: &Path,
) -> ReceiptVerification {
    let signature_valid = verify_receipt(receipt).unwrap_or(false);
    let snapshot_fingerprint_matches =
        receipt.snapshot_fingerprint == receipt.repository_fingerprint;
    let repository_fingerprint_matches =
        fingerprint_repository(repository, SnapshotLimits::default())
            .map(|fingerprint| fingerprint == receipt.repository_fingerprint)
            .unwrap_or(false);
    let repository_name_matches = repository
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name == receipt.repository_name)
        .unwrap_or(false);

    let reason_code = receipt_reason_code(
        &receipt.schema,
        signature_valid,
        snapshot_fingerprint_matches,
        repository_fingerprint_matches,
        repository_name_matches,
        &receipt.result,
    );

    ReceiptVerification {
        schema: RECEIPT_VERIFICATION_SCHEMA.into(),
        receipt_id: receipt.id.clone(),
        receipt_schema: receipt.schema.clone(),
        result: receipt.result.clone(),
        signature_valid,
        repository_fingerprint_matches,
        snapshot_fingerprint_matches,
        accepted: reason_code == "accepted",
        reason_code: reason_code.into(),
    }
}

pub fn execute_target<F: FnMut(RunProgressEvent)>(
    plan: &RunPlan,
    target_id: &str,
    session_id: &str,
    cancelled: &AtomicBool,
    mut progress: F,
) -> Result<VerificationReceipt, RunnerError> {
    let target = plan
        .targets
        .iter()
        .find(|target| target.id == target_id)
        .ok_or_else(|| RunnerError::TargetNotFound(target_id.into()))?;
    if target.plan_status != PlanStatus::Complete {
        return Err(RunnerError::TargetBlocked(
            target
                .blockers
                .first()
                .map(|b| b.summary.clone())
                .unwrap_or_else(|| "target is not ready".into()),
        ));
    }
    if target.commands.iter().any(|cmd| cmd.native) {
        return Err(RunnerError::RuntimeUnavailable("Native targets require the desktop confirmation runner; CLI container execution was not attempted.".into()));
    }
    let runtime = runtime_doctor();
    if runtime.status != RuntimeStatus::Ready {
        return Err(RunnerError::RuntimeUnavailable(runtime.reason_code.clone()));
    }
    emit_phase_event(
        &mut progress,
        RunPhase::Detect,
        RunProgressEventKind::Started,
        "Fingerprinting the current repository state",
        Path::new(&plan.repository_root),
        "host inspection",
    );
    let repository_fingerprint = verity_core::fingerprint_repository(
        Path::new(&plan.repository_root),
        SnapshotLimits::default(),
    )?;
    emit_phase_event(
        &mut progress,
        RunPhase::Detect,
        RunProgressEventKind::Heartbeat,
        "Creating a local read-only source snapshot",
        Path::new(&plan.repository_root),
        "host inspection",
    );
    let snapshot = snapshot_with_progress(plan, session_id, &mut progress)?;
    let snapshot_fingerprint =
        verity_core::fingerprint_repository(&snapshot, SnapshotLimits::default())?;
    if repository_fingerprint != snapshot_fingerprint {
        return Err(RunnerError::SnapshotChanged);
    }
    emit_phase_event(
        &mut progress,
        RunPhase::Detect,
        RunProgressEventKind::Completed,
        "Repository fingerprint and isolated snapshot match",
        &snapshot,
        "host inspection",
    );
    let network = format!("verity-{session_id}");
    let _docker_session = DockerSessionGuard::new(session_id);
    let _ = run_process(
        "docker",
        &[
            "network".into(),
            "create".into(),
            "--internal".into(),
            network.clone(),
        ],
        Path::new("."),
        Duration::from_secs(30),
        cancelled,
    );
    let mut phases: Vec<PhaseReceipt> = Vec::new();
    let mut blocker = None;
    for planned in &target.commands {
        if planned.phase == RunPhase::Launch {
            continue;
        }
        let args = docker_command(&snapshot, &target.stack, planned, session_id)?;
        let started = Utc::now();
        let phase_total = target
            .commands
            .iter()
            .filter(|command| command.phase == planned.phase)
            .count() as u64;
        let phase_completed = phases
            .iter()
            .filter(|phase| phase.phase == planned.phase && phase.success)
            .count() as u64;
        let result = run_process_with_progress(
            "docker",
            &args,
            Path::new("."),
            Duration::from_secs(900),
            cancelled,
            None,
            planned.phase.clone(),
            Some(planned.evidence.clone()),
            Some(planned.network.clone()),
            "docker-compatible isolated runtime",
            Some(phase_completed),
            Some(phase_total),
            &mut progress,
        )?;
        let receipt = phase_receipt(planned, &result, started);
        if !receipt.success {
            blocker = Some(classify_command_failure(planned, &receipt.output_excerpt));
            phases.push(receipt);
            break;
        }
        phases.push(receipt);
    }
    let mut oracle = OracleReceipt {
        kind: target.oracle.kind.clone(),
        passed: false,
        detail: "Oracle was not executed because an earlier phase failed.".into(),
        evidence_sha256: String::new(),
    };
    if blocker.is_none() {
        if let Some(planned) = target
            .commands
            .iter()
            .find(|cmd| cmd.phase == RunPhase::Launch)
        {
            let image = stack_image(&target.stack).ok_or_else(|| {
                RunnerError::RuntimeUnavailable("native image unavailable".into())
            })?;
            let source = docker_host_path(&snapshot);
            let workdir = if planned.relative_cwd.is_empty() {
                "/workspace".into()
            } else {
                format!("/workspace/{}", planned.relative_cwd)
            };
            let container = format!("verity-app-{session_id}");
            let mut args = vec![
                "run".into(),
                "-d".into(),
                "--name".into(),
                container.clone(),
                "--network".into(),
                network.clone(),
                "--cpus".into(),
                "2".into(),
                "--memory".into(),
                "4g".into(),
                "--pids-limit".into(),
                "512".into(),
                "--mount".into(),
                format!("type=bind,source={source},target=/workspace"),
                "--workdir".into(),
                workdir,
                image.into(),
                planned.program.clone(),
            ];
            args.extend(planned.args.clone());
            let started = Utc::now();
            let launched = run_process_with_progress(
                "docker",
                &args,
                Path::new("."),
                Duration::from_secs(60),
                cancelled,
                None,
                RunPhase::Launch,
                Some(planned.evidence.clone()),
                Some(planned.network.clone()),
                "docker-compatible internal network",
                Some(0),
                Some(1),
                &mut progress,
            )?;
            phases.push(phase_receipt(planned, &launched, started));
            if launched.code == Some(0) {
                emit_phase_event(
                    &mut progress,
                    RunPhase::Oracle,
                    RunProgressEventKind::Started,
                    "Checking the declared machine oracle",
                    &snapshot,
                    "docker-compatible internal network",
                );
                let oracle_result = if target.oracle.machine_verifiable {
                    docker_html_oracle(image, &network, &container, cancelled)?
                } else {
                    std::thread::sleep(Duration::from_secs(2));
                    run_process(
                        "docker",
                        &[
                            "inspect".into(),
                            "--format".into(),
                            "{{.State.Running}}".into(),
                            container.clone(),
                        ],
                        Path::new("."),
                        Duration::from_secs(15),
                        cancelled,
                    )?
                };
                let signal_passed = oracle_result.code == Some(0)
                    && (target.oracle.machine_verifiable || oracle_result.output.trim() == "true");
                let passed = signal_passed && target.oracle.machine_verifiable;
                oracle = OracleReceipt { kind: target.oracle.kind.clone(), passed, detail: if passed { "HTTP 200 returned a non-empty HTML body on the internal verification network." } else if signal_passed { "The declared process remained running, but no complete machine oracle was available." } else { "The application did not remain running or satisfy its declared machine oracle." }.into(), evidence_sha256: hash_text(&oracle_result.output) };
                if !signal_passed {
                    blocker = Some(verity_core::PlanBlocker {
                        phase: RunPhase::Launch,
                        origin: BlockerOrigin::Repository,
                        code: "repository_process_not_running".into(),
                        summary: "The launched process did not remain running".into(),
                        detail: oracle_result.output.clone(),
                        evidence: vec![planned.evidence.clone()],
                    });
                }
                emit_phase_event(
                    &mut progress,
                    RunPhase::Oracle,
                    if passed || signal_passed {
                        RunProgressEventKind::Completed
                    } else {
                        RunProgressEventKind::Blocked
                    },
                    &oracle.detail,
                    &snapshot,
                    "docker-compatible internal network",
                );
            }
            let _ = run_process(
                "docker",
                &["rm".into(), "-f".into(), container],
                Path::new("."),
                Duration::from_secs(30),
                &AtomicBool::new(false),
            );
        } else {
            emit_phase_event(
                &mut progress,
                RunPhase::Oracle,
                RunProgressEventKind::Started,
                "Checking the repository-declared machine oracle",
                &snapshot,
                "docker-compatible isolated runtime",
            );
            let passed = target.oracle.machine_verifiable
                && phases
                    .iter()
                    .any(|phase| phase.phase == RunPhase::Test && phase.success);
            oracle = OracleReceipt {
                kind: target.oracle.kind.clone(),
                passed,
                detail: if passed {
                    "The repository-declared machine oracle completed successfully."
                } else {
                    "No complete machine oracle was available."
                }
                .into(),
                evidence_sha256: hash_text(&serde_json::to_string(&phases)?),
            };
            emit_phase_event(
                &mut progress,
                RunPhase::Oracle,
                if passed {
                    RunProgressEventKind::Completed
                } else {
                    RunProgressEventKind::Blocked
                },
                &oracle.detail,
                &snapshot,
                "docker-compatible isolated runtime",
            );
        }
    }
    let _ = run_process(
        "docker",
        &["network".into(), "rm".into(), network],
        Path::new("."),
        Duration::from_secs(30),
        &AtomicBool::new(false),
    );
    let _ = run_process(
        "docker",
        &[
            "volume".into(),
            "rm".into(),
            format!("verity-cache-{session_id}"),
        ],
        Path::new("."),
        Duration::from_secs(30),
        &AtomicBool::new(false),
    );
    let all_success = blocker.is_none() && phases.iter().all(|phase| phase.success);
    let result = if all_success && oracle.passed {
        TargetResult::Verified
    } else if all_success
        && phases
            .iter()
            .any(|phase| phase.phase == RunPhase::Launch && phase.success)
    {
        TargetResult::StartedUnverified
    } else {
        TargetResult::Blocked
    };
    emit_phase_event(
        &mut progress,
        RunPhase::Receipt,
        RunProgressEventKind::Started,
        "Signing the local verification receipt",
        &snapshot,
        "local signing key",
    );
    let image = stack_image(&target.stack);
    let toolchain = image
        .map(|value| container_toolchain(&target.stack, value, cancelled))
        .unwrap_or_default();
    let execution_environment = image
        .map(image_identity)
        .unwrap_or_else(|| "confirmed-native".into());
    let mut receipt = VerificationReceipt {
        schema: RECEIPT_SCHEMA.into(),
        id: Uuid::new_v4().to_string(),
        session_id: session_id.into(),
        repository_name: plan.repository_name.clone(),
        repository_fingerprint,
        snapshot_fingerprint,
        target_id: target.id.clone(),
        target_label: target.label.clone(),
        target_relative_root: target.relative_root.clone(),
        stack: target.stack.clone(),
        kind: target.kind.clone(),
        host_os: std::env::consts::OS.into(),
        host_arch: std::env::consts::ARCH.into(),
        execution_environment,
        toolchain,
        runtime,
        result,
        phases,
        oracle,
        first_observed_blocker: blocker,
        created_at: Utc::now().to_rfc3339(),
        local_signature: String::new(),
        local_public_key: String::new(),
        signature_scope:
            "Locally signed, tamper-evident receipt. This is not a remote Verity attestation."
                .into(),
    };
    sign_receipt(&mut receipt)?;
    save_receipt(&receipt)?;
    emit_phase_event(
        &mut progress,
        RunPhase::Receipt,
        RunProgressEventKind::Completed,
        "Local verification receipt signed",
        &snapshot,
        "local signing key",
    );
    Ok(receipt)
}

pub fn execute_target_confirmed_native<F: FnMut(RunProgressEvent)>(
    plan: &RunPlan,
    target_id: &str,
    session_id: &str,
    cancelled: &AtomicBool,
    mut progress: F,
) -> Result<VerificationReceipt, RunnerError> {
    let target = plan
        .targets
        .iter()
        .find(|target| target.id == target_id)
        .ok_or_else(|| RunnerError::TargetNotFound(target_id.into()))?;
    if target.plan_status != PlanStatus::Complete {
        return Err(RunnerError::TargetBlocked(
            target
                .blockers
                .first()
                .map(|item| item.summary.clone())
                .unwrap_or_else(|| "target is not ready".into()),
        ));
    }
    if target.commands.iter().any(|command| !command.native) {
        return Err(RunnerError::TargetBlocked(
            "Mixed native and container execution is not supported for one target.".into(),
        ));
    }
    let runtime = target_runtime_capability(target);
    if runtime.status != RuntimeStatus::Ready {
        return Err(RunnerError::RuntimeUnavailable(runtime.reason_code.clone()));
    }
    let version = runtime.cli.version.clone();
    emit_phase_event(
        &mut progress,
        RunPhase::Detect,
        RunProgressEventKind::Started,
        "Fingerprinting the current repository state",
        Path::new(&plan.repository_root),
        "confirmed native inspection",
    );
    let repository_fingerprint = verity_core::fingerprint_repository(
        Path::new(&plan.repository_root),
        SnapshotLimits::default(),
    )?;
    emit_phase_event(
        &mut progress,
        RunPhase::Detect,
        RunProgressEventKind::Heartbeat,
        "Creating an isolated native-execution snapshot",
        Path::new(&plan.repository_root),
        "confirmed native inspection",
    );
    let snapshot = snapshot_with_progress(plan, session_id, &mut progress)?;
    let snapshot_fingerprint =
        verity_core::fingerprint_repository(&snapshot, SnapshotLimits::default())?;
    if repository_fingerprint != snapshot_fingerprint {
        return Err(RunnerError::SnapshotChanged);
    }
    emit_phase_event(
        &mut progress,
        RunPhase::Detect,
        RunProgressEventKind::Completed,
        "Repository fingerprint and isolated snapshot match",
        &snapshot,
        "confirmed native inspection",
    );
    let compose_project = format!(
        "verity-{}",
        session_id.to_ascii_lowercase().replace('_', "-")
    );
    let compose_cwd = if target.relative_root.is_empty() {
        snapshot.clone()
    } else {
        snapshot.join(&target.relative_root)
    };
    let _compose_guard = (target.stack == ProjectStack::Compose).then(|| ComposeSessionGuard {
        cwd: compose_cwd,
        project: compose_project.clone(),
    });
    let mut phases: Vec<PhaseReceipt> = Vec::new();
    let mut blocker = None;
    for planned in &target.commands {
        let mut effective = planned.clone();
        if target.stack == ProjectStack::Compose
            && effective.program == "docker"
            && effective
                .args
                .first()
                .is_some_and(|value| value == "compose")
        {
            effective
                .args
                .splice(1..1, ["--project-name".into(), compose_project.clone()]);
        }
        let cwd = if planned.relative_cwd.is_empty() {
            snapshot.clone()
        } else {
            snapshot.join(&planned.relative_cwd)
        };
        let started = Utc::now();
        let phase_total = target
            .commands
            .iter()
            .filter(|command| command.phase == planned.phase)
            .count() as u64;
        let phase_completed = phases
            .iter()
            .filter(|phase| phase.phase == planned.phase && phase.success)
            .count() as u64;
        let resolved_program = resolve_host_program(&effective.program);
        let result = run_process_with_progress(
            resolved_program.to_string_lossy().as_ref(),
            &effective.args,
            &cwd,
            Duration::from_secs(900),
            cancelled,
            launch_observation_window(&target.stack, &planned.phase),
            planned.phase.clone(),
            Some(effective.evidence.clone()),
            Some(effective.network.clone()),
            "confirmed native snapshot",
            Some(phase_completed),
            Some(phase_total),
            &mut progress,
        )?;
        let receipt = phase_receipt(&effective, &result, started);
        if !receipt.success {
            blocker = Some(classify_command_failure(
                &effective,
                &receipt.output_excerpt,
            ));
            phases.push(receipt);
            break;
        }
        phases.push(receipt);
    }
    let tests_passed = phases
        .iter()
        .any(|phase| phase.phase == RunPhase::Test && phase.success);
    let launch_passed = phases
        .iter()
        .any(|phase| phase.phase == RunPhase::Launch && phase.success);
    let oracle_passed = blocker.is_none()
        && target.oracle.machine_verifiable
        && match target.oracle.kind {
            verity_core::OracleKind::DeclaredHealth => launch_passed,
            verity_core::OracleKind::DeclaredSmoke => tests_passed && launch_passed,
            verity_core::OracleKind::TestSuite | verity_core::OracleKind::PackageArtifact => {
                tests_passed
                    && (!target
                        .commands
                        .iter()
                        .any(|command| command.phase == RunPhase::Launch)
                        || launch_passed)
            }
            _ => tests_passed && launch_passed,
        };
    if blocker.is_none() {
        emit_phase_event(
            &mut progress,
            RunPhase::Oracle,
            if oracle_passed || launch_passed {
                RunProgressEventKind::Completed
            } else {
                RunProgressEventKind::Blocked
            },
            if oracle_passed {
                "The declared native oracle passed"
            } else if launch_passed {
                "The application started, but no machine-verifiable oracle was available"
            } else {
                "The declared native oracle did not fully pass"
            },
            &snapshot,
            "confirmed native snapshot",
        );
    }
    let oracle_detail = if blocker.is_some() {
        "Oracle was not executed because an earlier phase failed."
    } else if oracle_passed {
        match target.oracle.kind {
            verity_core::OracleKind::DeclaredHealth => {
                "Every declared service reached its repository health check."
            }
            verity_core::OracleKind::DeclaredSmoke => {
                "The checked-in native smoke oracle and bounded launch observation passed."
            }
            verity_core::OracleKind::TestSuite | verity_core::OracleKind::PackageArtifact => {
                "The checked-in native test or package oracle passed."
            }
            _ => "The declared machine oracle passed.",
        }
    } else {
        "Native execution did not provide a passing machine oracle."
    };
    let oracle = OracleReceipt {
        kind: target.oracle.kind.clone(),
        passed: oracle_passed,
        detail: oracle_detail.into(),
        evidence_sha256: hash_text(&serde_json::to_string(&phases)?),
    };
    let result = if oracle_passed {
        TargetResult::Verified
    } else if launch_passed && blocker.is_none() {
        TargetResult::StartedUnverified
    } else {
        TargetResult::Blocked
    };
    emit_phase_event(
        &mut progress,
        RunPhase::Receipt,
        RunProgressEventKind::Started,
        "Signing the local verification receipt",
        &snapshot,
        "local signing key",
    );
    let mut receipt = VerificationReceipt {
        schema: RECEIPT_SCHEMA.into(),
        id: Uuid::new_v4().to_string(),
        session_id: session_id.into(),
        repository_name: plan.repository_name.clone(),
        repository_fingerprint,
        snapshot_fingerprint,
        target_id: target.id.clone(),
        target_label: target.label.clone(),
        target_relative_root: target.relative_root.clone(),
        stack: target.stack.clone(),
        kind: target.kind.clone(),
        host_os: std::env::consts::OS.into(),
        host_arch: std::env::consts::ARCH.into(),
        execution_environment: format!("confirmed-native:{}", runtime.provider),
        toolchain: vec![version],
        runtime,
        result,
        phases,
        oracle,
        first_observed_blocker: blocker,
        created_at: Utc::now().to_rfc3339(),
        local_signature: String::new(),
        local_public_key: String::new(),
        signature_scope:
            "Locally signed, tamper-evident receipt. This is not a remote Verity attestation."
                .into(),
    };
    sign_receipt(&mut receipt)?;
    save_receipt(&receipt)?;
    emit_phase_event(
        &mut progress,
        RunPhase::Receipt,
        RunProgressEventKind::Completed,
        "Local verification receipt signed",
        &snapshot,
        "local signing key",
    );
    Ok(receipt)
}

pub fn save_receipt(receipt: &VerificationReceipt) -> Result<PathBuf, RunnerError> {
    let dir = verity_data_dir().join("receipts");
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{}.json", receipt.id));
    fs::write(&path, serde_json::to_vec_pretty(receipt)?)?;
    Ok(path)
}

pub fn read_receipt(id: &str) -> Result<VerificationReceipt, RunnerError> {
    let receipt: VerificationReceipt = serde_json::from_slice(&fs::read(
        verity_data_dir()
            .join("receipts")
            .join(format!("{id}.json")),
    )?)?;
    if receipt.schema != RECEIPT_SCHEMA {
        return Err(RunnerError::UnsupportedReceiptSchema(receipt.schema));
    }
    Ok(receipt)
}

pub fn list_receipts() -> Result<Vec<VerificationReceipt>, RunnerError> {
    let dir = verity_data_dir().join("receipts");
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut values = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let bytes = fs::read(entry.path())?;
        let value = match serde_json::from_slice::<serde_json::Value>(&bytes) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if value.get("schema").and_then(serde_json::Value::as_str) != Some(RECEIPT_SCHEMA) {
            continue;
        }
        values.push(serde_json::from_value::<VerificationReceipt>(value)?);
    }
    values.sort_by(|a: &VerificationReceipt, b: &VerificationReceipt| {
        b.created_at.cmp(&a.created_at)
    });
    values.truncate(20);
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signed_test_receipt(repository: &Path, result: TargetResult) -> VerificationReceipt {
        let fingerprint = fingerprint_repository(repository, SnapshotLimits::default()).unwrap();
        let mut receipt = VerificationReceipt {
            schema: RECEIPT_SCHEMA.into(),
            id: "receipt-test".into(),
            session_id: "session-test".into(),
            repository_name: repository
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap()
                .into(),
            repository_fingerprint: fingerprint.clone(),
            snapshot_fingerprint: fingerprint,
            target_id: "node-test".into(),
            target_label: "fixture".into(),
            target_relative_root: String::new(),
            stack: ProjectStack::Node,
            kind: verity_core::ProjectKind::Cli,
            host_os: std::env::consts::OS.into(),
            host_arch: std::env::consts::ARCH.into(),
            execution_environment: "test".into(),
            toolchain: Vec::new(),
            runtime: RuntimeCapability {
                schema: RUNTIME_CAPABILITY_SCHEMA.into(),
                provider: "test".into(),
                status: RuntimeStatus::Ready,
                installed: true,
                launchable: true,
                cli: not_checked("test"),
                engine: not_checked("test"),
                buildkit: not_checked("test"),
                internal_network: not_checked("test"),
                resource_limits: not_checked("test"),
                reason_code: "test".into(),
            },
            result,
            phases: Vec::new(),
            oracle: OracleReceipt {
                kind: verity_core::OracleKind::TestSuite,
                passed: true,
                detail: "test".into(),
                evidence_sha256: "test".into(),
            },
            first_observed_blocker: None,
            created_at: "2026-08-16T00:00:00Z".into(),
            local_signature: String::new(),
            local_public_key: String::new(),
            signature_scope: "test".into(),
        };
        sign_receipt_with_key(&mut receipt, &SigningKey::generate(&mut OsRng)).unwrap();
        receipt
    }

    #[test]
    fn redacts_secret_bearing_lines() {
        assert!(!redact_output("ok\nAPI_KEY=abc\ndone").contains("abc"));
        assert_eq!(
            redact_observation_line("API_KEY=abc").as_deref(),
            Some("[redacted secret-bearing output]")
        );
    }

    #[test]
    fn receipt_verification_contract_rejects_every_failed_condition() {
        let cases = [
            (
                "old",
                true,
                true,
                true,
                true,
                TargetResult::Verified,
                "unsupported-schema",
            ),
            (
                RECEIPT_SCHEMA,
                false,
                true,
                true,
                true,
                TargetResult::Verified,
                "tampered",
            ),
            (
                RECEIPT_SCHEMA,
                true,
                false,
                true,
                true,
                TargetResult::Verified,
                "stale",
            ),
            (
                RECEIPT_SCHEMA,
                true,
                true,
                false,
                false,
                TargetResult::Verified,
                "wrong-repository",
            ),
            (
                RECEIPT_SCHEMA,
                true,
                true,
                true,
                true,
                TargetResult::Blocked,
                "not-verified",
            ),
            (
                RECEIPT_SCHEMA,
                true,
                true,
                true,
                true,
                TargetResult::Verified,
                "accepted",
            ),
        ];
        for (schema, signature, snapshot, repository, repository_name, result, expected) in cases {
            assert_eq!(
                receipt_reason_code(
                    schema,
                    signature,
                    snapshot,
                    repository,
                    repository_name,
                    &result,
                ),
                expected
            );
        }
    }

    #[test]
    fn verifies_receipt_against_the_current_repository() {
        let repository = tempfile::tempdir().unwrap();
        fs::write(repository.path().join("package.json"), "{}").unwrap();
        let receipt = signed_test_receipt(repository.path(), TargetResult::Verified);

        let verification = verify_receipt_for_repository(&receipt, repository.path());
        assert!(verification.accepted);
        assert_eq!(verification.reason_code, "accepted");
        assert_eq!(verification.schema, RECEIPT_VERIFICATION_SCHEMA);
    }

    #[test]
    fn rejects_tampered_stale_wrong_schema_and_non_verified_receipts() {
        let repository = tempfile::tempdir().unwrap();
        fs::write(repository.path().join("package.json"), "{}").unwrap();
        let key = SigningKey::generate(&mut OsRng);

        let mut tampered = signed_test_receipt(repository.path(), TargetResult::Verified);
        tampered.repository_name = "tampered".into();
        assert_eq!(
            verify_receipt_for_repository(&tampered, repository.path()).reason_code,
            "tampered"
        );

        let mut stale = signed_test_receipt(repository.path(), TargetResult::Verified);
        fs::write(repository.path().join("package.json"), "{\"changed\":true}").unwrap();
        assert_eq!(
            verify_receipt_for_repository(&stale, repository.path()).reason_code,
            "stale"
        );

        let other_repository = tempfile::tempdir().unwrap();
        fs::write(other_repository.path().join("Cargo.toml"), "[workspace]").unwrap();
        assert_eq!(
            verify_receipt_for_repository(&stale, other_repository.path()).reason_code,
            "wrong-repository"
        );

        stale.snapshot_fingerprint = "different".into();
        sign_receipt_with_key(&mut stale, &key).unwrap();
        assert_eq!(
            verify_receipt_for_repository(&stale, repository.path()).reason_code,
            "stale"
        );

        let mut unsupported = signed_test_receipt(repository.path(), TargetResult::Verified);
        unsupported.schema = "verity-verification-receipt.v2".into();
        sign_receipt_with_key(&mut unsupported, &key).unwrap();
        assert_eq!(
            verify_receipt_for_repository(&unsupported, repository.path()).reason_code,
            "unsupported-schema"
        );

        let blocked = signed_test_receipt(repository.path(), TargetResult::Blocked);
        assert_eq!(
            verify_receipt_for_repository(&blocked, repository.path()).reason_code,
            "not-verified"
        );
    }

    #[test]
    fn strips_windows_extended_path_prefix_for_docker() {
        let value = docker_host_path(Path::new(r"\\?\C:\Users\sample"));
        assert!(!value.starts_with(r"\\?\"));
    }

    #[test]
    fn classifies_npm_ci_lock_mismatch_as_repository_evidence() {
        let planned = verity_core::PlannedCommand {
            phase: RunPhase::Acquire,
            program: "npm".into(),
            args: vec!["ci".into()],
            relative_cwd: String::new(),
            evidence: verity_core::CommandEvidence {
                path: "package.json".into(),
                key: "package-lock.json".into(),
                precedence: 2,
            },
            network: NetworkPolicy::RegistryRestricted,
            native: false,
        };
        let blocker = classify_command_failure(
            &planned,
            "npm ci can only install packages when your package.json and package-lock.json are in sync. Missing: pkg@1 from lock file",
        );
        assert_eq!(blocker.origin, BlockerOrigin::Repository);
        assert_eq!(blocker.code, "lockfile_out_of_sync");
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn sanitized_docker_environment_keeps_compose_discoverable() {
        if host_command_version("docker", &["compose", "version"]).is_none() {
            return;
        }
        let result = run_process(
            "docker",
            &["compose".into(), "version".into()],
            Path::new("."),
            Duration::from_secs(20),
            &AtomicBool::new(false),
        )
        .expect("docker compose probe should run");
        assert_eq!(result.code, Some(0), "{}", result.output);
    }

    #[test]
    fn generated_compose_argument_failures_belong_to_verity_plan() {
        let planned = verity_core::PlannedCommand {
            phase: RunPhase::Build,
            program: "docker".into(),
            args: vec![
                "compose".into(),
                "--project-name".into(),
                "verity-test".into(),
            ],
            relative_cwd: String::new(),
            evidence: verity_core::CommandEvidence {
                path: "compose.yaml".into(),
                key: "services".into(),
                precedence: 1,
            },
            network: NetworkPolicy::NativeUserConfirmed,
            native: true,
        };
        let blocker = classify_command_failure(&planned, "unknown flag: --project-name");
        assert_eq!(blocker.origin, BlockerOrigin::VerityPlan);
        assert_eq!(blocker.code, "generated_command_invalid");
    }

    #[test]
    fn compose_launch_must_exit_instead_of_being_assumed_healthy() {
        assert_eq!(
            launch_observation_window(&ProjectStack::Compose, &RunPhase::Launch),
            None
        );
        assert_eq!(
            launch_observation_window(&ProjectStack::Godot, &RunPhase::Launch),
            Some(Duration::from_secs(8))
        );
        assert_eq!(
            launch_observation_window(&ProjectStack::Node, &RunPhase::Build),
            None
        );
    }

    #[test]
    fn streams_heartbeat_and_redacted_observation_events() {
        #[cfg(target_os = "windows")]
        let (program, args) = (
            "cmd",
            vec![
                "/C".into(),
                "echo start&&ping -n 4 127.0.0.1 >NUL&&echo API_KEY=abc&&echo done".into(),
            ],
        );
        #[cfg(not(target_os = "windows"))]
        let (program, args) = (
            "sh",
            vec![
                "-c".into(),
                "printf 'start\\n'; sleep 2.2; printf 'API_KEY=abc\\ndone\\n'".into(),
            ],
        );
        let mut events = Vec::new();
        let result = run_process_with_progress(
            program,
            &args,
            Path::new("."),
            Duration::from_secs(5),
            &AtomicBool::new(false),
            None,
            RunPhase::Build,
            None,
            Some(NetworkPolicy::None),
            "test runtime",
            None,
            None,
            &mut |event| events.push(event),
        )
        .expect("fixture process should run");
        assert_eq!(result.code, Some(0));
        assert!(events
            .iter()
            .any(|event| event.kind == RunProgressEventKind::Heartbeat));
        assert!(events
            .iter()
            .any(|event| event.kind == RunProgressEventKind::Completed));
        let observations = events
            .iter()
            .filter_map(|event| event.observation.as_ref())
            .map(|observation| observation.text.as_str())
            .collect::<Vec<_>>();
        assert!(observations.iter().any(|line| *line == "done"));
        assert!(!observations.iter().any(|line| line.contains("abc")));
    }
}
