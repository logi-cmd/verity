// SPDX-License-Identifier: MPL-2.0

use chrono::Utc;
use globset::{Glob, GlobSetBuilder};
use ignore::WalkBuilder;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use verity_core::*;

#[derive(Debug, Error)]
pub enum InspectError {
    #[error(transparent)]
    Fingerprint(#[from] verity_core::FingerprintError),
    #[error("unable to read manifest {path}: {message}")]
    Manifest { path: String, message: String },
    #[error("no supported project manifest was found")]
    Unsupported,
}

fn evidence(path: &str, key: &str, precedence: u8) -> CommandEvidence {
    CommandEvidence {
        path: path.replace('\\', "/"),
        key: key.to_string(),
        precedence,
    }
}

fn command(
    phase: RunPhase,
    program: &str,
    args: &[&str],
    source: CommandEvidence,
    network: NetworkPolicy,
    native: bool,
) -> PlannedCommand {
    PlannedCommand {
        phase,
        program: program.to_string(),
        args: args.iter().map(|value| value.to_string()).collect(),
        relative_cwd: String::new(),
        evidence: source,
        network,
        native,
    }
}

fn target_id(stack: &ProjectStack, relative: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("{stack:?}:{relative}").as_bytes());
    format!(
        "{}-{}",
        format!("{stack:?}").to_lowercase(),
        &hex::encode(hasher.finalize())[..10]
    )
}

fn relative(root: &Path, path: &Path) -> String {
    path.parent()
        .unwrap_or(root)
        .strip_prefix(root)
        .unwrap_or_else(|_| Path::new(""))
        .to_string_lossy()
        .replace('\\', "/")
}

