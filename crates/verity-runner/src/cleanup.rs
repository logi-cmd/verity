// SPDX-License-Identifier: MPL-2.0

use crate::{
    docker_host_path, execute_target, execute_target_confirmed_native, host_command_version,
    run_process, snapshot, verity_data_dir,
};
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use thiserror::Error;
use uuid::Uuid;
use verity_core::{
    copyable_files, CleanupAnalyzerState, CleanupAnalyzerStatus, CleanupBaseFile, CleanupCandidate,
    CleanupCandidateKind, CleanupEligibility, CleanupPreview, CleanupReceipt, CleanupSession,
    CleanupSessionStatus, ProjectStack, RunPlan, SnapshotLimits, TargetResult, VerificationReceipt,
    CLEANUP_CANDIDATE_SCHEMA, CLEANUP_PREVIEW_SCHEMA, CLEANUP_RECEIPT_SCHEMA,
    CLEANUP_SESSION_SCHEMA,
};

#[derive(Debug, Error)]
pub enum CleanupError {
    #[error("cleanup requires a verified baseline receipt")]
    BaselineNotVerified,
    #[error("the baseline receipt does not match this plan and target")]
    BaselineMismatch,
    #[error("cleanup candidate was not found: {0}")]
    CandidateNotFound(String),
    #[error("protected or report-only candidates cannot be applied")]
    CandidateNotEligible,
    #[error("cleanup session was not found: {0}")]
    SessionNotFound(String),
    #[error("repository file changed after cleanup verification: {0}")]
    SourceChanged(String),
    #[error("cleanup was cancelled")]
    Cancelled,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("repository inspection failed: {0}")]
    Inspect(String),
    #[error("verification failed: {0}")]
    Verify(String),
}

