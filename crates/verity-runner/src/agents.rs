// SPDX-License-Identifier: MPL-2.0

use crate::verity_data_dir;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use verity_core::{
    copyable_files, AgentCapabilityStatus, AgentChannel, AgentInstallation, AgentRepairCapability,
    AgentRepairOutput, RunPlan, SnapshotLimits, TargetResult, AGENT_REPAIR_SCHEMA,
};

pub struct VerifiedAgentRepair {
    pub output: AgentRepairOutput,
    pub result: TargetResult,
    pub receipt_id: String,
}

#[derive(Debug, Clone)]
struct CommandEntry {
    path: PathBuf,
    entry_id: String,
    entry_hash: String,
    version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Certification {
    provider: String,
    entry_hash: String,
    version: String,
    certified_at: String,
}

fn sha256_file(path: &Path) -> String {
    fs::read(path)
        .map(|bytes| hex::encode(Sha256::digest(bytes)))
        .unwrap_or_default()
}

#[cfg(target_os = "windows")]
fn command_candidates(name: &str) -> Vec<PathBuf> {
    Command::new("where.exe")
        .arg(name)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| {
            let mut paths: Vec<PathBuf> = String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(str::trim)
                .map(PathBuf::from)
                .filter(|path| path.is_file())
                .collect();
            paths.sort_by_key(|path| {
                match path
                    .extension()
                    .and_then(|value| value.to_str())
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .as_str()
                {
                    "exe" => 0,
                    "cmd" => 1,
                    "bat" => 2,
                    "ps1" => 4,
                    _ => 3,
                }
            });
            paths
        })
        .unwrap_or_default()
}

#[cfg(not(target_os = "windows"))]
fn command_candidates(name: &str) -> Vec<PathBuf> {
    Command::new("sh")
        .args(["-lc", &format!("command -v {name}")])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| {
            let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
            path.is_file().then_some(vec![path])
        })
        .unwrap_or_default()
}

fn configure_command(path: &Path, args: &[&str]) -> Command {
    #[cfg(target_os = "windows")]
    if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("cmd"))
    {
        let mut command = Command::new("cmd.exe");
        command.arg("/d").arg("/c").arg(path).args(args);
        return command;
    }
    let mut command = Command::new(path);
    command.args(args);
    command
}

fn command_output(path: &Path, args: &[&str]) -> Option<Output> {
    configure_command(path, args).output().ok()
}

fn npm_package_version(provider: &str, path: &Path) -> Option<String> {
    let package = match provider {
        "codex" => "@openai/codex",
        "claude" => "@anthropic-ai/claude-code",
        "kimi" => "kimi-code",
        _ => return None,
    };
    let npm_root = path.parent()?.join("node_modules");
    let package_path = package
        .split('/')
        .fold(npm_root, |current, part| current.join(part))
        .join("package.json");
    let value: serde_json::Value = serde_json::from_slice(&fs::read(package_path).ok()?).ok()?;
    value.get("version")?.as_str().map(str::to_string)
}

fn cli_entry(provider: &str) -> Option<CommandEntry> {
    command_candidates(provider).into_iter().find_map(|path| {
        let version = command_output(&path, &["--version"])
            .filter(|output| output.status.success())
            .map(|output| {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if stdout.is_empty() {
                    String::from_utf8_lossy(&output.stderr).trim().to_string()
                } else {
                    stdout
                }
            })
            .filter(|value| !value.is_empty())
            .or_else(|| npm_package_version(provider, &path))?;
        Some(CommandEntry {
            entry_id: format!("cli:{provider}"),
            entry_hash: sha256_file(&path),
            path,
            version,
        })
    })
}

#[cfg(target_os = "windows")]
fn windows_start_app_id(pattern: &str) -> Option<String> {
    let escaped = pattern.replace('\'', "''");
    let script = format!(
        "$a=Get-StartApps | Where-Object {{$_.Name -match '{escaped}'}} | Select-Object -First 1; if($a){{$a.AppID}}"
    );
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .ok()?;
    let start_app = output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|value| !value.is_empty());
    if start_app.is_some() {
        return start_app;
    }
    let package_name = match pattern {
        "Codex" => "OpenAI.Codex",
        _ => return None,
    };
    let script = format!(
        "$p=Get-AppxPackage -Name '{package_name}' | Select-Object -First 1; if($p){{$m=Get-AppxPackageManifest -Package $p; \"$($p.PackageFamilyName)!$($m.Package.Applications.Application.Id)\"}}"
    );
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(not(target_os = "windows"))]
fn windows_start_app_id(_pattern: &str) -> Option<String> {
    None
}

