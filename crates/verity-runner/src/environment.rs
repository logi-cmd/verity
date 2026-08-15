// SPDX-License-Identifier: MPL-2.0

use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};
use verity_core::{
    CapabilityCheck, CapabilityState, RuntimeCapability, RuntimeStatus, RUNTIME_CAPABILITY_SCHEMA,
};

fn output(program: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Some(if stdout.is_empty() { stderr } else { stdout })
}

fn check(state: CapabilityState, version: impl Into<String>, reason: &str) -> CapabilityCheck {
    CapabilityCheck {
        state,
        version: version.into(),
        reason_code: reason.into(),
    }
}

fn probe_buildkit(cli_path: &Path) -> bool {
    let temp = match tempfile::TempDir::new() {
        Ok(value) => value,
        Err(_) => return false,
    };
    if std::fs::write(
        temp.path().join("Dockerfile"),
        "FROM scratch\nLABEL dev.verity.probe=true\n",
    )
    .is_err()
    {
        return false;
    }
    Command::new(cli_path)
        .args([
            "buildx",
            "build",
            "--progress",
            "quiet",
            "--output",
            "type=cacheonly",
        ])
        .arg(temp.path())
        .output()
        .is_ok_and(|output| output.status.success())
}

#[cfg(target_os = "windows")]
fn docker_desktop_path() -> Option<PathBuf> {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    use winreg::RegKey;

    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    if let Ok(key) =
        hklm.open_subkey(r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\Docker Desktop")
    {
        if let Ok(location) = key.get_value::<String, _>("InstallLocation") {
            let candidate = PathBuf::from(location).join("Docker Desktop.exe");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    let candidate = PathBuf::from(r"C:\Program Files\Docker\Docker\Docker Desktop.exe");
    candidate.is_file().then_some(candidate)
}

#[cfg(not(target_os = "windows"))]
fn docker_desktop_path() -> Option<PathBuf> {
    None
}

fn resolve_docker_cli() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let result = Command::new("where.exe").arg("docker.exe").output().ok()?;
        if result.status.success() {
            return String::from_utf8_lossy(&result.stdout)
                .lines()
                .map(str::trim)
                .map(PathBuf::from)
                .find(|path| path.is_file());
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let result = Command::new("sh")
            .args(["-lc", "command -v docker"])
            .output()
            .ok()?;
        if result.status.success() {
            let path = PathBuf::from(String::from_utf8_lossy(&result.stdout).trim());
            return path.is_file().then_some(path);
        }
    }
    None
}

pub fn runtime_doctor() -> RuntimeCapability {
    let desktop = docker_desktop_path();
    let cli_path = resolve_docker_cli();
    let installed = desktop.is_some() || cli_path.is_some();
    if !installed {
        return RuntimeCapability {
            schema: RUNTIME_CAPABILITY_SCHEMA.into(),
            provider: "docker_desktop".into(),
            status: RuntimeStatus::NotInstalled,
            installed: false,
            launchable: false,
            cli: check(CapabilityState::Unavailable, "", "docker_cli_not_found"),
            engine: check(CapabilityState::NotChecked, "", "docker_engine_not_checked"),
            buildkit: check(
                CapabilityState::NotChecked,
                "",
                "docker_buildkit_not_checked",
            ),
            internal_network: check(
                CapabilityState::NotChecked,
                "",
                "docker_network_not_checked",
            ),
            resource_limits: check(CapabilityState::NotChecked, "", "docker_limits_not_checked"),
            reason_code: "docker_desktop_not_installed".into(),
        };
    }

    let Some(cli_path) = cli_path else {
        return RuntimeCapability {
            schema: RUNTIME_CAPABILITY_SCHEMA.into(),
            provider: "docker_desktop".into(),
            status: RuntimeStatus::CapabilityIncomplete,
            installed: true,
            launchable: desktop.is_some(),
            cli: check(CapabilityState::Unavailable, "", "docker_cli_not_found"),
            engine: check(CapabilityState::NotChecked, "", "docker_engine_not_checked"),
            buildkit: check(
                CapabilityState::NotChecked,
                "",
                "docker_buildkit_not_checked",
            ),
            internal_network: check(
                CapabilityState::NotChecked,
                "",
                "docker_network_not_checked",
            ),
            resource_limits: check(CapabilityState::NotChecked, "", "docker_limits_not_checked"),
            reason_code: "docker_cli_not_found".into(),
        };
    };

    let cli_version = output(&cli_path, &["--version"]).unwrap_or_default();
    let engine_version = output(&cli_path, &["version", "--format", "{{.Server.Version}}"]);
    if engine_version.is_none() {
        return RuntimeCapability {
            schema: RUNTIME_CAPABILITY_SCHEMA.into(),
            provider: "docker_desktop".into(),
            status: if desktop.is_some() {
                RuntimeStatus::Stopped
            } else {
                RuntimeStatus::DaemonUnreachable
            },
            installed: true,
            launchable: desktop.is_some(),
            cli: check(CapabilityState::Available, cli_version, "docker_cli_ready"),
            engine: check(
                CapabilityState::Unavailable,
                "",
                "docker_engine_unreachable",
            ),
            buildkit: check(
                CapabilityState::NotChecked,
                "",
                "docker_buildkit_not_checked",
            ),
            internal_network: check(
                CapabilityState::NotChecked,
                "",
                "docker_network_not_checked",
            ),
            resource_limits: check(CapabilityState::NotChecked, "", "docker_limits_not_checked"),
            reason_code: "docker_desktop_stopped".into(),
        };
    }

    let engine_version = engine_version.unwrap_or_default();
    let buildkit_version = output(&cli_path, &["buildx", "version"]);
    if buildkit_version.is_none() || !probe_buildkit(&cli_path) {
        return RuntimeCapability {
            schema: RUNTIME_CAPABILITY_SCHEMA.into(),
            provider: "docker_desktop".into(),
            status: RuntimeStatus::BuildkitUnavailable,
            installed: true,
            launchable: desktop.is_some(),
            cli: check(CapabilityState::Available, cli_version, "docker_cli_ready"),
            engine: check(
                CapabilityState::Available,
                engine_version,
                "docker_engine_ready",
            ),
            buildkit: check(
                CapabilityState::Unavailable,
                "",
                "docker_buildkit_unavailable",
            ),
            internal_network: check(
                CapabilityState::NotChecked,
                "",
                "docker_network_not_checked",
            ),
            resource_limits: check(CapabilityState::NotChecked, "", "docker_limits_not_checked"),
            reason_code: "docker_buildkit_unavailable".into(),
        };
    }

    let probe_name = format!("verity-probe-{}", std::process::id());
    let network_ready =
        output(&cli_path, &["network", "create", "--internal", &probe_name]).is_some();
    if network_ready {
        let _ = output(&cli_path, &["network", "rm", &probe_name]);
    }
    let limits_ready = output(
        &cli_path,
        &["info", "--format", "{{.MemoryLimit}}/{{.CPUCfsPeriod}}"],
    )
    .is_some_and(|value| value.trim().eq_ignore_ascii_case("true/true"));
    let ready = network_ready && limits_ready;
    RuntimeCapability {
        schema: RUNTIME_CAPABILITY_SCHEMA.into(),
        provider: "docker_desktop".into(),
        status: if ready {
            RuntimeStatus::Ready
        } else {
            RuntimeStatus::CapabilityIncomplete
        },
        installed: true,
        launchable: desktop.is_some(),
        cli: check(CapabilityState::Available, cli_version, "docker_cli_ready"),
        engine: check(
            CapabilityState::Available,
            engine_version,
            "docker_engine_ready",
        ),
        buildkit: check(
            CapabilityState::Available,
            buildkit_version.unwrap_or_default(),
            "docker_buildkit_ready",
        ),
        internal_network: check(
            if network_ready {
                CapabilityState::Available
            } else {
                CapabilityState::Unavailable
            },
            "",
            if network_ready {
                "docker_network_ready"
            } else {
                "docker_network_probe_failed"
            },
        ),
        resource_limits: check(
            if limits_ready {
                CapabilityState::Available
            } else {
                CapabilityState::Unavailable
            },
            "",
            if limits_ready {
                "docker_limits_ready"
            } else {
                "docker_limits_probe_failed"
            },
        ),
        reason_code: if ready {
            "docker_ready"
        } else {
            "docker_capability_incomplete"
        }
        .into(),
    }
}

pub fn start_docker_desktop(timeout: Duration) -> Result<RuntimeCapability, String> {
    let executable =
        docker_desktop_path().ok_or_else(|| "docker_desktop_not_installed".to_string())?;
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        Command::new(&executable)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|_| "docker_desktop_start_failed".to_string())?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        Command::new(&executable)
            .spawn()
            .map_err(|_| "docker_desktop_start_failed".to_string())?;
    }
    let started = Instant::now();
    loop {
        let capability = runtime_doctor();
        if capability.status == RuntimeStatus::Ready {
            return Ok(capability);
        }
        if started.elapsed() >= timeout {
            return Err("docker_desktop_start_timeout".into());
        }
        thread::sleep(Duration::from_millis(750));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_reason_codes_are_stable() {
        let result = runtime_doctor();
        assert_eq!(result.schema, "verity-runtime-capability.v2");
        assert!(!result.reason_code.trim().is_empty());
        assert_eq!(result.provider, "docker_desktop");
    }
}