fn manifest_paths(root: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut result = Vec::new();
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(false)
        .parents(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .require_git(false)
        .follow_links(false)
        .filter_entry(|entry| {
            entry.depth() == 0
                || entry.file_name().to_str().is_none_or(|name| {
                    !matches!(
                        name,
                        ".git"
                            | ".worktrees"
                            | ".wrangler"
                            | ".agent-guardrails"
                            | ".agents"
                            | ".gstack"
                            | ".omx"
                            | "node_modules"
                            | "target"
                            | "dist"
                            | "build"
                            | "coverage"
                            | ".next"
                            | ".venv"
                            | "venv"
                            | ".cache"
                            | "__pycache__"
                            | "tmp"
                    )
                })
        });
    for entry in builder.build() {
        let entry = entry.map_err(|error| std::io::Error::other(error.to_string()))?;
        if !entry.file_type().is_some_and(|kind| kind.is_file()) {
            continue;
        }
        let name = entry.file_name().to_string_lossy();
        let extension = entry.path().extension().and_then(|value| value.to_str());
        if matches!(
            name.as_ref(),
            "package.json"
                | "deno.json"
                | "deno.jsonc"
                | "bun.lock"
                | "bun.lockb"
                | "Cargo.toml"
                | "pyproject.toml"
                | "requirements.txt"
                | "go.mod"
                | "project.godot"
                | "compose.yaml"
                | "compose.yml"
                | "docker-compose.yml"
                | "pom.xml"
                | "build.gradle"
                | "build.gradle.kts"
                | "CMakeLists.txt"
                | "meson.build"
                | "Makefile"
                | "composer.json"
                | "Gemfile"
        ) || matches!(extension, Some("sln" | "csproj" | "fsproj"))
        {
            result.push(entry.into_path());
        }
    }
    result.sort();
    result.dedup();
    Ok(result)
}

fn static_entry_paths(root: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut result = Vec::new();
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(false)
        .parents(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .require_git(false)
        .follow_links(false)
        .max_depth(Some(3));
    for entry in builder.build() {
        let entry = entry.map_err(|error| std::io::Error::other(error.to_string()))?;
        if entry.file_type().is_some_and(|kind| kind.is_file()) && entry.file_name() == "index.html"
        {
            let rel = entry
                .path()
                .strip_prefix(root)
                .unwrap_or(entry.path())
                .to_string_lossy()
                .replace('\\', "/");
            let parts = rel.split('/').collect::<Vec<_>>();
            let generated = parts.iter().any(|part| {
                matches!(*part, "fixtures" | "examples" | ".next" | "coverage")
                    || part.starts_with("dist")
                    || part.starts_with("build")
                    || part.starts_with("out")
            });
            let owned_by_manifest = entry.path().parent().is_some_and(|dir| {
                dir.ancestors()
                    .take_while(|ancestor| ancestor.starts_with(root))
                    .any(|ancestor| {
                        ancestor.join("package.json").is_file()
                            || ancestor.join("deno.json").is_file()
                            || ancestor.join("deno.jsonc").is_file()
                    })
            });
            if parts.len() <= 2 && !generated && !owned_by_manifest {
                result.push(entry.into_path());
            }
        }
    }
    result.sort();
    result.dedup();
    Ok(result)
}

fn target_role(relative_root: &str, kind: &ProjectKind) -> TargetRole {
    let segments = relative_root.split('/').collect::<Vec<_>>();
    if relative_root.contains("tests/fixtures") || segments.iter().any(|part| *part == "fixtures") {
        TargetRole::Fixture
    } else if segments.iter().any(|part| {
        matches!(*part, "examples" | "example" | "samples") || part.contains("prototype")
    }) {
        TargetRole::Example
    } else if segments
        .iter()
        .any(|part| matches!(*part, "crates" | "packages"))
    {
        TargetRole::Component
    } else {
        match kind {
            ProjectKind::Web | ProjectKind::Desktop | ProjectKind::Game => TargetRole::Product,
            ProjectKind::Service => TargetRole::Service,
            ProjectKind::Cli => TargetRole::Tool,
            ProjectKind::Library | ProjectKind::Unknown => TargetRole::Library,
        }
    }
}

fn oracle_status(oracle: &VerificationOracle) -> OracleStatus {
    if oracle.machine_verifiable {
        OracleStatus::Machine
    } else if oracle.kind != OracleKind::None {
        OracleStatus::Limited
    } else {
        OracleStatus::None
    }
}

fn component_for(target: &RunTarget) -> TargetComponent {
    TargetComponent {
        id: target.id.clone(),
        label: target.label.clone(),
        relative_root: target.relative_root.clone(),
        stack: target.stack.clone(),
        kind: target.kind.clone(),
        role: target.role.clone(),
    }
}

fn declarations(dir: &Path) -> Vec<String> {
    [
        "compose.yaml",
        "compose.yml",
        "docker-compose.yml",
        "Dockerfile",
        ".devcontainer/devcontainer.json",
    ]
    .into_iter()
    .filter(|name| dir.join(name).is_file())
    .map(str::to_string)
    .collect()
}

fn pinned_requirements(path: &Path) -> bool {
    fn inspect(path: &Path, visited: &mut Vec<PathBuf>) -> bool {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if visited.contains(&canonical) {
            return true;
        }
        visited.push(canonical);
        let Ok(raw) = fs::read_to_string(path) else {
            return false;
        };
        let mut saw_dependency = false;
        for line in raw.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(include) = line
                .strip_prefix("-r ")
                .or_else(|| line.strip_prefix("--requirement "))
                .or_else(|| line.strip_prefix("-c "))
                .or_else(|| line.strip_prefix("--constraint "))
            {
                if !inspect(
                    &path.parent().unwrap_or(Path::new(".")).join(include.trim()),
                    visited,
                ) {
                    return false;
                }
                saw_dependency = true;
                continue;
            }
            if line.starts_with("--") {
                continue;
            }
            saw_dependency = true;
            if !(line.contains("==") || (line.contains(" @ ") && line.contains("#sha256="))) {
                return false;
            }
        }
        saw_dependency
    }
    inspect(path, &mut Vec::new())
}

fn npm_lock_consistent(package: &Value, lock_path: &Path) -> Option<bool> {
    let lock: Value = serde_json::from_slice(&fs::read(lock_path).ok()?).ok()?;
    let packages = lock.get("packages").and_then(Value::as_object);
    let root = packages.and_then(|packages| packages.get(""));
    for section in ["dependencies", "devDependencies", "optionalDependencies"] {
        let declared = package.get(section).and_then(Value::as_object);
        let Some(declared) = declared else { continue };
        if let Some(root) = root {
            let locked = root.get(section).and_then(Value::as_object);
            if declared
                .iter()
                .any(|(name, spec)| locked.and_then(|values| values.get(name)) != Some(spec))
            {
                return Some(false);
            }
        } else {
            let locked = lock.get("dependencies").and_then(Value::as_object);
            if declared
                .keys()
                .any(|name| locked.is_none_or(|values| !values.contains_key(name)))
            {
                return Some(false);
            }
        }
    }
    if let Some(packages) = packages {
        let resolves = |owner: &str, dependency: &str| {
            let suffix = format!("node_modules/{dependency}");
            if owner.is_empty() {
                return packages.contains_key(&suffix);
            }
            let mut cursor = owner;
            loop {
                let candidate = format!("{cursor}/node_modules/{dependency}");
                if packages.contains_key(&candidate) {
                    return true;
                }
                let Some(index) = cursor.rfind("/node_modules/") else {
                    break;
                };
                cursor = &cursor[..index];
            }
            packages.contains_key(&suffix)
        };
        for (owner, metadata) in packages {
            for section in ["dependencies", "devDependencies", "optionalDependencies"] {
                let Some(dependencies) = metadata.get(section).and_then(Value::as_object) else {
                    continue;
                };
                if dependencies
                    .keys()
                    .any(|dependency| !resolves(owner, dependency))
                {
                    return Some(false);
                }
            }
        }
    }
    Some(true)
}

fn node_target(root: &Path, path: &Path) -> Result<RunTarget, InspectError> {
    let raw = fs::read_to_string(path).map_err(|error| InspectError::Manifest {
        path: path.display().to_string(),
        message: error.to_string(),
    })?;
    let value: Value = serde_json::from_str(&raw).map_err(|error| InspectError::Manifest {
        path: path.display().to_string(),
        message: error.to_string(),
    })?;
    let dir = path.parent().unwrap_or(root);
    let rel = relative(root, path);
    let manifest = if rel.is_empty() {
        "package.json".to_string()
    } else {
        format!("{rel}/package.json")
    };
    let scripts = value.get("scripts").and_then(Value::as_object);
    let deps = [value.get("dependencies"), value.get("devDependencies")]
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .flat_map(|map| map.keys())
        .cloned()
        .collect::<Vec<_>>();
    let has_start_script = ["start", "dev", "preview"].into_iter().any(|key| {
        scripts
            .and_then(|values| values.get(key))
            .and_then(Value::as_str)
            .is_some()
    });
    let is_web = deps.iter().any(|name| {
        matches!(
            name.as_str(),
            "vite" | "next" | "react" | "vue" | "svelte" | "@angular/core"
        )
    }) || (has_start_script
        && (dir.join("index.html").is_file() || dir.join("public/index.html").is_file()));
    let is_cli = value.get("bin").is_some();
    let kind = if is_web {
        ProjectKind::Web
    } else if is_cli {
        ProjectKind::Cli
    } else if has_start_script {
        ProjectKind::Service
    } else {
        ProjectKind::Library
    };
    let mut commands = Vec::new();
    let mut blockers = Vec::new();
    let stack = if dir.join("bun.lock").is_file() || dir.join("bun.lockb").is_file() {
        ProjectStack::Bun
    } else {
        ProjectStack::Node
    };
    let lock = if dir.join("bun.lock").is_file() || dir.join("bun.lockb").is_file() {
        Some((
            "bun",
            vec!["install", "--frozen-lockfile", "--ignore-scripts"],
            if dir.join("bun.lock").is_file() {
                "bun.lock"
            } else {
                "bun.lockb"
            },
            vec![],
            vec![],
        ))
    } else if dir.join("pnpm-lock.yaml").is_file() {
        Some((
            "corepack",
            vec!["pnpm", "install", "--frozen-lockfile", "--ignore-scripts"],
            "pnpm-lock.yaml",
            vec!["pnpm", "rebuild", "--offline"],
            vec!["pnpm"],
        ))
    } else if dir.join("package-lock.json").is_file() {
        Some((
            "npm",
            vec!["ci", "--ignore-scripts"],
            "package-lock.json",
            vec!["rebuild", "--offline", "--foreground-scripts"],
            vec![],
        ))
    } else if dir.join("yarn.lock").is_file() {
        Some((
            "corepack",
            vec!["yarn", "install", "--immutable", "--mode=skip-builds"],
            "yarn.lock",
            vec!["yarn", "rebuild"],
            vec!["yarn"],
        ))
    } else {
        None
    };
    let (program, run_prefix) = lock
        .as_ref()
        .map(|(program, _, _, _, prefix)| (*program, prefix.clone()))
        .unwrap_or(("npm", Vec::new()));
    if let Some((program, args, lock_name, rebuild_args, _)) = lock {
        commands.push(command(
            RunPhase::Acquire,
            program,
            &args,
            evidence(&manifest, lock_name, 2),
            NetworkPolicy::RegistryRestricted,
            false,
        ));
        if !rebuild_args.is_empty() {
            commands.push(command(
                RunPhase::Build,
                program,
                &rebuild_args,
                evidence(&manifest, "offline lifecycle scripts", 2),
                NetworkPolicy::None,
                false,
            ));
        }
        if lock_name == "package-lock.json"
            && npm_lock_consistent(&value, &dir.join(lock_name)) == Some(false)
        {
            blockers.push(PlanBlocker {
                phase: RunPhase::Acquire,
                origin: BlockerOrigin::Repository,
                code: "node_lockfile_out_of_sync".into(),
                summary: "package-lock.json is out of sync with package.json".into(),
                detail: "Declared dependency specifications do not match the root package recorded in the npm lock file.".into(),
                evidence: vec![evidence(&manifest, "dependencies + package-lock.json", 2)],
            });
        }
    } else {
        blockers.push(PlanBlocker {
            phase: RunPhase::Acquire,
            origin: BlockerOrigin::Repository,
            code: "node_lockfile_missing".into(),
            summary: "Dependency lock file is missing".into(),
            detail: "Verity will not claim a reproducible Node installation without a lock file."
                .into(),
            evidence: vec![evidence(&manifest, "package manager", 2)],
        });
    }
    if scripts
        .and_then(|s| s.get("build"))
        .and_then(Value::as_str)
        .is_some()
    {
        let mut args = run_prefix.clone();
        args.extend(["run", "build"]);
        commands.push(command(
            RunPhase::Build,
            program,
            &args,
            evidence(&manifest, "scripts.build", 2),
            NetworkPolicy::None,
            false,
        ));
    }
    let has_real_test = scripts
        .and_then(|s| s.get("test"))
        .and_then(Value::as_str)
        .is_some_and(|script| !script.contains("no test specified") && !script.trim().is_empty());
    if has_real_test {
        let mut args = run_prefix.clone();
        if stack == ProjectStack::Bun {
            args.extend(["run", "test"]);
        } else {
            args.push("test");
        }
        commands.push(command(
            RunPhase::Test,
            program,
            &args,
            evidence(&manifest, "scripts.test", 2),
            NetworkPolicy::None,
            false,
        ));
    }
    let start_key = ["start", "dev", "preview"].into_iter().find(|key| {
        scripts
            .and_then(|s| s.get(*key))
            .and_then(Value::as_str)
            .is_some()
    });
    if let Some(key) = start_key {
        let mut args = run_prefix.clone();
        args.extend(["run", key]);
        if key == "dev" && deps.iter().any(|name| name == "vite") {
            args.extend(["--", "--host", "0.0.0.0", "--port", "4173"]);
        }
        commands.push(command(
            RunPhase::Launch,
            program,
            &args,
            evidence(&manifest, &format!("scripts.{key}"), 2),
            NetworkPolicy::InternalOnly,
            false,
        ));
    }
    let oracle = if kind == ProjectKind::Web && start_key.is_some() && has_real_test {
        VerificationOracle { kind: OracleKind::HttpHtml, description: "Declared tests pass and the live application returns a non-empty HTML document without an HTTP error.".into(), machine_verifiable: true, evidence: vec![evidence(&manifest, "scripts.test + scripts.start/dev/preview", 2)] }
    } else if has_real_test {
        VerificationOracle {
            kind: OracleKind::TestSuite,
            description: "The repository-declared test suite exits successfully.".into(),
            machine_verifiable: true,
            evidence: vec![evidence(&manifest, "scripts.test", 2)],
        }
    } else {
        if start_key.is_none() {
            blockers.push(PlanBlocker {
                phase: RunPhase::Oracle,
                origin: BlockerOrigin::Oracle,
                code: "machine_oracle_missing".into(),
                summary: "No machine-verifiable oracle was found".into(),
                detail:
                    "A process or generated artifact alone is insufficient for a verified result."
                        .into(),
                evidence: vec![evidence(&manifest, "scripts", 2)],
            });
        }
        VerificationOracle {
            kind: OracleKind::None,
            description: "No declared test, health check, or smoke oracle was found.".into(),
            machine_verifiable: false,
            evidence: vec![],
        }
    };
    let role = target_role(&rel, &kind);
    let oracle_state = oracle_status(&oracle);
    Ok(RunTarget {
        id: target_id(&stack, &rel),
        label: value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_else(|| {
                dir.file_name()
                    .and_then(|v| v.to_str())
                    .unwrap_or("Node target")
            })
            .to_string(),
        relative_root: rel,
        stack,
        kind,
        role,
        components: vec![],
        recommended: false,
        selection_reason: "Discovered from a Node package manifest.".into(),
        plan_status: if blockers
            .iter()
            .any(|item| item.origin != BlockerOrigin::Oracle)
        {
            PlanStatus::Incomplete
        } else {
            PlanStatus::Complete
        },
        environment_status: EnvironmentStatus::Unchecked,
        environment_reason_code: "target_runtime_not_checked".into(),
        oracle_status: oracle_state,
        commands,
        oracle,
        blockers,
        declarations: declarations(dir),
    })
}

fn rust_target(root: &Path, path: &Path) -> RunTarget {
    let dir = path.parent().unwrap_or(root);
    let rel = relative(root, path);
    let manifest = if rel.is_empty() {
        "Cargo.toml".into()
    } else {
        format!("{rel}/Cargo.toml")
    };
    let kind = if dir.join("tauri.conf.json").is_file()
        || dir.join("src-tauri/tauri.conf.json").is_file()
    {
        ProjectKind::Desktop
    } else if dir.join("src/main.rs").is_file() {
        ProjectKind::Cli
    } else {
        ProjectKind::Library
    };
    let mut blockers = Vec::new();
    let locked = dir.join("Cargo.lock").is_file() || root.join("Cargo.lock").is_file();
    if !locked {
        blockers.push(PlanBlocker {
            phase: RunPhase::Acquire,
            origin: BlockerOrigin::Repository,
            code: "rust_lockfile_missing".into(),
            summary: "Cargo.lock is missing".into(),
            detail: "Verity requires a resolved dependency graph before reproducible verification."
                .into(),
            evidence: vec![evidence(&manifest, "package.lock", 2)],
        });
    }
    let native = kind == ProjectKind::Desktop;
    let native_network = if native {
        NetworkPolicy::NativeUserConfirmed
    } else {
        NetworkPolicy::RegistryRestricted
    };
    let offline_network = if native {
        NetworkPolicy::NativeUserConfirmed
    } else {
        NetworkPolicy::None
    };
    let mut commands = vec![
        command(
            RunPhase::Acquire,
            "cargo",
            &["fetch", "--locked"],
            evidence(&manifest, "package", 2),
            native_network,
            native,
        ),
        command(
            RunPhase::Build,
            "cargo",
            &["build", "--locked"],
            evidence(&manifest, "package", 2),
            offline_network.clone(),
            native,
        ),
        command(
            RunPhase::Test,
            "cargo",
            &["test", "--locked"],
            evidence(&manifest, "package", 2),
            offline_network,
            native,
        ),
    ];
    if native {
        commands.push(command(
            RunPhase::Launch,
            "cargo",
            &["run", "--locked"],
            evidence(&manifest, "package", 2),
            NetworkPolicy::NativeUserConfirmed,
            true,
        ));
    }
    let oracle = VerificationOracle {
        kind: if native {
            OracleKind::None
        } else {
            OracleKind::TestSuite
        },
        description: if native {
            "No supported desktop launch oracle was found."
        } else {
            "Cargo build and repository tests complete successfully."
        }
        .into(),
        machine_verifiable: !native,
        evidence: if native {
            vec![]
        } else {
            vec![evidence(&manifest, "cargo test", 3)]
        },
    };
    let role = target_role(&rel, &kind);
    let oracle_state = oracle_status(&oracle);
    RunTarget {
        id: target_id(&ProjectStack::Rust, &rel),
        label: dir
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or("Rust target")
            .into(),
        relative_root: rel,
        stack: ProjectStack::Rust,
        kind,
        role,
        components: vec![],
        recommended: false,
        selection_reason: "Discovered from a Cargo manifest.".into(),
        plan_status: if blockers.is_empty() {
            PlanStatus::Complete
        } else {
            PlanStatus::Incomplete
        },
        environment_status: EnvironmentStatus::Unchecked,
        environment_reason_code: "target_runtime_not_checked".into(),
        oracle_status: oracle_state,
        commands,
        oracle,
        blockers,
        declarations: declarations(dir),
    }
}

fn python_target(root: &Path, path: &Path) -> RunTarget {
    let dir = path.parent().unwrap_or(root);
    let rel = relative(root, path);
    let filename = path
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("pyproject.toml");
    let manifest = if rel.is_empty() {
        filename.into()
    } else {
        format!("{rel}/{filename}")
    };
    let raw = fs::read_to_string(path).unwrap_or_default();
    let service = ["fastapi", "flask", "django"]
        .iter()
        .any(|token| raw.to_lowercase().contains(token));
    let tests = raw.to_lowercase().contains("pytest");
    let requirements = ["requirements.lock", "requirements.txt"]
        .into_iter()
        .find(|name| dir.join(name).is_file() && pinned_requirements(&dir.join(name)));
    let uses_uv = dir.join("uv.lock").is_file();
    let uses_poetry = dir.join("poetry.lock").is_file();
    let locked = uses_uv || requirements.is_some();
    let mut blockers = Vec::new();
    if !locked {
        blockers.push(PlanBlocker {
            phase: RunPhase::Acquire,
            origin: BlockerOrigin::Repository,
            code: "python_lockfile_missing".into(),
            summary: "Resolved Python dependencies are missing".into(),
            detail: "A pyproject file without a lock or constraints file is not reproducible."
                .into(),
            evidence: vec![evidence(&manifest, "dependencies", 2)],
        });
    }
    if uses_poetry {
        blockers.push(PlanBlocker {
            phase: RunPhase::Acquire,
            origin: BlockerOrigin::VerityPlan,
            code: "python_manager_unsupported".into(),
            summary: "Poetry execution is not supported by this beta adapter".into(),
            detail:
                "The lock is recognized, but Verity has no certified Poetry container plan yet."
                    .into(),
            evidence: vec![evidence(&manifest, "poetry.lock", 2)],
        });
    }
    if !tests {
        blockers.push(PlanBlocker {
            phase: RunPhase::Oracle,
            origin: BlockerOrigin::Oracle,
            code: "machine_oracle_missing".into(),
            summary: "No Python test oracle was found".into(),
            detail: "Verity will not infer application health from imports alone.".into(),
            evidence: vec![evidence(&manifest, "tests", 3)],
        });
    }
    let commands = if uses_uv {
        vec![
            command(
                RunPhase::Acquire,
                "uv",
                &["sync", "--frozen"],
                evidence(&manifest, "uv.lock", 2),
                NetworkPolicy::RegistryRestricted,
                false,
            ),
            command(
                RunPhase::Test,
                ".venv/bin/python",
                &["-m", "pytest", "-q"],
                evidence(&manifest, "tests", 3),
                NetworkPolicy::None,
                false,
            ),
        ]
    } else if let Some(requirements_file) = requirements {
        vec![
            command(
                RunPhase::Acquire,
                "python",
                &["-m", "venv", ".verity-venv"],
                evidence(&manifest, "virtual environment", 2),
                NetworkPolicy::None,
                false,
            ),
            command(
                RunPhase::Acquire,
                ".verity-venv/bin/python",
                &["-m", "pip", "install", "--requirement", requirements_file],
                evidence(&manifest, "requirements", 2),
                NetworkPolicy::RegistryRestricted,
                false,
            ),
            command(
                RunPhase::Test,
                ".verity-venv/bin/python",
                &["-m", "pytest", "-q"],
                evidence(&manifest, "tests", 3),
                NetworkPolicy::None,
                false,
            ),
        ]
    } else {
        Vec::new()
    };
    let kind = if service {
        ProjectKind::Service
    } else {
        ProjectKind::Library
    };
    let oracle = VerificationOracle {
        kind: if tests {
            OracleKind::TestSuite
        } else {
            OracleKind::None
        },
        description: if tests {
            "Pytest completes successfully."
        } else {
            "No declared oracle was found."
        }
        .into(),
        machine_verifiable: tests,
        evidence: if tests {
            vec![evidence(&manifest, "tests", 3)]
        } else {
            vec![]
        },
    };
    let role = target_role(&rel, &kind);
    let oracle_state = oracle_status(&oracle);
    RunTarget {
        id: target_id(&ProjectStack::Python, &rel),
        label: dir
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or("Python target")
            .into(),
        relative_root: rel,
        stack: ProjectStack::Python,
        kind,
        role,
        components: vec![],
        recommended: false,
        selection_reason: "Discovered from Python dependency metadata.".into(),
        plan_status: if blockers
            .iter()
            .any(|item| item.origin != BlockerOrigin::Oracle)
        {
            PlanStatus::Incomplete
        } else {
            PlanStatus::Complete
        },
        environment_status: EnvironmentStatus::Unchecked,
        environment_reason_code: "target_runtime_not_checked".into(),
        oracle_status: oracle_state,
        commands,
        oracle,
        blockers,
        declarations: declarations(dir),
    }
}

fn go_target(root: &Path, path: &Path) -> RunTarget {
    let dir = path.parent().unwrap_or(root);
    let rel = relative(root, path);
    let manifest = if rel.is_empty() {
        "go.mod".into()
    } else {
        format!("{rel}/go.mod")
    };
    let main = dir.join("main.go").is_file() || dir.join("cmd").is_dir();
    let locked = dir.join("go.sum").is_file();
    let mut blockers = Vec::new();
    if !locked {
        blockers.push(PlanBlocker {
            phase: RunPhase::Acquire,
            origin: BlockerOrigin::Repository,
            code: "go_sum_missing".into(),
            summary: "go.sum is missing".into(),
            detail: "Verity requires module checksums before offline verification.".into(),
            evidence: vec![evidence(&manifest, "module checksums", 2)],
        });
    }
    let commands = vec![
        command(
            RunPhase::Acquire,
            "go",
            &["mod", "download"],
            evidence(&manifest, "require", 2),
            NetworkPolicy::RegistryRestricted,
            false,
        ),
        command(
            RunPhase::Build,
            "go",
            &["build", "./..."],
            evidence(&manifest, "module", 2),
            NetworkPolicy::None,
            false,
        ),
        command(
            RunPhase::Test,
            "go",
            &["test", "./..."],
            evidence(&manifest, "module", 2),
            NetworkPolicy::None,
            false,
        ),
    ];
    let kind = if main {
        ProjectKind::Cli
    } else {
        ProjectKind::Library
    };
    let oracle = VerificationOracle {
        kind: OracleKind::TestSuite,
        description: "Go build and test complete successfully.".into(),
        machine_verifiable: true,
        evidence: vec![evidence(&manifest, "go test ./...", 3)],
    };
    let role = target_role(&rel, &kind);
    RunTarget {
        id: target_id(&ProjectStack::Go, &rel),
        label: dir
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or("Go target")
            .into(),
        relative_root: rel,
        stack: ProjectStack::Go,
        kind,
        role,
        components: vec![],
        recommended: false,
        selection_reason: "Discovered from go.mod.".into(),
        plan_status: if blockers.is_empty() {
            PlanStatus::Complete
        } else {
            PlanStatus::Incomplete
        },
        environment_status: EnvironmentStatus::Unchecked,
        environment_reason_code: "target_runtime_not_checked".into(),
        oracle_status: OracleStatus::Machine,
        commands,
        oracle,
        blockers,
        declarations: declarations(dir),
    }
}

fn godot_target(root: &Path, path: &Path) -> RunTarget {
    let dir = path.parent().unwrap_or(root);
    let rel = relative(root, path);
    let manifest = if rel.is_empty() {
        "project.godot".into()
    } else {
        format!("{rel}/project.godot")
    };
    let raw = fs::read_to_string(path).unwrap_or_default();
    let has_main = raw
        .lines()
        .any(|line| line.trim_start().starts_with("run/main_scene="));
    let has_gut = dir.join("addons/gut/gut_cmdln.gd").is_file();
    let mut blockers = Vec::new();
    if !has_main {
        blockers.push(PlanBlocker {
            phase: RunPhase::Detect,
            origin: BlockerOrigin::Repository,
            code: "godot_main_scene_missing".into(),
            summary: "Godot main scene is not declared".into(),
            detail: "The project cannot be launched deterministically without run/main_scene."
                .into(),
            evidence: vec![evidence(&manifest, "application.run/main_scene", 2)],
        });
    }
    let mut commands = vec![command(
        RunPhase::Build,
        "godot",
        &["--headless", "--path", ".", "--editor", "--quit"],
        evidence(&manifest, "project import", 2),
        NetworkPolicy::NativeUserConfirmed,
        true,
    )];
    if has_gut {
        commands.push(command(
            RunPhase::Test,
            "godot",
            &[
                "--headless",
                "--path",
                ".",
                "-s",
                "addons/gut/gut_cmdln.gd",
                "-gexit",
            ],
            evidence(&manifest, "addons/gut/gut_cmdln.gd", 3),
            NetworkPolicy::NativeUserConfirmed,
            true,
        ));
    }
    commands.push(command(
        RunPhase::Launch,
        "godot",
        &["--path", ".", "--quit-after", "120"],
        evidence(&manifest, "application.run/main_scene", 2),
        NetworkPolicy::NativeUserConfirmed,
        true,
    ));
    let test_script = [
        "tools/run-tests.ps1",
        "tools/run-tests.sh",
        "test/run-tests.sh",
    ]
    .into_iter()
    .find(|candidate| dir.join(candidate).is_file());
    if let Some(script) = test_script {
        let (program, args): (&str, Vec<&str>) = if script.ends_with(".ps1") {
            (
                "powershell",
                vec!["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", script],
            )
        } else {
            ("sh", vec![script])
        };
        commands.push(command(
            RunPhase::Test,
            program,
            &args,
            evidence(script, "declared test runner", 3),
            NetworkPolicy::NativeUserConfirmed,
            true,
        ));
    }
    let machine_oracle = has_gut || test_script.is_some();
    let oracle = VerificationOracle {
        kind: if machine_oracle {
            OracleKind::DeclaredSmoke
        } else {
            OracleKind::WindowSignal
        },
        description: if machine_oracle {
            "The checked-in headless test command must pass."
        } else {
            "Only a native window signal is available; the result cannot be verified."
        }
        .into(),
        machine_verifiable: machine_oracle,
        evidence: test_script
            .map(|script| vec![evidence(script, "declared test runner", 3)])
            .or_else(|| has_gut.then(|| vec![evidence("addons/gut/gut_cmdln.gd", "GUT CLI", 3)]))
            .unwrap_or_default(),
    };
    let kind = ProjectKind::Game;
    let role = target_role(&rel, &kind);
    let oracle_state = oracle_status(&oracle);
    RunTarget {
        id: target_id(&ProjectStack::Godot, &rel),
        label: dir
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or("Godot project")
            .into(),
        relative_root: rel,
        stack: ProjectStack::Godot,
        kind,
        role,
        components: vec![],
        recommended: false,
        selection_reason: "Discovered from project.godot.".into(),
        plan_status: if blockers.is_empty() {
            PlanStatus::Complete
        } else {
            PlanStatus::Incomplete
        },
        environment_status: EnvironmentStatus::Unchecked,
        environment_reason_code: "target_runtime_not_checked".into(),
        oracle_status: oracle_state,
        commands,
        oracle,
        blockers,
        declarations: declarations(dir),
    }
}

fn deno_target(root: &Path, path: &Path) -> Result<RunTarget, InspectError> {
    let raw = fs::read_to_string(path).map_err(|error| InspectError::Manifest {
        path: path.display().to_string(),
        message: error.to_string(),
    })?;
    let value: Value = json5::from_str(&raw).map_err(|error| InspectError::Manifest {
        path: path.display().to_string(),
        message: error.to_string(),
    })?;
    let dir = path.parent().unwrap_or(root);
    let rel = relative(root, path);
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("deno.json");
    let manifest = if rel.is_empty() {
        filename.into()
    } else {
        format!("{rel}/{filename}")
    };
    let tasks = value.get("tasks").and_then(Value::as_object);
    let has_lock = dir.join("deno.lock").is_file();
    let launch = ["start", "dev", "serve"]
        .into_iter()
        .find(|name| tasks.is_some_and(|tasks| tasks.contains_key(*name)));
    let has_test = tasks.is_some_and(|tasks| tasks.contains_key("test"));
    let is_web = launch.is_some();
    let kind = if is_web {
        ProjectKind::Web
    } else {
        ProjectKind::Library
    };
    let mut blockers = Vec::new();
    if !has_lock {
        blockers.push(PlanBlocker {
            phase: RunPhase::Acquire,
            origin: BlockerOrigin::Repository,
            code: "deno_lockfile_missing".into(),
            summary: "deno.lock is missing".into(),
            detail: "A Deno dependency graph must be locked before offline verification.".into(),
            evidence: vec![evidence(&manifest, "lock", 2)],
        });
    }
    if launch.is_none() && !has_test {
        blockers.push(PlanBlocker {
            phase: RunPhase::Detect,
            origin: BlockerOrigin::Repository,
            code: "deno_entry_missing".into(),
            summary: "No deterministic Deno task was found".into(),
            detail: "Declare one start/dev/serve task or a test task.".into(),
            evidence: vec![evidence(&manifest, "tasks", 2)],
        });
    }
    let mut commands = vec![command(
        RunPhase::Acquire,
        "deno",
        &["cache", "--lock=deno.lock", "--frozen", "deno.json"],
        evidence(&manifest, "lock", 2),
        NetworkPolicy::RegistryRestricted,
        false,
    )];
    if has_test {
        commands.push(command(
            RunPhase::Test,
            "deno",
            &["task", "test"],
            evidence(&manifest, "tasks.test", 2),
            NetworkPolicy::None,
            false,
        ));
    }
    if let Some(task) = launch {
        commands.push(command(
            RunPhase::Launch,
            "deno",
            &["task", task],
            evidence(&manifest, &format!("tasks.{task}"), 2),
            NetworkPolicy::InternalOnly,
            false,
        ));
    }
    let oracle = VerificationOracle {
        kind: if is_web && has_test {
            OracleKind::HttpHtml
        } else if has_test {
            OracleKind::TestSuite
        } else {
            OracleKind::None
        },
        description: if has_test {
            "The declared Deno test task must pass."
        } else {
            "The declared Deno task can be started, but no machine oracle exists."
        }
        .into(),
        machine_verifiable: has_test,
        evidence: has_test
            .then(|| vec![evidence(&manifest, "tasks.test", 3)])
            .unwrap_or_default(),
    };
    let role = target_role(&rel, &kind);
    let oracle_state = oracle_status(&oracle);
    Ok(RunTarget {
        id: target_id(&ProjectStack::Deno, &rel),
        label: dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Deno target")
            .into(),
        relative_root: rel,
        stack: ProjectStack::Deno,
        kind,
        role,
        components: Vec::new(),
        recommended: false,
        selection_reason: "Discovered from deno.json tasks and deno.lock.".into(),
        plan_status: if blockers.is_empty() {
            PlanStatus::Complete
        } else {
            PlanStatus::Incomplete
        },
        environment_status: EnvironmentStatus::Unchecked,
        environment_reason_code: "target_runtime_not_checked".into(),
        oracle_status: oracle_state,
        commands,
        oracle,
        blockers,
        declarations: declarations(dir),
    })
}

fn compose_target(root: &Path, path: &Path) -> Result<RunTarget, InspectError> {
    let raw = fs::read_to_string(path).map_err(|error| InspectError::Manifest {
        path: path.display().to_string(),
        message: error.to_string(),
    })?;
    let value: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(&raw).map_err(|error| InspectError::Manifest {
            path: path.display().to_string(),
            message: error.to_string(),
        })?;
    let dir = path.parent().unwrap_or(root);
    let rel = relative(root, path);
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("compose.yaml");
    let manifest = if rel.is_empty() {
        filename.into()
    } else {
        format!("{rel}/{filename}")
    };
    let services = value
        .get("services")
        .and_then(serde_yaml_ng::Value::as_mapping);
    let mut blockers = Vec::new();
    if services.is_none_or(|services| services.is_empty()) {
        blockers.push(PlanBlocker {
            phase: RunPhase::Detect,
            origin: BlockerOrigin::Repository,
            code: "compose_services_missing".into(),
            summary: "Compose declares no services".into(),
            detail: "A product target requires at least one declared service.".into(),
            evidence: vec![evidence(&manifest, "services", 1)],
        });
    }
    let service_count = services.map_or(0, |services| services.len());
    let healthy_count = services.map_or(0, |services| {
        services
            .values()
            .filter(|service| service.get("healthcheck").is_some())
            .count()
    });
    let oracle = VerificationOracle {
        kind: if service_count > 0 && healthy_count == service_count { OracleKind::DeclaredHealth } else { OracleKind::None },
        description: if service_count > 0 && healthy_count == service_count {
            "Every Compose service must reach its declared health check."
        } else {
            "The Compose topology can be started, but not every service has a machine health oracle."
        }.into(),
        machine_verifiable: service_count > 0 && healthy_count == service_count,
        evidence: (healthy_count > 0).then(|| vec![evidence(&manifest, "services.*.healthcheck", 1)]).unwrap_or_default(),
    };
    let mut commands = vec![command(
        RunPhase::Build,
        "docker",
        &["compose", "-f", filename, "build", "--pull"],
        evidence(&manifest, "services.*.build", 1),
        NetworkPolicy::NativeUserConfirmed,
        true,
    )];
    commands.push(command(
        RunPhase::Launch,
        "docker",
        &["compose", "-f", filename, "up", "--detach", "--wait"],
        evidence(&manifest, "services", 1),
        NetworkPolicy::NativeUserConfirmed,
        true,
    ));
    let oracle_state = oracle_status(&oracle);
    Ok(RunTarget {
        id: target_id(&ProjectStack::Compose, &rel),
        label: format!(
            "{} product",
            dir.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Compose")
        ),
        relative_root: rel,
        stack: ProjectStack::Compose,
        kind: ProjectKind::Service,
        role: TargetRole::Product,
        components: Vec::new(),
        recommended: false,
        selection_reason: format!(
            "Compose declares the complete {service_count}-service product topology."
        ),
        plan_status: if blockers.is_empty() {
            PlanStatus::Complete
        } else {
            PlanStatus::Incomplete
        },
        environment_status: EnvironmentStatus::Unchecked,
        environment_reason_code: "target_runtime_not_checked".into(),
        oracle_status: oracle_state,
        commands,
        oracle,
        blockers,
        declarations: vec![filename.into()],
    })
}

fn tree_has_extension(dir: &Path, extensions: &[&str]) -> bool {
    let mut builder = WalkBuilder::new(dir);
    builder
        .hidden(false)
        .parents(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .follow_links(false)
        .max_depth(Some(8));
    builder.build().filter_map(Result::ok).any(|entry| {
        entry.file_type().is_some_and(|kind| kind.is_file())
            && entry
                .path()
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| {
                    extensions
                        .iter()
                        .any(|item| value.eq_ignore_ascii_case(item))
                })
    })
}

fn jvm_target(root: &Path, path: &Path) -> Result<RunTarget, InspectError> {
    let dir = path.parent().unwrap_or(root);
    let rel = relative(root, path);
    let filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("pom.xml");
    let manifest = if rel.is_empty() {
        filename.into()
    } else {
        format!("{rel}/{filename}")
    };
    let raw = fs::read_to_string(path).map_err(|error| InspectError::Manifest {
        path: path.display().to_string(),
        message: error.to_string(),
    })?;
    let lower = raw.to_ascii_lowercase();
    let stack = if filename.ends_with(".kts") || tree_has_extension(dir, &["kt", "kts"]) {
        ProjectStack::Kotlin
    } else {
        ProjectStack::Java
    };
    let gradle = filename.starts_with("build.gradle");
    let wrapper = dir.join("gradlew").is_file()
        && dir
            .join("gradle/wrapper/gradle-wrapper.properties")
            .is_file()
        && dir.join("gradle/wrapper/gradle-wrapper.jar").is_file();
    let service = lower.contains("spring-boot")
        || lower.contains("org.springframework.boot")
        || lower.contains("id(\"application\")")
        || lower.contains("id 'application'")
        || lower.contains("apply plugin: 'application'");
    let tests =
        dir.join("src/test").is_dir() || lower.contains("junit") || lower.contains("testng");
    let kind = if service {
        ProjectKind::Service
    } else {
        ProjectKind::Library
    };
    let mut blockers = Vec::new();
    if gradle && !wrapper {
        blockers.push(PlanBlocker {
            phase: RunPhase::Acquire,
            origin: BlockerOrigin::Repository,
            code: "gradle_wrapper_missing".into(),
            summary: "Gradle wrapper is incomplete".into(),
            detail: "Verity requires the checked-in wrapper script, properties, and wrapper JAR to pin the Gradle runtime.".into(),
            evidence: vec![evidence(&manifest, "gradle wrapper", 2)],
        });
    }
    let mut commands = Vec::new();
    if gradle && wrapper {
        commands.push(command(
            RunPhase::Acquire,
            "sh",
            &["./gradlew", "--no-daemon", "dependencies"],
            evidence(&manifest, "gradle wrapper + dependencies", 2),
            NetworkPolicy::RegistryRestricted,
            false,
        ));
        commands.push(command(
            RunPhase::Build,
            "sh",
            &["./gradlew", "--offline", "--no-daemon", "assemble"],
            evidence(&manifest, "assemble", 2),
            NetworkPolicy::None,
            false,
        ));
        if tests {
            commands.push(command(
                RunPhase::Test,
                "sh",
                &["./gradlew", "--offline", "--no-daemon", "test"],
                evidence(&manifest, "test", 3),
                NetworkPolicy::None,
                false,
            ));
        }
        if service {
            commands.push(command(
                RunPhase::Launch,
                "sh",
                &["./gradlew", "--offline", "--no-daemon", "bootRun"],
                evidence(&manifest, "application/bootRun", 3),
                NetworkPolicy::InternalOnly,
                false,
            ));
        }
    } else if !gradle {
        commands.push(command(
            RunPhase::Acquire,
            "mvn",
            &["-B", "-ntp", "-DskipTests", "dependency:go-offline"],
            evidence(&manifest, "dependencies + plugins", 2),
            NetworkPolicy::RegistryRestricted,
            false,
        ));
        commands.push(command(
            RunPhase::Build,
            "mvn",
            &["-B", "-ntp", "-o", "-DskipTests", "package"],
            evidence(&manifest, "package", 2),
            NetworkPolicy::None,
            false,
        ));
        if tests {
            commands.push(command(
                RunPhase::Test,
                "mvn",
                &["-B", "-ntp", "-o", "test"],
                evidence(&manifest, "test", 3),
                NetworkPolicy::None,
                false,
            ));
        }
        if service {
            commands.push(command(
                RunPhase::Launch,
                "mvn",
                &["-B", "-ntp", "-o", "spring-boot:run"],
                evidence(&manifest, "spring-boot plugin", 3),
                NetworkPolicy::InternalOnly,
                false,
            ));
        }
    }
    let oracle = if tests {
        VerificationOracle {
            kind: OracleKind::TestSuite,
            description: "The repository-declared JVM test suite exits successfully.".into(),
            machine_verifiable: true,
            evidence: vec![evidence(&manifest, "test", 3)],
        }
    } else if service {
        VerificationOracle {
            kind: OracleKind::None,
            description:
                "The application has a traceable launch task but no declared machine oracle.".into(),
            machine_verifiable: false,
            evidence: Vec::new(),
        }
    } else {
        VerificationOracle {
            kind: OracleKind::PackageArtifact,
            description:
                "The JVM library resolves dependencies and produces its declared package artifact."
                    .into(),
            machine_verifiable: true,
            evidence: vec![evidence(&manifest, "package/assemble", 3)],
        }
    };
    let role = target_role(&rel, &kind);
    let oracle_state = oracle_status(&oracle);
    Ok(RunTarget {
        id: target_id(&stack, &rel),
        label: dir
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("JVM target")
            .into(),
        relative_root: rel,
        stack,
        kind,
        role,
        components: Vec::new(),
        recommended: false,
        selection_reason: format!("Discovered from the repository's {filename} build contract."),
        plan_status: if blockers.is_empty() {
            PlanStatus::Complete
        } else {
            PlanStatus::Incomplete
        },
        environment_status: EnvironmentStatus::Unchecked,
        environment_reason_code: "target_runtime_not_checked".into(),
        oracle_status: oracle_state,
        commands,
        oracle,
        blockers,
        declarations: declarations(dir),
    })
}

fn native_build_target(root: &Path, path: &Path) -> Result<RunTarget, InspectError> {
    let dir = path.parent().unwrap_or(root);
    let rel = relative(root, path);
    let filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("Makefile");
    let manifest = if rel.is_empty() {
        filename.into()
    } else {
        format!("{rel}/{filename}")
    };
    let raw = fs::read_to_string(path).map_err(|error| InspectError::Manifest {
        path: path.display().to_string(),
        message: error.to_string(),
    })?;
    let cpp = tree_has_extension(dir, &["cc", "cpp", "cxx", "hpp", "hh", "hxx"]);
    let stack = if cpp {
        ProjectStack::Cpp
    } else {
        ProjectStack::C
    };
    let has_main = raw.contains("add_executable")
        || tree_has_extension(&dir.join("src"), &["c", "cc", "cpp", "cxx"]);
    let kind = if has_main {
        ProjectKind::Cli
    } else {
        ProjectKind::Library
    };
    let has_tests = raw.to_ascii_lowercase().contains("test") || dir.join("tests").is_dir();
    let mut commands = Vec::new();
    match filename {
        "CMakeLists.txt" => {
            commands.push(command(
                RunPhase::Build,
                "cmake",
                &[
                    "-S",
                    ".",
                    "-B",
                    ".verity-build",
                    "-DCMAKE_BUILD_TYPE=Release",
                ],
                evidence(&manifest, "project + targets", 2),
                NetworkPolicy::NativeUserConfirmed,
                true,
            ));
            commands.push(command(
                RunPhase::Build,
                "cmake",
                &["--build", ".verity-build", "--config", "Release"],
                evidence(&manifest, "build targets", 2),
                NetworkPolicy::NativeUserConfirmed,
                true,
            ));
            if has_tests {
                commands.push(command(
                    RunPhase::Test,
                    "ctest",
                    &[
                        "--test-dir",
                        ".verity-build",
                        "--output-on-failure",
                        "-C",
                        "Release",
                    ],
                    evidence(&manifest, "CTest", 3),
                    NetworkPolicy::NativeUserConfirmed,
                    true,
                ));
            }
        }
        "meson.build" => {
            commands.push(command(
                RunPhase::Build,
                "meson",
                &["setup", ".verity-build", "--buildtype", "release"],
                evidence(&manifest, "project + targets", 2),
                NetworkPolicy::NativeUserConfirmed,
                true,
            ));
            commands.push(command(
                RunPhase::Build,
                "meson",
                &["compile", "-C", ".verity-build"],
                evidence(&manifest, "build targets", 2),
                NetworkPolicy::NativeUserConfirmed,
                true,
            ));
            if has_tests {
                commands.push(command(
                    RunPhase::Test,
                    "meson",
                    &["test", "-C", ".verity-build", "--print-errorlogs"],
                    evidence(&manifest, "test", 3),
                    NetworkPolicy::NativeUserConfirmed,
                    true,
                ));
            }
        }
        _ => {
            commands.push(command(
                RunPhase::Build,
                "make",
                &[],
                evidence(&manifest, "default target", 2),
                NetworkPolicy::NativeUserConfirmed,
                true,
            ));
            if has_tests {
                commands.push(command(
                    RunPhase::Test,
                    "make",
                    &["test"],
                    evidence(&manifest, "test target", 3),
                    NetworkPolicy::NativeUserConfirmed,
                    true,
                ));
            }
        }
    }
    let oracle = VerificationOracle {
        kind: if has_tests {
            OracleKind::TestSuite
        } else {
            OracleKind::PackageArtifact
        },
        description: if has_tests {
            "The native build and declared tests complete successfully."
        } else {
            "The native build produces its declared targets; no runtime behavior is inferred."
        }
        .into(),
        machine_verifiable: true,
        evidence: vec![evidence(
            &manifest,
            if has_tests { "test" } else { "build targets" },
            3,
        )],
    };
    let role = target_role(&rel, &kind);
    Ok(RunTarget {
        id: target_id(&stack, &rel),
        label: dir
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("Native target")
            .into(),
        relative_root: rel,
        stack,
        kind,
        role,
        components: Vec::new(),
        recommended: false,
        selection_reason: format!("Discovered from the repository's {filename} build contract."),
        plan_status: PlanStatus::Complete,
        environment_status: EnvironmentStatus::Unchecked,
        environment_reason_code: "target_runtime_not_checked".into(),
        oracle_status: oracle_status(&oracle),
        commands,
        oracle,
        blockers: Vec::new(),
        declarations: declarations(dir),
    })
}

fn dotnet_target(root: &Path, path: &Path) -> RunTarget {
    let dir = path.parent().unwrap_or(root);
    let rel = relative(root, path);
    let filename = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("project.csproj");
    let manifest = if rel.is_empty() {
        filename.into()
    } else {
        format!("{rel}/{filename}")
    };
    let mut project_files = Vec::new();
    let mut builder = WalkBuilder::new(dir);
    builder
        .hidden(false)
        .parents(true)
        .git_ignore(true)
        .follow_links(false)
        .max_depth(Some(8));
    for entry in builder.build().filter_map(Result::ok) {
        if entry.file_type().is_some_and(|kind| kind.is_file())
            && matches!(
                entry.path().extension().and_then(|value| value.to_str()),
                Some("csproj" | "fsproj")
            )
        {
            project_files.push(entry.path().to_path_buf());
        }
    }
    if project_files.is_empty() && !filename.ends_with(".sln") {
        project_files.push(path.to_path_buf());
    }
    let project_text = project_files
        .iter()
        .filter_map(|item| fs::read_to_string(item).ok())
        .collect::<Vec<_>>()
        .join("\n");
    let web = project_text.contains("Microsoft.NET.Sdk.Web") || dir.join("wwwroot").is_dir();
    let tests = project_text.contains("Microsoft.NET.Test.Sdk")
        || project_files.iter().any(|item| {
            item.file_name()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.to_ascii_lowercase().contains("test"))
        })
        || dir.join("tests").is_dir();
    let kind = if web {
        ProjectKind::Service
    } else {
        ProjectKind::Library
    };
    let mut blockers = Vec::new();
    let locks_complete = !project_files.is_empty()
        && project_files.iter().all(|item| {
            item.parent()
                .is_some_and(|parent| parent.join("packages.lock.json").is_file())
        });
    if !locks_complete {
        blockers.push(PlanBlocker {
            phase: RunPhase::Acquire,
            origin: BlockerOrigin::Repository,
            code: "dotnet_lockfile_missing".into(),
            summary: "packages.lock.json is missing".into(),
            detail: "Verity requires NuGet locked restore mode for reproducible verification."
                .into(),
            evidence: vec![evidence(&manifest, "RestorePackagesWithLockFile", 2)],
        });
    }
    let mut commands = vec![
        command(
            RunPhase::Acquire,
            "dotnet",
            &["restore", "--locked-mode"],
            evidence(&manifest, "PackageReference + packages.lock.json", 2),
            NetworkPolicy::RegistryRestricted,
            false,
        ),
        command(
            RunPhase::Build,
            "dotnet",
            &["build", "--no-restore", "-c", "Release"],
            evidence(&manifest, "Build", 2),
            NetworkPolicy::None,
            false,
        ),
    ];
    if tests {
        commands.push(command(
            RunPhase::Test,
            "dotnet",
            &["test", "--no-build", "-c", "Release"],
            evidence(&manifest, "test SDK", 3),
            NetworkPolicy::None,
            false,
        ));
    }
    if web {
        commands.push(command(
            RunPhase::Launch,
            "dotnet",
            &[
                "run",
                "--no-build",
                "-c",
                "Release",
                "--urls",
                "http://0.0.0.0:4173",
            ],
            evidence(&manifest, "Microsoft.NET.Sdk.Web", 3),
            NetworkPolicy::InternalOnly,
            false,
        ));
    }
    if !web && !tests {
        commands.push(command(
            RunPhase::Build,
            "dotnet",
            &[
                "pack",
                "--no-build",
                "-c",
                "Release",
                "-o",
                ".verity-artifacts",
            ],
            evidence(&manifest, "Pack", 3),
            NetworkPolicy::None,
            false,
        ));
    }
    let oracle = if tests {
        VerificationOracle {
            kind: OracleKind::TestSuite,
            description: "The .NET test suite exits successfully.".into(),
            machine_verifiable: true,
            evidence: vec![evidence(&manifest, "test SDK", 3)],
        }
    } else if web {
        VerificationOracle { kind: OracleKind::None, description: "The web project has a traceable launch command but no declared test or health oracle.".into(), machine_verifiable: false, evidence: Vec::new() }
    } else {
        VerificationOracle {
            kind: OracleKind::PackageArtifact,
            description: "The .NET library builds and produces a NuGet package.".into(),
            machine_verifiable: true,
            evidence: vec![evidence(&manifest, "Pack", 3)],
        }
    };
    let role = target_role(&rel, &kind);
    RunTarget {
        id: target_id(&ProjectStack::DotNet, &rel),
        label: dir
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or(".NET target")
            .into(),
        relative_root: rel,
        stack: ProjectStack::DotNet,
        kind,
        role,
        components: Vec::new(),
        recommended: false,
        selection_reason: "Discovered from a .NET project or solution contract.".into(),
        plan_status: if blockers.is_empty() {
            PlanStatus::Complete
        } else {
            PlanStatus::Incomplete
        },
        environment_status: EnvironmentStatus::Unchecked,
        environment_reason_code: "target_runtime_not_checked".into(),
        oracle_status: oracle_status(&oracle),
        commands,
        oracle,
        blockers,
        declarations: declarations(dir),
    }
}