fn desktop_installation(provider: &str) -> Option<AgentInstallation> {
    let pattern = match provider {
        "codex" => "Codex",
        "claude" => "Claude",
        "kimi" => "Kimi",
        _ => return None,
    };
    let app_id = windows_start_app_id(pattern)?;
    Some(AgentInstallation {
        channel: AgentChannel::Desktop,
        status: AgentCapabilityStatus::Detected,
        version: String::new(),
        entry_id: format!("app:{app_id}"),
        entry_hash: String::new(),
        launchable: true,
        reason_code: "agent_desktop_detected".into(),
    })
}

fn ollama_service_available() -> bool {
    std::net::TcpStream::connect_timeout(
        &"127.0.0.1:11434".parse().expect("valid Ollama address"),
        Duration::from_millis(250),
    )
    .is_ok()
}

fn certification_path() -> PathBuf {
    verity_data_dir().join("agent-certifications-v2.json")
}

fn certifications() -> Vec<Certification> {
    fs::read(certification_path())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save_certification(certification: Certification) -> Result<(), String> {
    let mut items = certifications();
    items.retain(|item| item.provider != certification.provider);
    items.push(certification);
    let path = certification_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|_| "agent_certification_store_failed".to_string())?;
    }
    fs::write(
        path,
        serde_json::to_vec_pretty(&items)
            .map_err(|_| "agent_certification_store_failed".to_string())?,
    )
    .map_err(|_| "agent_certification_store_failed".to_string())
}

fn is_certified(provider: &str, entry: &CommandEntry) -> bool {
    certifications().iter().any(|item| {
        item.provider == provider
            && item.entry_hash == entry.entry_hash
            && item.version == entry.version
    })
}

pub fn agent_capabilities() -> Vec<AgentRepairCapability> {
    ["codex", "claude", "kimi", "ollama"]
        .into_iter()
        .map(|provider| {
            let entry = cli_entry(provider);
            let certified = entry
                .as_ref()
                .is_some_and(|entry| is_certified(provider, entry));
            let mut installations = Vec::new();
            if let Some(entry) = &entry {
                let unsupported = provider == "kimi";
                installations.push(AgentInstallation {
                    channel: AgentChannel::Cli,
                    status: if unsupported {
                        AgentCapabilityStatus::UnsupportedVersion
                    } else if certified {
                        AgentCapabilityStatus::Certified
                    } else {
                        AgentCapabilityStatus::CapabilityTestRequired
                    },
                    version: entry.version.clone(),
                    entry_id: entry.entry_id.clone(),
                    entry_hash: entry.entry_hash.clone(),
                    launchable: true,
                    reason_code: if unsupported {
                        "kimi_wrapper_contract_unsupported"
                    } else if certified {
                        "agent_cli_certified"
                    } else {
                        "agent_capability_test_required"
                    }
                    .into(),
                });
            }
            if let Some(desktop) = desktop_installation(provider) {
                installations.push(desktop);
            }
            if provider == "ollama" && ollama_service_available() {
                installations.push(AgentInstallation {
                    channel: AgentChannel::LocalService,
                    status: AgentCapabilityStatus::Detected,
                    version: String::new(),
                    entry_id: "service:ollama:11434".into(),
                    entry_hash: String::new(),
                    launchable: false,
                    reason_code: "ollama_service_detected".into(),
                });
            }
            if installations.is_empty() {
                installations.push(AgentInstallation {
                    channel: if provider == "ollama" {
                        AgentChannel::LocalService
                    } else {
                        AgentChannel::Cli
                    },
                    status: AgentCapabilityStatus::NotInstalled,
                    version: String::new(),
                    entry_id: String::new(),
                    entry_hash: String::new(),
                    launchable: false,
                    reason_code: "agent_not_installed".into(),
                });
            }
            AgentRepairCapability {
                schema: AGENT_REPAIR_SCHEMA.into(),
                provider: provider.into(),
                installations,
                non_interactive: certified,
                directory_confined: certified,
                structured_patch: certified,
                cancellable: certified,
                usable_in_app: certified,
                reason_code: if certified {
                    "agent_cli_certified"
                } else if provider == "kimi" && entry.is_some() {
                    "kimi_wrapper_contract_unsupported"
                } else if entry.is_some() {
                    "agent_capability_test_required"
                } else {
                    "agent_not_installed"
                }
                .into(),
            }
        })
        .collect()
}