fn normalized(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn hash_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn candidate_id(kind: &CleanupCandidateKind, path: &str, related: Option<&str>) -> String {
    let key = format!("{kind:?}:{path}:{}", related.unwrap_or(""));
    format!("cleanup-{}", &hash_bytes(key.as_bytes())[..16])
}

fn protected_path(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.starts_with("tests/")
        || lower.contains("/tests/")
        || lower.starts_with("test/")
        || lower.contains("/migrations/")
        || lower.starts_with(".github/")
        || lower.contains("/ci/")
        || lower.contains("/deploy")
        || lower.starts_with("tools/")
        || lower.contains("/ops/")
        || lower.contains("schema")
        || lower.contains("public_api")
        || lower.contains("/docs/")
        || lower.starts_with("docs/")
        || lower.ends_with("license")
        || lower.contains("license.")
        || lower.ends_with("notice")
        || lower.contains("notice.")
}

fn extract_godot_paths(text: &str, target_root: &str) -> Vec<String> {
    let mut paths = Vec::new();
    let mut rest = text;
    while let Some(index) = rest.find("res://") {
        rest = &rest[index + 6..];
        let end = rest
            .find(|character: char| {
                matches!(character, '"' | '\'' | ')' | ']' | '}' | ',' | '\r' | '\n')
            })
            .unwrap_or(rest.len());
        let relative = rest[..end].trim().trim_end_matches('\\').replace('\\', "/");
        if !relative.is_empty() && !relative.contains("..") {
            paths.push(if target_root.is_empty() {
                relative
            } else {
                format!("{target_root}/{relative}")
            });
        }
        rest = &rest[end..];
    }
    paths.sort();
    paths.dedup();
    paths
}

fn godot_unreferenced_candidates(
    plan: &RunPlan,
    baseline: &VerificationReceipt,
    files: &[(String, Vec<u8>)],
) -> Vec<CleanupCandidate> {
    let Some(target) = plan
        .targets
        .iter()
        .find(|target| target.id == baseline.target_id)
    else {
        return Vec::new();
    };
    if target.stack != ProjectStack::Godot {
        return Vec::new();
    }
    let target_root = target.relative_root.trim_end_matches('/');
    let mut references = BTreeMap::<String, Vec<String>>::new();
    let mut roots = BTreeSet::new();
    for (path, bytes) in files {
        let Ok(text) = std::str::from_utf8(bytes) else {
            continue;
        };
        let refs = extract_godot_paths(text, target_root);
        if path.ends_with("project.godot") || path.ends_with("export_presets.cfg") {
            roots.extend(refs.iter().cloned());
        }
        references.insert(path.clone(), refs);
    }
    if roots.is_empty() {
        return Vec::new();
    }
    let mut reachable = roots.clone();
    let mut queue = roots.into_iter().collect::<VecDeque<_>>();
    while let Some(path) = queue.pop_front() {
        for next in references.get(&path).into_iter().flatten() {
            if reachable.insert(next.clone()) {
                queue.push_back(next.clone());
            }
        }
    }
    let eligible_extension = |path: &str| {
        [
            ".gd",
            ".tscn",
            ".tres",
            ".gdshader",
            ".png",
            ".jpg",
            ".jpeg",
            ".svg",
            ".wav",
            ".ogg",
        ]
        .iter()
        .any(|extension| path.to_ascii_lowercase().ends_with(extension))
    };
    files
        .iter()
        .filter(|(path, _)| {
            (target_root.is_empty() || path.starts_with(&format!("{target_root}/")))
                && eligible_extension(path)
                && !reachable.contains(path)
        })
        .map(|(path, bytes)| {
            make_candidate(
                CleanupCandidateKind::UnreferencedResource,
                path.clone(),
                None,
                bytes.len() as u64,
                "verity-godot-resource-graph",
                vec!["No literal res:// path from the declared main scene, autoloads, export presets, or their transitive resources reaches this file.".into()],
                baseline.result == TargetResult::Verified,
            )
        })
        .collect()
}

fn backup_artifact(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with('~')
        || [".bak", ".backup", ".old", ".orig", ".rej", ".tmp"]
            .iter()
            .any(|suffix| lower.ends_with(suffix))
}

fn make_candidate(
    kind: CleanupCandidateKind,
    path: String,
    related_path: Option<String>,
    size_bytes: u64,
    analyzer: &str,
    evidence: Vec<String>,
    verified_baseline: bool,
) -> CleanupCandidate {
    let protected = protected_path(&path);
    let eligibility = if protected {
        CleanupEligibility::Protected
    } else if verified_baseline {
        CleanupEligibility::ReverificationRequired
    } else {
        CleanupEligibility::ReportOnly
    };
    CleanupCandidate {
        schema: CLEANUP_CANDIDATE_SCHEMA.into(),
        id: candidate_id(&kind, &path, related_path.as_deref()),
        kind,
        path,
        related_path,
        size_bytes,
        analyzer: analyzer.into(),
        evidence,
        risk_reason: if protected {
            "Protected project surface; report only even when an analyzer reports it.".into()
        } else {
            "Static evidence is not proof of safe removal; the unchanged baseline oracle must pass again.".into()
        },
        eligibility,
    }
}

fn report_candidate(
    kind: CleanupCandidateKind,
    path: String,
    related_path: Option<String>,
    size_bytes: u64,
    analyzer: &str,
    evidence: Vec<String>,
    risk_reason: &str,
) -> CleanupCandidate {
    let mut candidate = make_candidate(
        kind,
        path,
        related_path,
        size_bytes,
        analyzer,
        evidence,
        false,
    );
    candidate.eligibility = CleanupEligibility::ReportOnly;
    candidate.risk_reason = risk_reason.into();
    candidate
}

fn analyzer_status(
    analyzer: &str,
    state: CleanupAnalyzerState,
    version: impl Into<String>,
    reason_code: &str,
    finding_count: usize,
) -> CleanupAnalyzerStatus {
    CleanupAnalyzerStatus {
        analyzer: analyzer.into(),
        state,
        version: version.into(),
        reason_code: reason_code.into(),
        finding_count,
    }
}

fn analyzer_snapshot(baseline: &VerificationReceipt) -> Option<PathBuf> {
    let path = verity_data_dir()
        .join("sessions")
        .join(&baseline.session_id)
        .join("snapshot");
    path.is_dir().then_some(path)
}

fn file_size(root: &Path, relative: &str) -> u64 {
    fs::metadata(root.join(relative.replace('/', std::path::MAIN_SEPARATOR_STR)))
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}

fn safe_relative(root: &Path, value: &str) -> Option<String> {
    let value = value.replace('\\', "/");
    let path = Path::new(&value);
    let relative = if path.is_absolute() {
        path.strip_prefix(root).ok()?.to_path_buf()
    } else {
        path.to_path_buf()
    };
    if relative
        .components()
        .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return None;
    }
    Some(relative.to_string_lossy().replace('\\', "/"))
}