fn php_target(root: &Path, path: &Path) -> Result<RunTarget, InspectError> {
    let dir = path.parent().unwrap_or(root);
    let rel = relative(root, path);
    let manifest = if rel.is_empty() {
        "composer.json".into()
    } else {
        format!("{rel}/composer.json")
    };
    let value: Value =
        serde_json::from_slice(&fs::read(path).map_err(|error| InspectError::Manifest {
            path: path.display().to_string(),
            message: error.to_string(),
        })?)
        .map_err(|error| InspectError::Manifest {
            path: path.display().to_string(),
            message: error.to_string(),
        })?;
    let scripts = value.get("scripts").and_then(Value::as_object);
    let start = ["start", "serve"]
        .into_iter()
        .find(|key| scripts.and_then(|items| items.get(*key)).is_some());
    let tests = ["test", "tests"]
        .into_iter()
        .find(|key| scripts.and_then(|items| items.get(*key)).is_some());
    let framework = value
        .get("require")
        .and_then(Value::as_object)
        .is_some_and(|deps| {
            deps.keys()
                .any(|name| name.contains("laravel") || name.contains("symfony"))
        });
    let kind = if start.is_some() || framework {
        ProjectKind::Service
    } else {
        ProjectKind::Library
    };
    let mut blockers = Vec::new();
    if !dir.join("composer.lock").is_file() {
        blockers.push(PlanBlocker {
            phase: RunPhase::Acquire,
            origin: BlockerOrigin::Repository,
            code: "composer_lock_missing".into(),
            summary: "composer.lock is missing".into(),
            detail: "Verity requires a committed Composer lock file before dependency acquisition."
                .into(),
            evidence: vec![evidence(&manifest, "require + composer.lock", 2)],
        });
    }
    if tests.is_none() && start.is_none() {
        blockers.push(PlanBlocker {
            phase: RunPhase::Oracle,
            origin: BlockerOrigin::Oracle,
            code: "php_machine_oracle_missing".into(),
            summary: "No PHP package or test oracle is declared".into(),
            detail: "Composer validation alone cannot prove that a PHP library behaves correctly. Add a repository test or package oracle before Verity can verify it.".into(),
            evidence: vec![evidence(&manifest, "scripts.test", 3)],
        });
    }
    let mut commands = vec![
        command(
            RunPhase::Acquire,
            "composer",
            &[
                "install",
                "--no-interaction",
                "--no-scripts",
                "--prefer-dist",
            ],
            evidence(&manifest, "composer.lock", 2),
            NetworkPolicy::RegistryRestricted,
            false,
        ),
        command(
            RunPhase::Build,
            "composer",
            &["validate", "--strict", "--no-check-publish"],
            evidence(&manifest, "schema", 2),
            NetworkPolicy::None,
            false,
        ),
    ];
    if let Some(key) = tests {
        commands.push(command(
            RunPhase::Test,
            "composer",
            &["run-script", key, "--no-interaction"],
            evidence(&manifest, &format!("scripts.{key}"), 3),
            NetworkPolicy::None,
            false,
        ));
    }
    if let Some(key) = start {
        commands.push(command(
            RunPhase::Launch,
            "composer",
            &["run-script", key, "--no-interaction"],
            evidence(&manifest, &format!("scripts.{key}"), 3),
            NetworkPolicy::InternalOnly,
            false,
        ));
    }
    let oracle = if tests.is_some() {
        VerificationOracle {
            kind: OracleKind::TestSuite,
            description: "The Composer-declared test script exits successfully.".into(),
            machine_verifiable: true,
            evidence: vec![evidence(&manifest, "scripts.test", 3)],
        }
    } else if start.is_some() {
        VerificationOracle {
            kind: OracleKind::None,
            description: "The PHP service has a declared launch script but no machine oracle."
                .into(),
            machine_verifiable: false,
            evidence: Vec::new(),
        }
    } else {
        VerificationOracle {
            kind: OracleKind::None,
            description: "Composer can validate the manifest, but the library declares no machine behavior oracle.".into(),
            machine_verifiable: false,
            evidence: Vec::new(),
        }
    };
    let role = target_role(&rel, &kind);
    Ok(RunTarget {
        id: target_id(&ProjectStack::Php, &rel),
        label: value
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("PHP target")
            .into(),
        relative_root: rel,
        stack: ProjectStack::Php,
        kind,
        role,
        components: Vec::new(),
        recommended: false,
        selection_reason: "Discovered from composer.json and composer.lock.".into(),
        plan_status: if blockers.is_empty() {
            PlanStatus::Complete
        } else {
            PlanStatus::Incomplete
        },
        environment_status: EnvironmentStatus::Unchecked,
        environment_reason_code: "target_runtime_not_checked".into(),
        oracle_status: oracle_status(&oracle),
        commands,
        oracle,
        blockers,
        declarations: declarations(dir),
    })
}