fn allowed_environment() -> BTreeMap<String, String> {
    let names = [
        "PATH",
        "PATHEXT",
        "SYSTEMROOT",
        "WINDIR",
        "COMSPEC",
        "USERPROFILE",
        "HOME",
        "APPDATA",
        "LOCALAPPDATA",
        "TEMP",
        "TMP",
        "HTTPS_PROXY",
        "HTTP_PROXY",
        "NO_PROXY",
        "CODEX_HOME",
        "OPENAI_API_KEY",
        "ANTHROPIC_API_KEY",
    ];
    names
        .into_iter()
        .filter_map(|name| std::env::var(name).ok().map(|value| (name.into(), value)))
        .collect()
}

fn spawn_with_clean_environment(mut command: Command, cwd: &Path) -> Result<Child, String> {
    command
        .current_dir(cwd)
        .env_clear()
        .envs(allowed_environment())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "agent_probe_start_failed".to_string())
}

#[cfg(target_os = "windows")]
struct ProcessTreeGuard(windows_sys::Win32::Foundation::HANDLE);

#[cfg(target_os = "windows")]
impl ProcessTreeGuard {
    fn attach(child: &Child) -> Result<Self, String> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };
        let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if job.is_null() {
            return Err("agent_job_create_failed".into());
        }
        let mut information: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        information.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let configured = unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &information as *const _ as *const _,
                std::mem::size_of_val(&information) as u32,
            )
        };
        let assigned = unsafe { AssignProcessToJobObject(job, child.as_raw_handle() as _) };
        if configured == 0 || assigned == 0 {
            unsafe { windows_sys::Win32::Foundation::CloseHandle(job) };
            return Err("agent_job_attach_failed".into());
        }
        Ok(Self(job))
    }
}

#[cfg(target_os = "windows")]
impl Drop for ProcessTreeGuard {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0) };
    }
}

#[cfg(not(target_os = "windows"))]
struct ProcessTreeGuard;

#[cfg(not(target_os = "windows"))]
impl ProcessTreeGuard {
    fn attach(_child: &Child) -> Result<Self, String> {
        Ok(Self)
    }
}

fn wait_with_timeout(
    mut child: Child,
    timeout: Duration,
    cancelled: Option<&AtomicBool>,
) -> Result<Output, String> {
    let _tree = ProcessTreeGuard::attach(&child)?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "agent_probe_read_failed".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "agent_probe_read_failed".to_string())?;
    let stdout_reader = thread::spawn(move || {
        let mut reader = stdout;
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).map(|_| bytes)
    });
    let stderr_reader = thread::spawn(move || {
        let mut reader = stderr;
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).map(|_| bytes)
    });
    let started = Instant::now();
    loop {
        if cancelled.is_some_and(|flag| flag.load(Ordering::SeqCst)) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err("agent_repair_cancelled".into());
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = stdout_reader
                    .join()
                    .map_err(|_| "agent_probe_read_failed".to_string())?
                    .map_err(|_| "agent_probe_read_failed".to_string())?;
                let stderr = stderr_reader
                    .join()
                    .map_err(|_| "agent_probe_read_failed".to_string())?
                    .map_err(|_| "agent_probe_read_failed".to_string())?;
                return Ok(Output {
                    status,
                    stdout,
                    stderr,
                });
            }
            Ok(None) if started.elapsed() < timeout => thread::sleep(Duration::from_millis(100)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err("agent_probe_timeout".into());
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err("agent_probe_wait_failed".into());
            }
        }
    }
}

fn capability_schema() -> &'static str {
    r#"{"type":"object","additionalProperties":false,"properties":{"unified_diff":{"type":"string"},"evidence":{"type":"array","items":{"type":"string"}},"base_files":{"type":"array","items":{"type":"object","additionalProperties":false,"properties":{"path":{"type":"string"},"sha256":{"type":"string"}},"required":["path","sha256"]}}},"required":["unified_diff","evidence","base_files"]}"#
}