fn knip_analysis(
    target_root: &Path,
    snapshot_root: &Path,
    verified: bool,
) -> (Vec<CleanupCandidate>, CleanupAnalyzerStatus) {
    let package = target_root.join("package.json");
    if !package.is_file() {
        return (
            Vec::new(),
            analyzer_status(
                "knip",
                CleanupAnalyzerState::NotApplicable,
                "",
                "not_a_javascript_target",
                0,
            ),
        );
    }
    let executable = target_root.join("node_modules/knip/bin/knip.js");
    if !executable.is_file() {
        return (
            Vec::new(),
            analyzer_status(
                "knip",
                CleanupAnalyzerState::NotInstalled,
                "",
                "knip_not_in_verified_dependency_graph",
                0,
            ),
        );
    }
    let dynamic_config = ["knip.js", "knip.ts", "knip.config.js", "knip.config.ts"]
        .iter()
        .any(|name| target_root.join(name).is_file());
    let static_config = ["knip.json", "knip.jsonc", ".knip.json", ".knip.jsonc"]
        .iter()
        .any(|name| target_root.join(name).is_file())
        || fs::read(&package)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .is_some_and(|value| value.get("knip").is_some());
    let relative_root = target_root
        .strip_prefix(snapshot_root)
        .unwrap_or(Path::new(""));
    let workdir = if relative_root.as_os_str().is_empty() {
        "/workspace".into()
    } else {
        format!(
            "/workspace/{}",
            relative_root.to_string_lossy().replace('\\', "/")
        )
    };
    let source = docker_host_path(snapshot_root);
    let args = vec![
        "run".into(),
        "--rm".into(),
        "--network".into(),
        "none".into(),
        "--cpus".into(),
        "1".into(),
        "--memory".into(),
        "2g".into(),
        "--pids-limit".into(),
        "256".into(),
        "--mount".into(),
        format!("type=bind,source={source},target=/workspace"),
        "--workdir".into(),
        workdir,
        "node:22-bookworm-slim".into(),
        "node".into(),
        "./node_modules/knip/bin/knip.js".into(),
        "--reporter".into(),
        "json".into(),
        "--include".into(),
        "files,dependencies,exports,types".into(),
        "--no-progress".into(),
        "--no-exit-code".into(),
    ];
    let cancelled = AtomicBool::new(false);
    let result = match run_process(
        "docker",
        &args,
        Path::new("."),
        Duration::from_secs(120),
        &cancelled,
    ) {
        Ok(result) if result.code == Some(0) => result,
        _ => {
            return (
                Vec::new(),
                analyzer_status(
                    "knip",
                    CleanupAnalyzerState::Failed,
                    "",
                    "knip_execution_failed",
                    0,
                ),
            )
        }
    };
    let value: serde_json::Value = match serde_json::from_str(&result.output) {
        Ok(value) => value,
        Err(_) => {
            return (
                Vec::new(),
                analyzer_status(
                    "knip",
                    CleanupAnalyzerState::Failed,
                    "",
                    "knip_invalid_json",
                    0,
                ),
            )
        }
    };
    let mut candidates = Vec::new();
    for issue in value
        .get("issues")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
    {
        for file in issue
            .get("files")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
        {
            let Some(path) = file
                .get("name")
                .and_then(serde_json::Value::as_str)
                .and_then(|value| safe_relative(target_root, value))
            else {
                continue;
            };
            let path = if relative_root.as_os_str().is_empty() {
                path
            } else {
                format!(
                    "{}/{}",
                    relative_root.to_string_lossy().replace('\\', "/"),
                    path
                )
            };
            let mut candidate = make_candidate(CleanupCandidateKind::UnusedFile, path.clone(), None, file_size(snapshot_root, &path), "knip", vec!["Knip's entry graph reports this file as unreachable from configured project entries.".into()], verified && static_config && !dynamic_config);
            if !static_config || dynamic_config {
                candidate.eligibility = CleanupEligibility::ReportOnly;
                candidate.risk_reason = "Knip entry configuration is absent or executable; the finding remains report-only until entry coverage is explicit.".into();
            }
            candidates.push(candidate);
        }
        for key in [
            "dependencies",
            "devDependencies",
            "optionalPeerDependencies",
            "exports",
            "types",
        ] {
            for item in issue
                .get(key)
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
            {
                let Some(name) = item.get("name").and_then(serde_json::Value::as_str) else {
                    continue;
                };
                let source_file = issue
                    .get("file")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("package.json");
                let path = safe_relative(target_root, source_file)
                    .unwrap_or_else(|| "package.json".into());
                let path = if relative_root.as_os_str().is_empty() {
                    path
                } else {
                    format!(
                        "{}/{}",
                        relative_root.to_string_lossy().replace('\\', "/"),
                        path
                    )
                };
                candidates.push(report_candidate(if key.contains("ependenc") { CleanupCandidateKind::UnusedDependency } else { CleanupCandidateKind::UnusedSymbol }, path, Some(name.into()), 0, "knip", vec![format!("Knip reports unused {key} item '{name}'.")], "Manifest and symbol edits require a separately generated diff; this finding is never converted into a file deletion."));
            }
        }
    }
    candidates.sort_by(|a, b| a.path.cmp(&b.path).then(a.id.cmp(&b.id)));
    candidates.dedup_by(|a, b| a.id == b.id);
    let count = candidates.len();
    let state = if dynamic_config {
        CleanupAnalyzerState::UnsafeConfiguration
    } else {
        CleanupAnalyzerState::Completed
    };
    let reason = if dynamic_config {
        "knip_dynamic_config_report_only"
    } else {
        "knip_completed"
    };
    (
        candidates,
        analyzer_status("knip", state, "project dependency", reason, count),
    )
}