fn ruby_target(root: &Path, path: &Path) -> RunTarget {
    let dir = path.parent().unwrap_or(root);
    let rel = relative(root, path);
    let manifest = if rel.is_empty() {
        "Gemfile".into()
    } else {
        format!("{rel}/Gemfile")
    };
    let raw = fs::read_to_string(path)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let rails = raw.contains("gem 'rails'")
        || raw.contains("gem \"rails\"")
        || dir.join("bin/rails").is_file();
    let tests = raw.contains("rspec") || dir.join("spec").is_dir() || dir.join("test").is_dir();
    let kind = if rails {
        ProjectKind::Service
    } else {
        ProjectKind::Library
    };
    let mut blockers = Vec::new();
    if !dir.join("Gemfile.lock").is_file() {
        blockers.push(PlanBlocker {
            phase: RunPhase::Acquire,
            origin: BlockerOrigin::Repository,
            code: "bundler_lock_missing".into(),
            summary: "Gemfile.lock is missing".into(),
            detail: "Verity requires a committed Bundler lock file before dependency acquisition."
                .into(),
            evidence: vec![evidence(&manifest, "Gemfile.lock", 2)],
        });
    }
    if !tests && !rails {
        blockers.push(PlanBlocker {
            phase: RunPhase::Oracle,
            origin: BlockerOrigin::Oracle,
            code: "ruby_machine_oracle_missing".into(),
            summary: "No Ruby package or test oracle is declared".into(),
            detail: "Resolving a Gemfile is not proof that a Ruby library behaves correctly. Add a repository test or package oracle before Verity can verify it.".into(),
            evidence: vec![evidence(&manifest, "tests", 3)],
        });
    }
    let mut commands = vec![
        command(
            RunPhase::Acquire,
            "bundle",
            &["config", "set", "path", ".verity-bundle"],
            evidence(&manifest, "bundle path", 2),
            NetworkPolicy::None,
            false,
        ),
        command(
            RunPhase::Acquire,
            "bundle",
            &["install", "--jobs", "4", "--retry", "2"],
            evidence(&manifest, "Gemfile.lock", 2),
            NetworkPolicy::RegistryRestricted,
            false,
        ),
    ];
    if tests {
        let args = if raw.contains("rspec") || dir.join("spec").is_dir() {
            vec!["exec", "rspec"]
        } else {
            vec![
                "exec",
                "ruby",
                "-Itest",
                "-e",
                "Dir['test/**/*_test.rb'].sort.each { |f| require File.expand_path(f) }",
            ]
        };
        commands.push(command(
            RunPhase::Test,
            "bundle",
            &args,
            evidence(&manifest, "tests", 3),
            NetworkPolicy::None,
            false,
        ));
    }
    if rails {
        commands.push(command(
            RunPhase::Launch,
            "bundle",
            &["exec", "rails", "server", "-b", "0.0.0.0", "-p", "4173"],
            evidence(&manifest, "rails", 3),
            NetworkPolicy::InternalOnly,
            false,
        ));
    }
    let oracle = if tests {
        VerificationOracle {
            kind: OracleKind::TestSuite,
            description: "The Bundler-backed Ruby test suite exits successfully.".into(),
            machine_verifiable: true,
            evidence: vec![evidence(&manifest, "tests", 3)],
        }
    } else if rails {
        VerificationOracle {
            kind: OracleKind::None,
            description:
                "The Rails application can be launched, but no test or health oracle was declared."
                    .into(),
            machine_verifiable: false,
            evidence: Vec::new(),
        }
    } else {
        VerificationOracle {
            kind: OracleKind::None,
            description: "Bundler can resolve the dependency graph, but the library declares no machine behavior oracle.".into(),
            machine_verifiable: false,
            evidence: Vec::new(),
        }
    };
    let role = target_role(&rel, &kind);
    RunTarget {
        id: target_id(&ProjectStack::Ruby, &rel),
        label: dir
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("Ruby target")
            .into(),
        relative_root: rel,
        stack: ProjectStack::Ruby,
        kind,
        role,
        components: Vec::new(),
        recommended: false,
        selection_reason: "Discovered from Gemfile and Gemfile.lock.".into(),
        plan_status: if blockers.is_empty() {
            PlanStatus::Complete
        } else {
            PlanStatus::Incomplete
        },
        environment_status: EnvironmentStatus::Unchecked,
        environment_reason_code: "target_runtime_not_checked".into(),
        oracle_status: oracle_status(&oracle),
        commands,
        oracle,
        blockers,
        declarations: declarations(dir),
    }
}