fn parse_structured_result(provider: &str, output: &Output, result_path: &Path) -> bool {
    let direct = if provider == "codex" {
        fs::read_to_string(result_path).unwrap_or_default()
    } else {
        String::from_utf8_lossy(&output.stdout).to_string()
    };
    if serde_json::from_str::<serde_json::Value>(&direct)
        .ok()
        .is_some_and(|value| {
            value.get("unified_diff").is_some() || value.get("structured_output").is_some()
        })
    {
        return true;
    }
    serde_json::from_str::<serde_json::Value>(&direct)
        .ok()
        .and_then(|value| {
            value
                .get("result")
                .and_then(|value| value.as_str())
                .map(str::to_string)
        })
        .and_then(|value| serde_json::from_str::<serde_json::Value>(&value).ok())
        .is_some_and(|value| value.get("unified_diff").is_some())
}

fn codex_capability_args(
    schema_path: &str,
    result_path: &str,
    working_directory: &str,
    prompt: &str,
) -> Vec<String> {
    [
        "exec",
        "--ephemeral",
        "--ignore-user-config",
        "--skip-git-repo-check",
        "--sandbox",
        "read-only",
        "--output-schema",
        schema_path,
        "--output-last-message",
        result_path,
        "--color",
        "never",
        "-C",
        working_directory,
        prompt,
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

pub fn test_agent_capability(provider: &str) -> Result<AgentRepairCapability, String> {
    if !matches!(provider, "codex" | "claude") {
        return Err(if provider == "kimi" {
            "kimi_wrapper_contract_unsupported"
        } else {
            "agent_provider_not_certifiable"
        }
        .into());
    }
    let entry = cli_entry(provider).ok_or_else(|| "agent_not_installed".to_string())?;
    if provider == "claude"
        && std::env::var("ANTHROPIC_API_KEY").map_or(true, |value| value.trim().is_empty())
    {
        return Err("claude_bare_auth_required".into());
    }
    let temp = TempDir::new().map_err(|_| "agent_probe_workspace_failed".to_string())?;
    let schema_path = temp.path().join("schema.json");
    let result_path = temp.path().join("result.json");
    fs::write(&schema_path, capability_schema())
        .map_err(|_| "agent_probe_workspace_failed".to_string())?;
    fs::write(
        temp.path().join("probe.txt"),
        "verity-agent-capability-probe",
    )
    .map_err(|_| "agent_probe_workspace_failed".to_string())?;
    let prompt = "Return one JSON object matching the supplied schema. Set unified_diff to an empty string and evidence to [\"capability-probe\"]. Do not modify files and do not ask questions.";
    let schema_arg = schema_path.to_string_lossy().to_string();
    let result_arg = result_path.to_string_lossy().to_string();
    let working_arg = temp.path().to_string_lossy().to_string();
    let command = if provider == "codex" {
        let owned_args = codex_capability_args(&schema_arg, &result_arg, &working_arg, prompt);
        let args: Vec<&str> = owned_args.iter().map(String::as_str).collect();
        configure_command(&entry.path, &args)
    } else {
        let args = [
            "--print",
            "--bare",
            "--no-session-persistence",
            "--output-format",
            "json",
            "--json-schema",
            capability_schema(),
            "--tools",
            "Read,Grep,Glob",
            "--permission-mode",
            "dontAsk",
            prompt,
        ];
        configure_command(&entry.path, &args)
    };
    let output = wait_with_timeout(
        spawn_with_clean_environment(command, temp.path())?,
        Duration::from_secs(90),
        None,
    )?;
    if !output.status.success() || !parse_structured_result(provider, &output, &result_path) {
        return Err("agent_structured_probe_failed".into());
    }
    if fs::read_to_string(temp.path().join("probe.txt")).unwrap_or_default()
        != "verity-agent-capability-probe"
    {
        return Err("agent_directory_contract_failed".into());
    }
    save_certification(Certification {
        provider: provider.into(),
        entry_hash: entry.entry_hash,
        version: entry.version,
        certified_at: Utc::now().to_rfc3339(),
    })?;
    agent_capabilities()
        .into_iter()
        .find(|capability| capability.provider == provider)
        .ok_or_else(|| "agent_capability_refresh_failed".into())
}

pub fn launch_agent_desktop(provider: &str) -> Result<bool, String> {
    let installation =
        desktop_installation(provider).ok_or_else(|| "agent_desktop_not_installed".to_string())?;
    let app_id = installation
        .entry_id
        .strip_prefix("app:")
        .ok_or_else(|| "agent_desktop_launch_unsupported".to_string())?;
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer.exe")
            .arg(format!("shell:AppsFolder\\{app_id}"))
            .spawn()
            .map_err(|_| "agent_desktop_launch_failed".to_string())?;
        Ok(true)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = app_id;
        Err("agent_desktop_launch_unsupported".into())
    }
}

fn copy_repository_snapshot(source: &Path, target: &Path) -> Result<(), String> {
    fs::create_dir_all(target).map_err(|_| "agent_snapshot_failed".to_string())?;
    let files = copyable_files(source, SnapshotLimits::default())
        .map_err(|_| "agent_snapshot_failed".to_string())?;
    for path in files {
        let relative = path
            .strip_prefix(source)
            .map_err(|_| "agent_snapshot_failed".to_string())?;
        let destination = target.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|_| "agent_snapshot_failed".to_string())?;
        }
        fs::copy(path, destination).map_err(|_| "agent_snapshot_failed".to_string())?;
    }
    Ok(())
}