fn cargo_machete_analysis(
    target_root: &Path,
    snapshot_root: &Path,
) -> (Vec<CleanupCandidate>, CleanupAnalyzerStatus) {
    if !target_root.join("Cargo.toml").is_file() {
        return (
            Vec::new(),
            analyzer_status(
                "cargo-machete",
                CleanupAnalyzerState::NotApplicable,
                "",
                "not_a_rust_target",
                0,
            ),
        );
    }
    let Some(version) = host_command_version("cargo", &["machete", "--version"]) else {
        return (
            Vec::new(),
            analyzer_status(
                "cargo-machete",
                CleanupAnalyzerState::NotInstalled,
                "",
                "cargo_machete_not_installed",
                0,
            ),
        );
    };
    let cancelled = AtomicBool::new(false);
    let args = vec![
        "machete".into(),
        "--json".into(),
        target_root.display().to_string(),
    ];
    let result = match run_process(
        "cargo",
        &args,
        snapshot_root,
        Duration::from_secs(120),
        &cancelled,
    ) {
        Ok(result) if matches!(result.code, Some(0) | Some(1)) => result,
        _ => {
            return (
                Vec::new(),
                analyzer_status(
                    "cargo-machete",
                    CleanupAnalyzerState::Failed,
                    version,
                    "cargo_machete_execution_failed",
                    0,
                ),
            )
        }
    };
    let value: serde_json::Value = match serde_json::from_str(&result.output) {
        Ok(value) => value,
        Err(_) => {
            return (
                Vec::new(),
                analyzer_status(
                    "cargo-machete",
                    CleanupAnalyzerState::Failed,
                    version,
                    "cargo_machete_invalid_json",
                    0,
                ),
            )
        }
    };
    let mut candidates = Vec::new();
    for krate in value
        .get("crates")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
    {
        let path = krate
            .get("manifest_path")
            .and_then(serde_json::Value::as_str)
            .and_then(|value| safe_relative(snapshot_root, value))
            .unwrap_or_else(|| "Cargo.toml".into());
        for name in krate
            .get("unused")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
        {
            candidates.push(report_candidate(CleanupCandidateKind::UnusedDependency, path.clone(), Some(name.into()), 0, "cargo-machete", vec![format!("cargo-machete reports dependency '{name}' as unused.")], "cargo-machete is intentionally fast and imprecise; dependency removal remains report-only."));
        }
    }
    let count = candidates.len();
    (
        candidates,
        analyzer_status(
            "cargo-machete",
            CleanupAnalyzerState::Completed,
            version,
            "cargo_machete_completed",
            count,
        ),
    )
}

fn vulture_analysis(
    target_root: &Path,
    snapshot_root: &Path,
) -> (Vec<CleanupCandidate>, CleanupAnalyzerStatus) {
    if !target_root.join("pyproject.toml").is_file()
        && !target_root.join("requirements.txt").is_file()
    {
        return (
            Vec::new(),
            analyzer_status(
                "vulture",
                CleanupAnalyzerState::NotApplicable,
                "",
                "not_a_python_target",
                0,
            ),
        );
    }
    let Some(version) = host_command_version("python", &["-m", "vulture", "--version"]) else {
        return (
            Vec::new(),
            analyzer_status(
                "vulture",
                CleanupAnalyzerState::NotInstalled,
                "",
                "vulture_not_installed",
                0,
            ),
        );
    };
    let cancelled = AtomicBool::new(false);
    let args = vec![
        "-m".into(),
        "vulture".into(),
        ".".into(),
        "--min-confidence".into(),
        "100".into(),
    ];
    let result = match run_process(
        "python",
        &args,
        target_root,
        Duration::from_secs(120),
        &cancelled,
    ) {
        Ok(result) if matches!(result.code, Some(0) | Some(3)) => result,
        _ => {
            return (
                Vec::new(),
                analyzer_status(
                    "vulture",
                    CleanupAnalyzerState::Failed,
                    version,
                    "vulture_execution_failed",
                    0,
                ),
            )
        }
    };
    let target_relative = target_root
        .strip_prefix(snapshot_root)
        .unwrap_or(Path::new(""));
    let mut candidates = Vec::new();
    for line in result
        .output
        .lines()
        .filter(|line| line.contains("100% confidence"))
    {
        let Some((path_part, detail)) = line.split_once(':') else {
            continue;
        };
        let Some(path) = safe_relative(target_root, path_part) else {
            continue;
        };
        let path = if target_relative.as_os_str().is_empty() {
            path
        } else {
            format!(
                "{}/{}",
                target_relative.to_string_lossy().replace('\\', "/"),
                path
            )
        };
        candidates.push(report_candidate(CleanupCandidateKind::UnusedSymbol, path, None, 0, "vulture", vec![detail.trim().into()], "Python dynamic dispatch and framework hooks prevent symbol-level automatic deletion; Vulture findings remain report-only."));
    }
    let count = candidates.len();
    (
        candidates,
        analyzer_status(
            "vulture",
            CleanupAnalyzerState::Completed,
            version,
            "vulture_completed",
            count,
        ),
    )
}