fn static_web_target(root: &Path, path: &Path) -> RunTarget {
    let dir = path.parent().unwrap_or(root);
    let rel = relative(root, path);
    let manifest = if rel.is_empty() {
        "index.html".into()
    } else {
        format!("{rel}/index.html")
    };
    let oracle = VerificationOracle {
        kind: OracleKind::HttpHtml,
        description: "The static entry must return a non-empty HTML document without page errors."
            .into(),
        machine_verifiable: true,
        evidence: vec![evidence(&manifest, "html entry", 3)],
    };
    RunTarget {
        id: target_id(&ProjectStack::StaticWeb, &rel),
        label: dir.file_name().and_then(|name| name.to_str()).unwrap_or("Static site").into(),
        relative_root: rel.clone(),
        stack: ProjectStack::StaticWeb,
        kind: ProjectKind::Web,
        role: target_role(&rel, &ProjectKind::Web),
        components: Vec::new(),
        recommended: false,
        selection_reason: "Discovered from a standalone HTML entry.".into(),
        plan_status: PlanStatus::Complete,
        environment_status: EnvironmentStatus::Unchecked,
        environment_reason_code: "target_runtime_not_checked".into(),
        oracle_status: OracleStatus::Machine,
        commands: vec![command(
            RunPhase::Launch,
            "node",
            &[
                "-e",
                "const h=require('http'),f=require('fs');h.createServer((q,r)=>{try{const b=f.readFileSync('index.html');r.writeHead(200,{'content-type':'text/html'});r.end(b)}catch(e){r.writeHead(500);r.end()}}).listen(4173,'0.0.0.0')",
            ],
            evidence(&manifest, "html entry", 3),
            NetworkPolicy::InternalOnly,
            false,
        )],
        oracle,
        blockers: Vec::new(),
        declarations: Vec::new(),
    }
}