fn snapshots_for_agent(plan: &RunPlan, repair_id: &str) -> Result<(PathBuf, PathBuf), String> {
    let source = Path::new(&plan.repository_root);
    let root = verity_data_dir().join("agent-repairs").join(repair_id);
    let agent = root.join("agent-snapshot");
    let verification = root.join("verification-snapshot");
    copy_repository_snapshot(source, &agent)?;
    copy_repository_snapshot(source, &verification)?;
    Ok((agent, verification))
}

fn validate_repair_output(
    snapshot: &Path,
    output: AgentRepairOutput,
) -> Result<AgentRepairOutput, String> {
    if output.unified_diff.trim().is_empty() || output.unified_diff.len() > 2_000_000 {
        return Err("agent_diff_invalid".into());
    }
    let mut existing_paths = BTreeSet::new();
    for line in output
        .unified_diff
        .lines()
        .filter(|line| line.starts_with("+++ ") || line.starts_with("--- "))
    {
        let raw = line[4..].trim();
        if raw == "/dev/null" {
            continue;
        }
        let relative = raw
            .strip_prefix("a/")
            .or_else(|| raw.strip_prefix("b/"))
            .unwrap_or(raw);
        let path = Path::new(relative);
        if path.is_absolute()
            || path
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err("agent_diff_path_outside_snapshot".into());
        }
        if line.starts_with("--- ") {
            existing_paths.insert(relative.replace('\\', "/"));
        }
    }
    let declared_base_paths: BTreeSet<String> = output
        .base_files
        .iter()
        .map(|base| base.path.replace('\\', "/"))
        .collect();
    if !existing_paths.is_subset(&declared_base_paths) {
        return Err("agent_base_hash_missing".into());
    }
    for base in &output.base_files {
        let relative = Path::new(&base.path);
        if relative.is_absolute()
            || relative
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err("agent_base_path_outside_snapshot".into());
        }
        let path = snapshot.join(relative);
        if !path.is_file() || sha256_file(&path) != base.sha256 {
            return Err("agent_base_hash_mismatch".into());
        }
    }
    Ok(output)
}