fn deadcode_analysis(
    target_root: &Path,
    snapshot_root: &Path,
) -> (Vec<CleanupCandidate>, CleanupAnalyzerStatus) {
    if !target_root.join("go.mod").is_file() {
        return (
            Vec::new(),
            analyzer_status(
                "go-deadcode",
                CleanupAnalyzerState::NotApplicable,
                "",
                "not_a_go_target",
                0,
            ),
        );
    }
    let cancelled = AtomicBool::new(false);
    let args = vec!["-json".into(), "-test".into(), "./...".into()];
    let result = match run_process(
        "deadcode",
        &args,
        target_root,
        Duration::from_secs(180),
        &cancelled,
    ) {
        Ok(result) if result.code == Some(0) => result,
        Err(_) => {
            return (
                Vec::new(),
                analyzer_status(
                    "go-deadcode",
                    CleanupAnalyzerState::NotInstalled,
                    "",
                    "go_deadcode_not_installed",
                    0,
                ),
            )
        }
        _ => {
            return (
                Vec::new(),
                analyzer_status(
                    "go-deadcode",
                    CleanupAnalyzerState::Failed,
                    "installed",
                    "go_deadcode_execution_failed",
                    0,
                ),
            )
        }
    };
    let value: serde_json::Value = match serde_json::from_str(&result.output) {
        Ok(value) => value,
        Err(_) => {
            return (
                Vec::new(),
                analyzer_status(
                    "go-deadcode",
                    CleanupAnalyzerState::Failed,
                    "installed",
                    "go_deadcode_invalid_json",
                    0,
                ),
            )
        }
    };
    let target_relative = target_root
        .strip_prefix(snapshot_root)
        .unwrap_or(Path::new(""));
    let mut candidates = Vec::new();
    for package in value.as_array().into_iter().flatten() {
        for function in package
            .get("Funcs")
            .or_else(|| package.get("funcs"))
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
        {
            let position = function
                .get("Position")
                .or_else(|| function.get("position"));
            let file = position
                .and_then(|item| item.get("File").or_else(|| item.get("file")))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("");
            let Some(path) = safe_relative(target_root, file) else {
                continue;
            };
            let path = if target_relative.as_os_str().is_empty() {
                path
            } else {
                format!(
                    "{}/{}",
                    target_relative.to_string_lossy().replace('\\', "/"),
                    path
                )
            };
            let name = function
                .get("Name")
                .or_else(|| function.get("name"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("function");
            candidates.push(report_candidate(CleanupCandidateKind::UnusedSymbol, path, Some(name.into()), 0, "go-deadcode", vec![format!("deadcode -test reports function '{name}' as unreachable for this GOOS/GOARCH configuration.")], "Go deadcode results are configuration-specific and cannot protect external callers, assembly, or go:linkname edges; findings remain report-only."));
        }
    }
    let count = candidates.len();
    (
        candidates,
        analyzer_status(
            "go-deadcode",
            CleanupAnalyzerState::Completed,
            "installed",
            "go_deadcode_completed",
            count,
        ),
    )
}

fn missing_snapshot_status(analyzer: &str, applicable: bool) -> CleanupAnalyzerStatus {
    if applicable {
        analyzer_status(
            analyzer,
            CleanupAnalyzerState::Failed,
            "",
            "verified_snapshot_unavailable",
            0,
        )
    } else {
        analyzer_status(
            analyzer,
            CleanupAnalyzerState::NotApplicable,
            "",
            "not_applicable_to_selected_target",
            0,
        )
    }
}

pub fn preview_cleanup(
    plan: &RunPlan,
    baseline: &VerificationReceipt,
) -> Result<CleanupPreview, CleanupError> {
    if baseline.target_id
        != plan
            .targets
            .iter()
            .find(|target| target.id == baseline.target_id)
            .map(|target| target.id.as_str())
            .unwrap_or("")
        || baseline.repository_fingerprint.is_empty()
    {
        return Err(CleanupError::BaselineMismatch);
    }
    let verified = baseline.result == TargetResult::Verified;
    let root = Path::new(&plan.repository_root);
    let files = copyable_files(root, SnapshotLimits::default())
        .map_err(|error| CleanupError::Inspect(error.to_string()))?;
    let mut by_hash: BTreeMap<String, Vec<(String, u64)>> = BTreeMap::new();
    let mut source_files = Vec::new();
    let mut candidates = Vec::new();
    for file in files {
        let path = normalized(&file, root);
        let bytes = fs::read(&file)?;
        source_files.push((path.clone(), bytes.clone()));
        if backup_artifact(&path) {
            candidates.push(make_candidate(
                CleanupCandidateKind::ObsoleteArtifact,
                path.clone(),
                None,
                bytes.len() as u64,
                "verity-obsolete-artifact",
                vec!["Filename matches a conventional backup/conflict residue suffix.".into()],
                verified,
            ));
        }
        if !bytes.is_empty() {
            by_hash
                .entry(hash_bytes(&bytes))
                .or_default()
                .push((path, bytes.len() as u64));
        }
    }
    for (digest, mut group) in by_hash {
        if group.len() < 2 {
            continue;
        }
        group.sort_by(|a, b| a.0.cmp(&b.0));
        let keeper = group[0].0.clone();
        for (path, size) in group.into_iter().skip(1) {
            candidates.push(make_candidate(
                CleanupCandidateKind::DuplicateFile,
                path,
                Some(keeper.clone()),
                size,
                "verity-exact-content",
                vec![format!("SHA-256 {digest} exactly matches {keeper}.")],
                false,
            ));
        }
    }
    candidates.extend(godot_unreferenced_candidates(plan, baseline, &source_files));

    let target = plan
        .targets
        .iter()
        .find(|target| target.id == baseline.target_id)
        .ok_or(CleanupError::BaselineMismatch)?;
    let mut analyzers = Vec::new();
    if let Some(snapshot_root) = analyzer_snapshot(baseline) {
        let target_root = snapshot_root.join(&target.relative_root);
        for (mut findings, status) in [
            knip_analysis(&target_root, &snapshot_root, verified),
            cargo_machete_analysis(&target_root, &snapshot_root),
            vulture_analysis(&target_root, &snapshot_root),
            deadcode_analysis(&target_root, &snapshot_root),
        ] {
            candidates.append(&mut findings);
            analyzers.push(status);
        }
    } else {
        analyzers.extend([
            missing_snapshot_status("knip", matches!(target.stack, ProjectStack::Node)),
            missing_snapshot_status("cargo-machete", matches!(target.stack, ProjectStack::Rust)),
            missing_snapshot_status("vulture", matches!(target.stack, ProjectStack::Python)),
            missing_snapshot_status("go-deadcode", matches!(target.stack, ProjectStack::Go)),
        ]);
    }
    candidates.sort_by(|a, b| a.path.cmp(&b.path).then(a.id.cmp(&b.id)));
    candidates.dedup_by(|a, b| a.id == b.id);
    Ok(CleanupPreview {
        schema: CLEANUP_PREVIEW_SCHEMA.into(),
        candidates,
        analyzers,
    })
}

fn cleanup_dir(id: &str) -> PathBuf {
    verity_data_dir().join("cleanup").join(id)
}

fn save_session(session: &CleanupSession) -> Result<(), CleanupError> {
    let dir = cleanup_dir(&session.id);
    fs::create_dir_all(&dir)?;
    fs::write(
        dir.join("session.json"),
        serde_json::to_vec_pretty(session)?,
    )?;
    Ok(())
}

pub fn read_cleanup_session(id: &str) -> Result<CleanupSession, CleanupError> {
    let path = cleanup_dir(id).join("session.json");
    if !path.is_file() {
        return Err(CleanupError::SessionNotFound(id.into()));
    }
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

pub fn read_cleanup_receipts(id: &str) -> Result<Vec<CleanupReceipt>, CleanupError> {
    let dir = cleanup_dir(id).join("receipts");
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut receipts = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter_map(|entry| fs::read(entry.path()).ok())
        .filter_map(|bytes| serde_json::from_slice(&bytes).ok())
        .collect::<Vec<_>>();
    receipts.sort_by(|a: &CleanupReceipt, b: &CleanupReceipt| a.created_at.cmp(&b.created_at));
    Ok(receipts)
}

fn attempt_group(
    plan: &RunPlan,
    target_id: &str,
    candidates: &[CleanupCandidate],
    cancelled: &AtomicBool,
) -> Result<Option<VerificationReceipt>, CleanupError> {
    if cancelled.load(Ordering::SeqCst) {
        return Err(CleanupError::Cancelled);
    }
    let attempt_id = format!("cleanup-attempt-{}", Uuid::new_v4());
    let snapshot_root =
        snapshot(plan, &attempt_id).map_err(|error| CleanupError::Verify(error.to_string()))?;
    for candidate in candidates {
        let path = snapshot_root.join(&candidate.path);
        if path.is_file() {
            fs::remove_file(path)?;
        }
    }
    let mut attempt_plan = verity_adapters::inspect_repository(&snapshot_root)
        .map_err(|error| CleanupError::Inspect(error.to_string()))?;
    crate::assess_plan_environment(&mut attempt_plan);
    let target = attempt_plan
        .targets
        .iter()
        .find(|target| target.id == target_id)
        .ok_or_else(|| {
            CleanupError::Inspect("cleanup changed the selected target identity".into())
        })?;
    let result = if target.commands.iter().any(|command| command.native) {
        execute_target_confirmed_native(&attempt_plan, target_id, &attempt_id, cancelled, |_| {})
    } else {
        execute_target(&attempt_plan, target_id, &attempt_id, cancelled, |_| {})
    }
    .map_err(|error| CleanupError::Verify(error.to_string()))?;
    Ok((result.result == TargetResult::Verified).then_some(result))
}

fn isolate_verified(
    plan: &RunPlan,
    target_id: &str,
    candidates: &[CleanupCandidate],
    cancelled: &AtomicBool,
    verified: &mut Vec<(Vec<CleanupCandidate>, VerificationReceipt)>,
) -> Result<(), CleanupError> {
    if candidates.is_empty() {
        return Ok(());
    }
    if let Some(receipt) = attempt_group(plan, target_id, candidates, cancelled)? {
        verified.push((candidates.to_vec(), receipt));
        return Ok(());
    }
    if candidates.len() == 1 {
        return Ok(());
    }
    let midpoint = candidates.len() / 2;
    isolate_verified(
        plan,
        target_id,
        &candidates[..midpoint],
        cancelled,
        verified,
    )?;
    isolate_verified(
        plan,
        target_id,
        &candidates[midpoint..],
        cancelled,
        verified,
    )
}

pub fn run_cleanup(
    plan: &RunPlan,
    baseline: &VerificationReceipt,
    candidate_ids: &[String],
    cleanup_session_id: Option<&str>,
    cancelled: &AtomicBool,
) -> Result<(CleanupSession, Vec<CleanupReceipt>), CleanupError> {
    if baseline.result != TargetResult::Verified {
        return Err(CleanupError::BaselineNotVerified);
    }
    if baseline.target_id.is_empty()
        || baseline.repository_fingerprint
            != verity_core::fingerprint_repository(
                Path::new(&plan.repository_root),
                SnapshotLimits::default(),
            )
            .map_err(|error| CleanupError::Inspect(error.to_string()))?
    {
        return Err(CleanupError::BaselineMismatch);
    }
    let all = preview_cleanup(plan, baseline)?.candidates;
    let mut selected = Vec::new();
    for id in candidate_ids {
        let candidate = all
            .iter()
            .find(|candidate| &candidate.id == id)
            .cloned()
            .ok_or_else(|| CleanupError::CandidateNotFound(id.clone()))?;
        if candidate.eligibility != CleanupEligibility::ReverificationRequired {
            return Err(CleanupError::CandidateNotEligible);
        }
        selected.push(candidate);
    }
    let now = Utc::now().to_rfc3339();
    let mut session = CleanupSession {
        schema: CLEANUP_SESSION_SCHEMA.into(),
        id: cleanup_session_id
            .map(str::to_string)
            .unwrap_or_else(|| Uuid::new_v4().to_string()),
        repository_root: plan.repository_root.clone(),
        target_id: baseline.target_id.clone(),
        baseline_receipt_id: baseline.id.clone(),
        status: CleanupSessionStatus::Revalidating,
        candidates: all,
        verified_candidate_ids: Vec::new(),
        started_at: now.clone(),
        updated_at: now,
        error_code: None,
    };
    save_session(&session)?;
    let mut verified = Vec::new();
    isolate_verified(
        plan,
        &baseline.target_id,
        &selected,
        cancelled,
        &mut verified,
    )?;
    session.verified_candidate_ids = verified
        .iter()
        .flat_map(|(candidates, _)| candidates.iter().map(|candidate| candidate.id.clone()))
        .collect();
    session.verified_candidate_ids.sort();
    session.verified_candidate_ids.dedup();
    for candidate in &mut session.candidates {
        if session.verified_candidate_ids.contains(&candidate.id) {
            candidate.eligibility = CleanupEligibility::RemovalVerified;
        }
    }
    session.status = if cancelled.load(Ordering::SeqCst) {
        CleanupSessionStatus::Cancelled
    } else {
        CleanupSessionStatus::Completed
    };
    session.updated_at = Utc::now().to_rfc3339();
    let mut receipts = Vec::new();
    for (candidates, verification) in verified {
        let mut base_files = Vec::new();
        let mut patches = Vec::new();
        for candidate in &candidates {
            let source = Path::new(&plan.repository_root).join(&candidate.path);
            let bytes = fs::read(&source)?;
            base_files.push(CleanupBaseFile {
                path: candidate.path.clone(),
                sha256: hash_bytes(&bytes),
            });
            patches.push(deletion_patch(&candidate.path, &bytes));
        }
        let receipt = CleanupReceipt {
            schema: CLEANUP_RECEIPT_SCHEMA.into(),
            id: Uuid::new_v4().to_string(),
            cleanup_session_id: session.id.clone(),
            baseline_receipt_id: baseline.id.clone(),
            verification_receipt_id: verification.id,
            target_id: baseline.target_id.clone(),
            candidate_ids: candidates.iter().map(|candidate| candidate.id.clone()).collect(),
            removed_files: candidates.iter().map(|candidate| candidate.path.clone()).collect(),
            removed_bytes: candidates.iter().map(|candidate| candidate.size_bytes).sum(),
            unified_diff: patches.join("\n"),
            base_files,
            conclusion: "This removal group passed the same recorded build, test, launch, and machine oracle as the verified baseline.".into(),
            created_at: Utc::now().to_rfc3339(),
        };
        let dir = cleanup_dir(&session.id).join("receipts");
        fs::create_dir_all(&dir)?;
        fs::write(
            dir.join(format!("{}.json", receipt.id)),
            serde_json::to_vec_pretty(&receipt)?,
        )?;
        receipts.push(receipt);
    }
    save_session(&session)?;
    Ok((session, receipts))
}

fn deletion_patch(path: &str, bytes: &[u8]) -> String {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return format!("--- a/{path}\n+++ /dev/null\nBinary file removal verified by SHA-256.\n");
    };
    let line_count = text.lines().count().max(1);
    let body = text
        .lines()
        .map(|line| format!("-{line}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!("--- a/{path}\n+++ /dev/null\n@@ -1,{line_count} +0,0 @@\n{body}\n")
}

pub fn export_cleanup_patch(
    receipt: &CleanupReceipt,
    destination: &Path,
) -> Result<(), CleanupError> {
    fs::write(destination, &receipt.unified_diff)?;
    Ok(())
}

pub fn apply_cleanup_receipt(
    receipt: &CleanupReceipt,
    repository_root: &Path,
) -> Result<(), CleanupError> {
    for base in &receipt.base_files {
        let path = repository_root.join(&base.path);
        let bytes = fs::read(&path).map_err(|_| CleanupError::SourceChanged(base.path.clone()))?;
        if hash_bytes(&bytes) != base.sha256 {
            return Err(CleanupError::SourceChanged(base.path.clone()));
        }
    }
    for base in &receipt.base_files {
        fs::remove_file(repository_root.join(&base.path))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_surfaces_never_become_writeback_candidates() {
        assert!(protected_path("tests/fixture.ts"));
        assert!(protected_path("docs/guide.md"));
        assert!(protected_path("tools/release.ps1"));
        assert!(protected_path("LICENSE"));
        assert!(!protected_path("src/legacy.ts.bak"));
    }

    #[test]
    fn godot_literal_resource_paths_are_normalized_without_escape() {
        assert_eq!(
            extract_godot_paths(
                "load(\"res://scenes/main.tscn\")\npreload('res://scripts/main.gd')",
                "game"
            ),
            vec!["game/scenes/main.tscn", "game/scripts/main.gd"]
        );
        assert!(extract_godot_paths("load('../secret')", "").is_empty());
    }

    #[test]
    fn deletion_patch_is_deterministic() {
        assert_eq!(
            deletion_patch("src/old.txt", b"one\ntwo\n"),
            "--- a/src/old.txt\n+++ /dev/null\n@@ -1,2 +0,0 @@\n-one\n-two\n"
        );
    }
}