fn npm_workspace_matcher(root: &Path, relative_root: &str) -> Option<globset::GlobSet> {
    let manifest = root.join(relative_root).join("package.json");
    let package: Value = serde_json::from_slice(&fs::read(manifest).ok()?).ok()?;
    let workspaces = package.get("workspaces")?;
    let patterns = workspaces
        .as_array()
        .or_else(|| workspaces.get("packages").and_then(Value::as_array))?;
    let mut builder = GlobSetBuilder::new();
    let mut count = 0;
    for pattern in patterns.iter().filter_map(Value::as_str) {
        builder.add(Glob::new(pattern).ok()?);
        count += 1;
    }
    (count > 0).then(|| builder.build().ok()).flatten()
}

fn normalize_and_group_targets(root: &Path, mut targets: Vec<RunTarget>) -> Vec<RunTarget> {
    for target in &mut targets {
        for planned in &mut target.commands {
            if planned.relative_cwd.is_empty() {
                planned.relative_cwd = target.relative_root.clone();
            }
        }
        target.commands.sort_by_key(|planned| planned.phase.clone());
    }

    let tauri_pairs = targets
        .iter()
        .enumerate()
        .filter(|(_, target)| {
            target.stack == ProjectStack::Rust && target.kind == ProjectKind::Desktop
        })
        .filter_map(|(rust_index, rust)| {
            let parent = rust.relative_root.strip_suffix("/src-tauri").unwrap_or("");
            targets
                .iter()
                .enumerate()
                .find(|(_, node)| {
                    matches!(node.stack, ProjectStack::Node | ProjectStack::Bun)
                        && node.relative_root == parent
                })
                .map(|(node_index, _)| (node_index, rust_index))
        })
        .collect::<Vec<_>>();
    let mut removed = Vec::new();
    for (node_index, rust_index) in tauri_pairs {
        if removed.contains(&node_index) || removed.contains(&rust_index) {
            continue;
        }
        let node = targets[node_index].clone();
        let rust = targets[rust_index].clone();
        let mut composite = node.clone();
        composite.id = target_id(
            &ProjectStack::Rust,
            &format!("{}:tauri", node.relative_root),
        );
        composite.label = format!("{} desktop", node.label);
        composite.stack = ProjectStack::Rust;
        composite.kind = ProjectKind::Desktop;
        composite.role = TargetRole::Product;
        composite.components = vec![component_for(&node), component_for(&rust)];
        composite.selection_reason =
            "The Node frontend and src-tauri backend form one Tauri desktop product.".into();
        composite.commands = node
            .commands
            .into_iter()
            .chain(rust.commands.into_iter())
            .map(|mut planned| {
                planned.native = true;
                planned.network = NetworkPolicy::NativeUserConfirmed;
                planned
            })
            .collect();
        composite
            .commands
            .sort_by_key(|planned| planned.phase.clone());
        composite.blockers.extend(rust.blockers);
        composite.plan_status = if composite
            .blockers
            .iter()
            .any(|blocker| blocker.origin != BlockerOrigin::Oracle)
        {
            PlanStatus::Incomplete
        } else {
            PlanStatus::Complete
        };
        composite.oracle = if node.oracle.machine_verifiable && rust.oracle.machine_verifiable {
            VerificationOracle {
                kind: OracleKind::DeclaredSmoke,
                description:
                    "Frontend and Rust test oracles pass before the bounded desktop launch.".into(),
                machine_verifiable: true,
                evidence: node
                    .oracle
                    .evidence
                    .into_iter()
                    .chain(rust.oracle.evidence)
                    .collect(),
            }
        } else {
            VerificationOracle {
                kind: OracleKind::WindowSignal,
                description: "The composite desktop can be built and launched, but no complete machine oracle exists.".into(),
                machine_verifiable: false,
                evidence: Vec::new(),
            }
        };
        composite.oracle_status = oracle_status(&composite.oracle);
        targets[node_index] = composite;
        removed.push(rust_index);
    }
    removed.sort_unstable();
    removed.dedup();
    for index in removed.into_iter().rev() {
        targets.remove(index);
    }

    let workspace_products = targets
        .iter()
        .enumerate()
        .filter(|(_, target)| {
            matches!(target.stack, ProjectStack::Node | ProjectStack::Bun)
                && matches!(target.role, TargetRole::Product | TargetRole::Service)
                && target
                    .commands
                    .iter()
                    .any(|command| command.phase == RunPhase::Launch)
        })
        .filter_map(|(index, target)| {
            npm_workspace_matcher(root, &target.relative_root)
                .map(|matcher| (index, target.relative_root.clone(), matcher))
        })
        .collect::<Vec<_>>();
    for (product_index, workspace_root, matcher) in workspace_products {
        let members = targets
            .iter()
            .enumerate()
            .filter(|(index, target)| {
                if *index == product_index || target.relative_root.is_empty() {
                    return false;
                }
                let relative = if workspace_root.is_empty() {
                    target.relative_root.as_str()
                } else {
                    target
                        .relative_root
                        .strip_prefix(&format!("{workspace_root}/"))
                        .unwrap_or("")
                };
                !relative.is_empty() && matcher.is_match(relative)
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let components = members
            .iter()
            .map(|index| component_for(&targets[*index]))
            .collect::<Vec<_>>();
        for index in members {
            if !matches!(
                targets[index].role,
                TargetRole::Fixture | TargetRole::Example
            ) {
                targets[index].role = TargetRole::Component;
                targets[index].selection_reason =
                    "This npm workspace member is part of the root product target.".into();
            }
        }
        targets[product_index].components.extend(components);
        targets[product_index].selection_reason = format!(
            "{} Its declared npm workspaces are modeled as dependent components.",
            targets[product_index].selection_reason
        );
    }

    if targets
        .iter()
        .any(|target| target.kind == ProjectKind::Desktop && target.role == TargetRole::Product)
    {
        for target in &mut targets {
            if target.stack == ProjectStack::StaticWeb {
                target.role = TargetRole::Component;
                target.selection_reason = "This static surface is shipped alongside the desktop product and is shown as an advanced component.".into();
            }
        }
    }

    let compose_roots = targets
        .iter()
        .filter(|target| target.stack == ProjectStack::Compose)
        .map(|target| target.relative_root.clone())
        .collect::<Vec<_>>();
    for target in &mut targets {
        if target.stack != ProjectStack::Compose
            && compose_roots.iter().any(|root_path| {
                root_path.is_empty() || target.relative_root.starts_with(root_path)
            })
        {
            if !matches!(target.role, TargetRole::Fixture | TargetRole::Example) {
                target.role = TargetRole::Component;
                target.selection_reason =
                    "This component is included in the repository's Compose product topology."
                        .into();
            }
        }
    }
    for compose_root in &compose_roots {
        let components = targets
            .iter()
            .filter(|target| {
                target.stack != ProjectStack::Compose
                    && (compose_root.is_empty() || target.relative_root.starts_with(compose_root))
            })
            .map(component_for)
            .collect::<Vec<_>>();
        if let Some(compose) = targets.iter_mut().find(|target| {
            target.stack == ProjectStack::Compose && &target.relative_root == compose_root
        }) {
            compose.components = components;
        }
    }

    let candidates = targets
        .iter()
        .enumerate()
        .filter(|(_, target)| {
            target.plan_status == PlanStatus::Complete
                && matches!(target.role, TargetRole::Product | TargetRole::Service)
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if candidates.len() == 1 {
        let target = &mut targets[candidates[0]];
        target.recommended = true;
        target.selection_reason = format!(
            "{} It is the only complete high-confidence product target.",
            target.selection_reason
        );
    }
    targets.sort_by(|a, b| {
        b.recommended
            .cmp(&a.recommended)
            .then(a.role.cmp(&b.role))
            .then(a.relative_root.cmp(&b.relative_root))
            .then(a.stack.cmp(&b.stack))
    });
    let _ = root;
    targets
}

pub fn inspect_repository(root: &Path) -> Result<RunPlan, InspectError> {
    let paths = manifest_paths(root).map_err(|error| InspectError::Manifest {
        path: root.display().to_string(),
        message: error.to_string(),
    })?;
    let mut plan_hasher = Sha256::new();
    for path in &paths {
        plan_hasher.update(
            path.strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/")
                .as_bytes(),
        );
        plan_hasher.update([0]);
        if let Ok(bytes) = fs::read(path) {
            plan_hasher.update(bytes);
        }
        plan_hasher.update([0xff]);
    }
    let fingerprint = hex::encode(plan_hasher.finalize());
    let mut targets = Vec::new();
    let mut compose_roots = HashSet::new();
    let pom_roots = paths
        .iter()
        .filter(|path| path.file_name().and_then(|value| value.to_str()) == Some("pom.xml"))
        .filter_map(|path| path.parent().map(Path::to_path_buf))
        .collect::<HashSet<_>>();
    let solution_roots = paths
        .iter()
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("sln"))
        .filter_map(|path| path.parent().map(Path::to_path_buf))
        .collect::<Vec<_>>();
    let mut native_roots = HashSet::new();
    let mut dotnet_roots = HashSet::new();
    for path in paths {
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        match name {
            "package.json" => targets.push(node_target(root, &path)?),
            "deno.json" | "deno.jsonc" => targets.push(deno_target(root, &path)?),
            "Cargo.toml" => targets.push(rust_target(root, &path)),
            "pyproject.toml" | "requirements.txt" => {
                if !targets.iter().any(|target: &RunTarget| {
                    target.stack == ProjectStack::Python
                        && target.relative_root == relative(root, &path)
                }) {
                    targets.push(python_target(root, &path));
                }
            }
            "go.mod" => targets.push(go_target(root, &path)),
            "project.godot" => targets.push(godot_target(root, &path)),
            "compose.yaml" | "compose.yml" | "docker-compose.yml" => {
                let rel = relative(root, &path);
                if compose_roots.insert(rel) {
                    targets.push(compose_target(root, &path)?)
                }
            }
            "pom.xml" => targets.push(jvm_target(root, &path)?),
            "build.gradle" | "build.gradle.kts" => {
                if !pom_roots.contains(path.parent().unwrap_or(root)) {
                    targets.push(jvm_target(root, &path)?);
                }
            }
            "CMakeLists.txt" | "meson.build" | "Makefile" => {
                let rel = relative(root, &path);
                if native_roots.insert(rel) {
                    targets.push(native_build_target(root, &path)?);
                }
            }
            "composer.json" => targets.push(php_target(root, &path)?),
            "Gemfile" => targets.push(ruby_target(root, &path)),
            _ if matches!(
                path.extension().and_then(|value| value.to_str()),
                Some("sln" | "csproj" | "fsproj")
            ) =>
            {
                let dir = path.parent().unwrap_or(root);
                let covered_by_solution = path.extension().and_then(|value| value.to_str())
                    != Some("sln")
                    && solution_roots
                        .iter()
                        .any(|solution| dir.starts_with(solution));
                let rel = relative(root, &path);
                if !covered_by_solution && dotnet_roots.insert(rel) {
                    targets.push(dotnet_target(root, &path));
                }
            }
            _ => {}
        }
    }
    for path in static_entry_paths(root).map_err(|error| InspectError::Manifest {
        path: root.display().to_string(),
        message: error.to_string(),
    })? {
        targets.push(static_web_target(root, &path));
    }
    if targets.is_empty() {
        return Err(InspectError::Unsupported);
    }
    let targets = normalize_and_group_targets(root, targets);
    let selection_ambiguity = (targets
        .iter()
        .filter(|target| {
            target.plan_status == PlanStatus::Complete
                && matches!(target.role, TargetRole::Product | TargetRole::Service)
        })
        .count()
        > 1) as usize;
    let ambiguity_count = selection_ambiguity
        + targets
            .iter()
            .filter(|target| {
                target
                    .blockers
                    .iter()
                    .any(|blocker| blocker.code.contains("ambiguous"))
            })
            .count();
    let absolute_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    Ok(RunPlan {
        schema: RUN_PLAN_SCHEMA.into(),
        repository_root: absolute_root.display().to_string(),
        repository_name: absolute_root
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("repository")
            .into(),
        inspection_fingerprint: fingerprint,
        generated_at: Utc::now().to_rfc3339(),
        targets,
        ambiguity_count,
        source_scope: "git_tracked_and_non_ignored_source".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn node_without_lock_or_tests_is_blocked() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"sample","scripts":{"dev":"vite"},"devDependencies":{"vite":"1"}}"#,
        )
        .unwrap();
        let plan = inspect_repository(dir.path()).unwrap();
        assert_eq!(plan.targets[0].plan_status, PlanStatus::Incomplete);
        assert!(plan.targets[0]
            .blockers
            .iter()
            .any(|item| item.code == "node_lockfile_missing"));
        assert!(plan.targets[0]
            .blockers
            .iter()
            .filter(|item| item.origin != BlockerOrigin::Oracle)
            .all(|item| item.phase == RunPhase::Acquire));
    }

    #[test]
    fn node_with_lock_and_unique_launch_is_ready_but_limited() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"sample","scripts":{"dev":"vite"},"devDependencies":{"vite":"1"}}"#,
        )
        .unwrap();
        fs::write(
            dir.path().join("package-lock.json"),
            r#"{"lockfileVersion":3,"packages":{"":{"devDependencies":{"vite":"1"}},"node_modules/vite":{"version":"1.0.0"}}}"#,
        )
        .unwrap();
        let plan = inspect_repository(dir.path()).unwrap();
        assert_eq!(plan.targets[0].plan_status, PlanStatus::Complete);
        assert!(!plan.targets[0].oracle.machine_verifiable);
        assert!(plan.targets[0].blockers.is_empty());
        assert!(plan.targets[0]
            .commands
            .iter()
            .any(|command| command.phase == RunPhase::Launch));
    }

    #[test]
    fn node_with_lock_and_test_has_machine_oracle() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("package.json"), r#"{"name":"sample","scripts":{"dev":"vite","build":"vite build","test":"vitest run"},"devDependencies":{"vite":"1"}}"#).unwrap();
        fs::write(
            dir.path().join("package-lock.json"),
            r#"{"lockfileVersion":3,"packages":{"":{"devDependencies":{"vite":"1"}},"node_modules/vite":{"version":"1.0.0"}}}"#,
        )
        .unwrap();
        let plan = inspect_repository(dir.path()).unwrap();
        assert_eq!(plan.targets[0].plan_status, PlanStatus::Complete);
        assert_eq!(plan.targets[0].oracle.kind, OracleKind::HttpHtml);
    }

    #[test]
    fn rust_requires_a_resolved_lock() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname='sample'\nversion='0.1.0'\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("src/lib.rs"),
            "pub fn ok() -> bool { true }",
        )
        .unwrap();
        let blocked = inspect_repository(dir.path()).unwrap();
        assert_eq!(blocked.targets[0].blockers[0].code, "rust_lockfile_missing");
        fs::write(dir.path().join("Cargo.lock"), "# generated").unwrap();
        let ready = inspect_repository(dir.path()).unwrap();
        assert_eq!(ready.targets[0].plan_status, PlanStatus::Complete);
        assert!(ready.targets[0]
            .commands
            .iter()
            .all(|command| !command.native));
    }

    #[test]
    fn python_and_go_do_not_guess_past_missing_resolution_evidence() {
        let python = tempfile::tempdir().unwrap();
        fs::write(
            python.path().join("pyproject.toml"),
            "[project]\nname='sample'\ndependencies=['pytest']\n",
        )
        .unwrap();
        fs::create_dir(python.path().join("tests")).unwrap();
        assert!(inspect_repository(python.path()).unwrap().targets[0]
            .blockers
            .iter()
            .any(|item| item.code == "python_lockfile_missing"));

        let go = tempfile::tempdir().unwrap();
        fs::write(
            go.path().join("go.mod"),
            "module example.test/sample\n\ngo 1.24\n",
        )
        .unwrap();
        assert!(inspect_repository(go.path()).unwrap().targets[0]
            .blockers
            .iter()
            .any(|item| item.code == "go_sum_missing"));
    }

    #[test]
    fn godot_test_directory_alone_is_not_a_machine_oracle() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("project.godot"),
            "[application]\nrun/main_scene=\"res://main.tscn\"\n",
        )
        .unwrap();
        fs::create_dir(dir.path().join("tests")).unwrap();
        let plan = inspect_repository(dir.path()).unwrap();
        assert_eq!(plan.targets[0].plan_status, PlanStatus::Complete);
        assert!(!plan.targets[0].oracle.machine_verifiable);
        assert!(plan.targets[0].blockers.is_empty());
        assert!(plan.targets[0]
            .commands
            .iter()
            .any(|command| command.phase == RunPhase::Launch && command.native));
    }

    #[test]
    fn checked_in_gut_runner_is_a_traceable_godot_oracle() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("project.godot"),
            "[application]\nrun/main_scene=\"res://main.tscn\"\n",
        )
        .unwrap();
        fs::create_dir_all(dir.path().join("addons/gut")).unwrap();
        fs::write(
            dir.path().join("addons/gut/gut_cmdln.gd"),
            "extends SceneTree\n",
        )
        .unwrap();
        let plan = inspect_repository(dir.path()).unwrap();
        assert_eq!(plan.targets[0].plan_status, PlanStatus::Complete);
        assert!(plan.targets[0]
            .commands
            .iter()
            .any(|command| command.phase == RunPhase::Test && command.native));
    }

    #[test]
    fn damaged_node_manifest_is_reported_instead_of_skipped() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("package.json"), "{not-json").unwrap();
        assert!(matches!(
            inspect_repository(dir.path()),
            Err(InspectError::Manifest { .. })
        ));
    }

    #[test]
    fn tauri_frontend_and_backend_form_one_product_target() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("desktop/src-tauri/src")).unwrap();
        fs::write(dir.path().join("desktop/package.json"), r#"{"name":"desk","scripts":{"build":"vite build","dev":"vite"},"dependencies":{"react":"1"}}"#).unwrap();
        fs::write(
            dir.path().join("desktop/package-lock.json"),
            r#"{"lockfileVersion":3,"packages":{"":{"dependencies":{"react":"1"}},"node_modules/react":{"version":"1.0.0"}}}"#,
        )
        .unwrap();
        fs::write(
            dir.path().join("desktop/src-tauri/Cargo.toml"),
            "[package]\nname='desk'\nversion='0.1.0'\n",
        )
        .unwrap();
        fs::write(dir.path().join("desktop/src-tauri/Cargo.lock"), "# lock").unwrap();
        fs::write(dir.path().join("desktop/src-tauri/tauri.conf.json"), "{}").unwrap();
        fs::write(
            dir.path().join("desktop/src-tauri/src/main.rs"),
            "fn main() {}",
        )
        .unwrap();
        let plan = inspect_repository(dir.path()).unwrap();
        let products = plan
            .targets
            .iter()
            .filter(|target| target.role == TargetRole::Product)
            .collect::<Vec<_>>();
        assert_eq!(products.len(), 1);
        assert_eq!(products[0].kind, ProjectKind::Desktop);
        assert_eq!(products[0].components.len(), 2);
        assert!(products[0].recommended);
        assert!(products[0].commands.iter().all(|command| command.native));
    }

    #[test]
    fn compose_is_the_product_and_fixture_is_advanced() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("compose.yaml"),
            "services:\n  web:\n    build: .\n    healthcheck:\n      test: ['CMD', 'true']\n",
        )
        .unwrap();
        fs::create_dir_all(dir.path().join("tests/fixtures/web")).unwrap();
        fs::write(
            dir.path().join("tests/fixtures/web/package.json"),
            r#"{"name":"fixture","scripts":{"start":"node app.js"}}"#,
        )
        .unwrap();
        let plan = inspect_repository(dir.path()).unwrap();
        assert_eq!(
            plan.targets
                .iter()
                .filter(|target| target.recommended)
                .count(),
            1
        );
        assert_eq!(
            plan.targets
                .iter()
                .find(|target| target.recommended)
                .unwrap()
                .stack,
            ProjectStack::Compose
        );
        assert_eq!(
            plan.targets
                .iter()
                .find(|target| target.role == TargetRole::Fixture)
                .unwrap()
                .recommended,
            false
        );
    }

    #[test]
    fn npm_lock_mismatch_is_a_repository_blocker() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"web","scripts":{"dev":"vite"},"dependencies":{"vite":"2"}}"#,
        )
        .unwrap();
        fs::write(
            dir.path().join("package-lock.json"),
            r#"{"lockfileVersion":3,"packages":{"":{"dependencies":{"vite":"1"}}}}"#,
        )
        .unwrap();
        let target = inspect_repository(dir.path()).unwrap().targets.remove(0);
        let blocker = target
            .blockers
            .iter()
            .find(|blocker| blocker.code == "node_lockfile_out_of_sync")
            .unwrap();
        assert_eq!(blocker.origin, BlockerOrigin::Repository);
        assert_eq!(target.plan_status, PlanStatus::Incomplete);
    }

    #[test]
    fn npm_lock_missing_a_transitive_package_is_incomplete() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"web","scripts":{"dev":"vite"},"dependencies":{"vite":"1"}}"#,
        )
        .unwrap();
        fs::write(
            dir.path().join("package-lock.json"),
            r#"{"lockfileVersion":3,"packages":{"":{"dependencies":{"vite":"1"}},"node_modules/vite":{"version":"1.0.0","dependencies":{"missing-transitive":"1"}}}}"#,
        )
        .unwrap();
        let target = inspect_repository(dir.path()).unwrap().targets.remove(0);
        assert!(target
            .blockers
            .iter()
            .any(|blocker| blocker.code == "node_lockfile_out_of_sync"));
        assert_eq!(target.plan_status, PlanStatus::Incomplete);
    }

    #[test]
    fn deno_tasks_and_lock_create_a_traceable_target() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("deno.json"),
            r#"{"tasks":{"test":"deno test","start":"deno run --allow-net main.ts"}}"#,
        )
        .unwrap();
        fs::write(dir.path().join("deno.lock"), "{}").unwrap();
        fs::write(
            dir.path().join("main.ts"),
            "Deno.serve(() => new Response('ok'));",
        )
        .unwrap();
        let plan = inspect_repository(dir.path()).unwrap();
        assert_eq!(plan.targets[0].stack, ProjectStack::Deno);
        assert_eq!(plan.targets[0].plan_status, PlanStatus::Complete);
        assert_eq!(plan.targets[0].oracle_status, OracleStatus::Machine);
    }

    #[test]
    fn npm_workspace_members_are_components_of_the_root_product() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("apps/web")).unwrap();
        fs::create_dir_all(dir.path().join("packages/shared")).unwrap();
        fs::write(
            dir.path().join("package.json"),
            r#"{"name":"workspace-product","workspaces":["apps/*","packages/*"],"scripts":{"start":"npm --prefix apps/web run start"}}"#,
        )
        .unwrap();
        fs::write(
            dir.path().join("package-lock.json"),
            r#"{"lockfileVersion":3,"packages":{"":{"name":"workspace-product"}}}"#,
        )
        .unwrap();
        fs::write(
            dir.path().join("apps/web/package.json"),
            r#"{"name":"web","scripts":{"start":"node server.js"}}"#,
        )
        .unwrap();
        fs::write(
            dir.path().join("packages/shared/package.json"),
            r#"{"name":"shared"}"#,
        )
        .unwrap();

        let plan = inspect_repository(dir.path()).unwrap();
        let product = plan
            .targets
            .iter()
            .find(|target| target.recommended)
            .unwrap();
        assert_eq!(product.label, "workspace-product");
        assert_eq!(product.role, TargetRole::Service);
        assert_eq!(product.components.len(), 2);
        assert!(plan
            .targets
            .iter()
            .filter(|target| !target.relative_root.is_empty())
            .all(|target| target.role == TargetRole::Component));
    }

    #[test]
    fn maven_and_gradle_require_traceable_build_contracts() {
        let maven = tempfile::tempdir().unwrap();
        fs::create_dir_all(maven.path().join("src/test/java")).unwrap();
        fs::write(maven.path().join("pom.xml"), "<project><dependencies><dependency><groupId>org.junit.jupiter</groupId><artifactId>junit-jupiter</artifactId><version>5.11.0</version></dependency></dependencies></project>").unwrap();
        let target = inspect_repository(maven.path()).unwrap().targets.remove(0);
        assert_eq!(target.stack, ProjectStack::Java);
        assert_eq!(target.plan_status, PlanStatus::Complete);
        assert_eq!(target.oracle_status, OracleStatus::Machine);
        assert!(target
            .commands
            .iter()
            .any(|item| item.program == "mvn" && item.phase == RunPhase::Acquire));

        let gradle = tempfile::tempdir().unwrap();
        fs::write(
            gradle.path().join("build.gradle.kts"),
            "plugins { kotlin(\"jvm\") version \"2.0.0\" }",
        )
        .unwrap();
        let blocked = inspect_repository(gradle.path()).unwrap().targets.remove(0);
        assert_eq!(blocked.stack, ProjectStack::Kotlin);
        assert!(blocked
            .blockers
            .iter()
            .any(|item| item.code == "gradle_wrapper_missing"));
        fs::create_dir_all(gradle.path().join("gradle/wrapper")).unwrap();
        fs::write(gradle.path().join("gradlew"), "#!/bin/sh").unwrap();
        fs::write(
            gradle
                .path()
                .join("gradle/wrapper/gradle-wrapper.properties"),
            "distributionUrl=https://services.gradle.org/distributions/gradle-8.10-bin.zip",
        )
        .unwrap();
        fs::write(
            gradle.path().join("gradle/wrapper/gradle-wrapper.jar"),
            b"wrapper",
        )
        .unwrap();
        let complete = inspect_repository(gradle.path()).unwrap().targets.remove(0);
        assert_eq!(complete.plan_status, PlanStatus::Complete);
        assert!(complete.commands.iter().all(|item| !item.native));
    }

    #[test]
    fn native_build_systems_are_explicit_confirmed_targets() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::create_dir_all(dir.path().join("tests")).unwrap();
        fs::write(dir.path().join("src/main.cpp"), "int main(){return 0;}").unwrap();
        fs::write(dir.path().join("CMakeLists.txt"), "cmake_minimum_required(VERSION 3.24)\nproject(sample)\nadd_executable(sample src/main.cpp)\nenable_testing()\nadd_test(NAME smoke COMMAND sample)\n").unwrap();
        let target = inspect_repository(dir.path()).unwrap().targets.remove(0);
        assert_eq!(target.stack, ProjectStack::Cpp);
        assert_eq!(target.plan_status, PlanStatus::Complete);
        assert_eq!(target.oracle.kind, OracleKind::TestSuite);
        assert!(target.commands.iter().all(|item| item.native));
        assert!(target.commands.iter().any(|item| item.program == "ctest"));
    }

    #[test]
    fn dotnet_php_and_ruby_require_locked_dependency_graphs() {
        let unlocked_dotnet = tempfile::tempdir().unwrap();
        fs::write(
            unlocked_dotnet.path().join("App.csproj"),
            "<Project Sdk=\"Microsoft.NET.Sdk\"></Project>",
        )
        .unwrap();
        let unlocked_dotnet_target = inspect_repository(unlocked_dotnet.path())
            .unwrap()
            .targets
            .remove(0);
        assert!(unlocked_dotnet_target
            .blockers
            .iter()
            .any(|item| item.code == "dotnet_lockfile_missing"));

        let dotnet = tempfile::tempdir().unwrap();
        fs::write(dotnet.path().join("App.csproj"), "<Project Sdk=\"Microsoft.NET.Sdk.Web\"><ItemGroup><PackageReference Include=\"Microsoft.NET.Test.Sdk\" Version=\"17.11.1\" /></ItemGroup></Project>").unwrap();
        fs::write(dotnet.path().join("packages.lock.json"), "{}").unwrap();
        let dotnet_target = inspect_repository(dotnet.path()).unwrap().targets.remove(0);
        assert_eq!(dotnet_target.stack, ProjectStack::DotNet);
        assert_eq!(dotnet_target.plan_status, PlanStatus::Complete);
        assert_eq!(dotnet_target.oracle.kind, OracleKind::TestSuite);

        let php = tempfile::tempdir().unwrap();
        fs::write(
            php.path().join("composer.json"),
            r#"{"name":"sample/app","scripts":{"test":"phpunit"}}"#,
        )
        .unwrap();
        fs::write(php.path().join("composer.lock"), "{}").unwrap();
        let php_target = inspect_repository(php.path()).unwrap().targets.remove(0);
        assert_eq!(php_target.stack, ProjectStack::Php);
        assert_eq!(php_target.oracle.kind, OracleKind::TestSuite);

        let php_without_oracle = tempfile::tempdir().unwrap();
        fs::write(
            php_without_oracle.path().join("composer.json"),
            r#"{"name":"sample/library"}"#,
        )
        .unwrap();
        fs::write(php_without_oracle.path().join("composer.lock"), "{}").unwrap();
        let php_without_oracle_target = inspect_repository(php_without_oracle.path())
            .unwrap()
            .targets
            .remove(0);
        assert_eq!(
            php_without_oracle_target.plan_status,
            PlanStatus::Incomplete
        );
        assert!(php_without_oracle_target
            .blockers
            .iter()
            .any(|item| item.code == "php_machine_oracle_missing"));

        let ruby = tempfile::tempdir().unwrap();
        fs::create_dir(ruby.path().join("spec")).unwrap();
        fs::write(
            ruby.path().join("Gemfile"),
            "source 'https://rubygems.org'\ngem 'rspec'\n",
        )
        .unwrap();
        fs::write(ruby.path().join("Gemfile.lock"), "GEM\n").unwrap();
        let ruby_target = inspect_repository(ruby.path()).unwrap().targets.remove(0);
        assert_eq!(ruby_target.stack, ProjectStack::Ruby);
        assert_eq!(ruby_target.plan_status, PlanStatus::Complete);
        assert_eq!(ruby_target.oracle.kind, OracleKind::TestSuite);

        let ruby_without_oracle = tempfile::tempdir().unwrap();
        fs::write(
            ruby_without_oracle.path().join("Gemfile"),
            "source 'https://rubygems.org'\n",
        )
        .unwrap();
        fs::write(ruby_without_oracle.path().join("Gemfile.lock"), "GEM\n").unwrap();
        let ruby_without_oracle_target = inspect_repository(ruby_without_oracle.path())
            .unwrap()
            .targets
            .remove(0);
        assert_eq!(
            ruby_without_oracle_target.plan_status,
            PlanStatus::Incomplete
        );
        assert!(ruby_without_oracle_target
            .blockers
            .iter()
            .any(|item| item.code == "ruby_machine_oracle_missing"));
    }
}