pub fn run_agent_repair(
    plan: &RunPlan,
    target_id: &str,
    provider: &str,
    repair_id: &str,
    cancelled: Arc<AtomicBool>,
) -> Result<VerifiedAgentRepair, String> {
    let capability = agent_capabilities()
        .into_iter()
        .find(|capability| capability.provider == provider)
        .ok_or_else(|| "agent_not_installed".to_string())?;
    if !capability.usable_in_app {
        return Err("agent_capability_not_certified".into());
    }
    let entry = cli_entry(provider).ok_or_else(|| "agent_not_installed".to_string())?;
    let target = plan
        .targets
        .iter()
        .find(|target| target.id == target_id)
        .ok_or_else(|| "agent_target_not_found".to_string())?;
    let (agent_snapshot, verification_snapshot) = snapshots_for_agent(plan, repair_id)?;
    let repair_root = agent_snapshot.parent().unwrap_or(&agent_snapshot);
    let schema_path = repair_root.join("repair-schema.json");
    let result_path = repair_root.join("repair-result.json");
    fs::write(&schema_path, capability_schema())
        .map_err(|_| "agent_repair_workspace_failed".to_string())?;
    let blocker = target
        .blockers
        .first()
        .map(|blocker| format!("{}: {}", blocker.code, blocker.summary))
        .unwrap_or_else(|| "the first observed verification blocker".into());
    let prompt = format!(
        "Repair only {blocker}. Work only inside the supplied isolated snapshot. Never read secrets or paths outside it. Return one JSON object matching the schema: a unified diff, concise evidence, and base_files with relative path plus SHA-256 for every existing file changed. Do not claim success; Verity will re-run deterministic verification."
    );
    let schema_arg = schema_path.to_string_lossy().to_string();
    let result_arg = result_path.to_string_lossy().to_string();
    let snapshot_arg = agent_snapshot.to_string_lossy().to_string();
    let command = if provider == "codex" {
        configure_command(
            &entry.path,
            &[
                "exec",
                "--ephemeral",
                "--ignore-user-config",
                "--sandbox",
                "workspace-write",
                "--output-schema",
                schema_arg.as_str(),
                "--output-last-message",
                result_arg.as_str(),
                "--color",
                "never",
                "-C",
                snapshot_arg.as_str(),
                prompt.as_str(),
            ],
        )
    } else {
        configure_command(
            &entry.path,
            &[
                "--print",
                "--bare",
                "--no-session-persistence",
                "--output-format",
                "json",
                "--json-schema",
                capability_schema(),
                "--tools",
                "Read,Grep,Glob,Edit,Write",
                "--permission-mode",
                "acceptEdits",
                prompt.as_str(),
            ],
        )
    };
    let output = wait_with_timeout(
        spawn_with_clean_environment(command, &agent_snapshot)?,
        Duration::from_secs(600),
        Some(&cancelled),
    )?;
    if !output.status.success() {
        return Err("agent_repair_process_failed".into());
    }
    let direct = if provider == "codex" {
        fs::read_to_string(&result_path).map_err(|_| "agent_repair_result_missing".to_string())?
    } else {
        let envelope: serde_json::Value = serde_json::from_slice(&output.stdout)
            .map_err(|_| "agent_repair_result_invalid".to_string())?;
        envelope
            .get("structured_output")
            .cloned()
            .or_else(|| {
                envelope
                    .get("result")
                    .and_then(|value| value.as_str())
                    .and_then(|value| serde_json::from_str(value).ok())
            })
            .ok_or_else(|| "agent_repair_result_invalid".to_string())?
            .to_string()
    };
    let result: AgentRepairOutput =
        serde_json::from_str(&direct).map_err(|_| "agent_repair_result_invalid".to_string())?;
    let result = validate_repair_output(&verification_snapshot, result)?;
    let patch_path = repair_root.join("candidate.patch");
    fs::write(&patch_path, &result.unified_diff)
        .map_err(|_| "agent_patch_write_failed".to_string())?;
    let check = Command::new("git")
        .current_dir(&verification_snapshot)
        .args(["apply", "--check", "--whitespace=nowarn"])
        .arg(&patch_path)
        .output()
        .map_err(|_| "agent_patch_check_failed".to_string())?;
    if !check.status.success() {
        return Err("agent_patch_check_failed".into());
    }
    let applied = Command::new("git")
        .current_dir(&verification_snapshot)
        .args(["apply", "--whitespace=nowarn"])
        .arg(&patch_path)
        .output()
        .map_err(|_| "agent_patch_apply_failed".to_string())?;
    if !applied.status.success() {
        return Err("agent_patch_apply_failed".into());
    }
    if target.commands.iter().any(|command| command.native) {
        return Err("agent_native_reverification_requires_confirmation".into());
    }
    let mut verification_plan = plan.clone();
    verification_plan.repository_root = verification_snapshot.to_string_lossy().to_string();
    let receipt = crate::execute_target(
        &verification_plan,
        target_id,
        &format!("agent-repair-{repair_id}"),
        &cancelled,
        |_| {},
    )
    .map_err(|_| "agent_reverification_failed".to_string())?;
    if !matches!(
        receipt.result,
        TargetResult::Verified | TargetResult::StartedUnverified
    ) {
        return Err("agent_reverification_blocked".into());
    }
    Ok(VerifiedAgentRepair {
        output: result,
        result: receipt.result,
        receipt_id: receipt.id,
    })
}

pub fn apply_verified_agent_repair(
    repository_root: &Path,
    output: &AgentRepairOutput,
) -> Result<bool, String> {
    let output = validate_repair_output(repository_root, output.clone())?;
    let patch_dir = verity_data_dir().join("agent-repairs").join("writeback");
    fs::create_dir_all(&patch_dir).map_err(|_| "agent_patch_write_failed".to_string())?;
    let patch_path = patch_dir.join(format!("{}.patch", uuid::Uuid::new_v4()));
    fs::write(&patch_path, &output.unified_diff)
        .map_err(|_| "agent_patch_write_failed".to_string())?;
    let check = Command::new("git")
        .current_dir(repository_root)
        .args(["apply", "--check", "--whitespace=nowarn"])
        .arg(&patch_path)
        .output()
        .map_err(|_| "agent_patch_check_failed".to_string())?;
    if !check.status.success() {
        return Err("agent_patch_conflicts_with_repository".into());
    }
    let applied = Command::new("git")
        .current_dir(repository_root)
        .args(["apply", "--whitespace=nowarn"])
        .arg(&patch_path)
        .output()
        .map_err(|_| "agent_patch_apply_failed".to_string())?;
    if !applied.status.success() {
        return Err("agent_patch_apply_failed".into());
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_provider_has_an_explicit_installation_state() {
        let providers = agent_capabilities();
        assert_eq!(providers.len(), 4);
        assert!(providers
            .iter()
            .all(|provider| !provider.installations.is_empty()));
        assert!(providers
            .iter()
            .all(|provider| provider.schema == "verity-agent-repair.v2"));
    }

    #[test]
    fn capability_schema_forbids_unexpected_fields() {
        let value: serde_json::Value = serde_json::from_str(capability_schema()).unwrap();
        assert_eq!(value["additionalProperties"], false);
        assert_eq!(value["required"].as_array().unwrap().len(), 3);
    }

    #[test]
    fn codex_probe_can_run_in_a_non_git_isolated_directory() {
        let args = codex_capability_args("schema.json", "result.json", "probe", "prompt");
        assert!(args.iter().any(|value| value == "--skip-git-repo-check"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn timeout_waiter_drains_large_child_output() {
        let mut command = Command::new("powershell.exe");
        command
            .args([
                "-NoProfile",
                "-Command",
                "[Console]::Out.Write(('x' * 200000))",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let child = command.spawn().unwrap();

        let output = wait_with_timeout(child, Duration::from_secs(10), None).unwrap();

        assert!(output.status.success());
        assert_eq!(output.stdout.len(), 200_000);
    }

    #[test]
    fn rejects_patch_paths_outside_snapshot() {
        let temp = TempDir::new().unwrap();
        let result = AgentRepairOutput {
            unified_diff: "--- a/../../secret\n+++ b/../../secret\n".into(),
            evidence: vec![],
            base_files: vec![],
        };
        assert_eq!(
            validate_repair_output(temp.path(), result).unwrap_err(),
            "agent_diff_path_outside_snapshot"
        );
    }

    #[test]
    fn requires_a_hash_for_every_existing_file_in_the_patch() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("README.md"), "before\n").unwrap();
        let result = AgentRepairOutput {
            unified_diff: "--- a/README.md\n+++ b/README.md\n@@ -1 +1 @@\n-before\n+after\n".into(),
            evidence: vec![],
            base_files: vec![],
        };
        assert_eq!(
            validate_repair_output(temp.path(), result).unwrap_err(),
            "agent_base_hash_missing"
        );
    }

    #[test]
    fn accepts_a_patch_only_when_the_declared_base_hash_matches() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("README.md");
        fs::write(&file, "before\n").unwrap();
        let result = AgentRepairOutput {
            unified_diff: "--- a/README.md\n+++ b/README.md\n@@ -1 +1 @@\n-before\n+after\n".into(),
            evidence: vec!["corrected fixture".into()],
            base_files: vec![verity_core::AgentBaseFile {
                path: "README.md".into(),
                sha256: sha256_file(&file),
            }],
        };
        assert!(validate_repair_output(temp.path(), result).is_ok());
    }
}
