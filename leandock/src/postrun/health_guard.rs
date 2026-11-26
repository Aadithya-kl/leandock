use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::error::{LeanDockError, Result};
use crate::profiler::ComposeDriver;

/// Status snapshot of a single container monitored by the Health Verification Guard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerHealthState {
    pub id: String,
    pub name: String,
    pub status: String,
    pub restart_count: u32,
    pub exit_code: i32,
    pub health: Option<String>,
}

/// Result of running the post-deploy health verification window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthGuardReport {
    pub healthy: bool,
    pub checked_containers: Vec<ContainerHealthState>,
    pub failing_service: Option<String>,
    pub failing_logs_tail: Vec<String>,
    pub rolled_back: bool,
}

/// Evaluates whether a container snapshot indicates a crash-loop or non-zero exit failure.
pub fn is_container_failing(state: &ContainerHealthState) -> bool {
    let lower_status = state.status.to_lowercase();
    if lower_status == "restarting" || lower_status == "dead" || lower_status == "oomkilled" {
        return true;
    }
    if lower_status == "exited" && state.exit_code != 0 {
        return true;
    }
    if state.restart_count >= 2 {
        return true;
    }
    if let Some(ref h) = state.health {
        if h.eq_ignore_ascii_case("unhealthy") {
            return true;
        }
    }
    false
}

/// Monitors all containers in the Compose project for `stability_window` (default 10s).
/// If any container enters crash-loop or exits non-zero, captures the last 30 lines of logs
/// and — when `auto_rollback` is enabled — immediately redeploys the untouched original Compose file.
pub fn verify_deployment_health(
    driver: &ComposeDriver,
    runtime_bin: &str,
    compose_file: &Path,
    override_file: &Path,
    stability_window: Duration,
    auto_rollback: bool,
) -> Result<HealthGuardReport> {
    let start = Instant::now();
    let poll_interval = Duration::from_secs(2);
    let mut last_states = Vec::new();

    while start.elapsed() < stability_window {
        let ids = list_compose_container_ids(driver, compose_file, Some(override_file))?;
        let mut states = Vec::with_capacity(ids.len());

        for cid in ids {
            if let Some(st) = inspect_container_state(runtime_bin, &cid) {
                if is_container_failing(&st) {
                    let logs = fetch_container_logs_tail(runtime_bin, &cid, 30);
                    let mut rolled_back = false;

                    if auto_rollback {
                        rolled_back = rollback_to_raw_compose(driver, compose_file).is_ok();
                    }

                    states.push(st.clone());
                    return Ok(HealthGuardReport {
                        healthy: false,
                        checked_containers: states,
                        failing_service: Some(st.name),
                        failing_logs_tail: logs,
                        rolled_back,
                    });
                }
                states.push(st);
            }
        }

        last_states = states;
        if start.elapsed() + poll_interval < stability_window {
            std::thread::sleep(poll_interval);
        } else {
            break;
        }
    }

    Ok(HealthGuardReport {
        healthy: true,
        checked_containers: last_states,
        failing_service: None,
        failing_logs_tail: Vec::new(),
        rolled_back: false,
    })
}

fn list_compose_container_ids(
    driver: &ComposeDriver,
    compose_file: &Path,
    override_file: Option<&Path>,
) -> Result<Vec<String>> {
    let Some((bin, mut prefix)) = driver.command_prefix() else {
        return Err(LeanDockError::RuntimeNotFound);
    };

    let compose_str = compose_file.to_string_lossy().to_string();
    let override_str = override_file.map(|p| p.to_string_lossy().to_string());

    let mut args: Vec<String> = prefix.drain(..).map(|s| s.to_string()).collect();
    args.push("-f".to_string());
    args.push(compose_str);
    if let Some(ov) = override_str {
        args.push("-f".to_string());
        args.push(ov);
    }
    args.push("ps".to_string());
    args.push("-a".to_string());
    args.push("-q".to_string());

    let out = Command::new(bin).args(&args).output()?;
    if !out.status.success() {
        return Ok(Vec::new());
    }

    let ids = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    Ok(ids)
}

fn inspect_container_state(runtime_bin: &str, container_id: &str) -> Option<ContainerHealthState> {
    let fmt = "{{.Name}}|{{.State.Status}}|{{.RestartCount}}|{{.State.ExitCode}}|{{if .State.Health}}{{.State.Health.Status}}{{end}}";
    let out = Command::new(runtime_bin)
        .args(["inspect", "--format", fmt, container_id])
        .output()
        .ok()?;

    if !out.status.success() {
        return None;
    }

    let line = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let parts: Vec<&str> = line.split('|').collect();
    if parts.len() < 4 {
        return None;
    }

    let health = parts
        .get(4)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    Some(ContainerHealthState {
        id: container_id.to_string(),
        name: parts[0].trim_start_matches('/').to_string(),
        status: parts[1].to_string(),
        restart_count: parts[2].parse::<u32>().unwrap_or(0),
        exit_code: parts[3].parse::<i32>().unwrap_or(0),
        health,
    })
}

fn fetch_container_logs_tail(runtime_bin: &str, container_id: &str, lines: usize) -> Vec<String> {
    let out = Command::new(runtime_bin)
        .args(["logs", "--tail", &lines.to_string(), container_id])
        .output();

    match out {
        Ok(o) => {
            let mut combined = String::from_utf8_lossy(&o.stdout).to_string();
            let err = String::from_utf8_lossy(&o.stderr);
            if !err.is_empty() {
                combined.push_str(&err);
            }
            combined.lines().map(|l| l.to_string()).collect()
        }
        Err(_) => Vec::new(),
    }
}

/// Falls back immediately to launching the unmodified user Compose file without `.leandock/compose.override.yml`.
pub fn rollback_to_raw_compose(driver: &ComposeDriver, compose_file: &Path) -> Result<()> {
    let Some((bin, prefix)) = driver.command_prefix() else {
        return Err(LeanDockError::RuntimeNotFound);
    };

    let mut args: Vec<String> = prefix.into_iter().map(|s| s.to_string()).collect();
    args.push("-f".to_string());
    args.push(compose_file.to_string_lossy().to_string());
    args.push("up".to_string());
    args.push("-d".to_string());
    args.push("--force-recreate".to_string());

    let out = Command::new(bin).args(&args).output()?;
    if !out.status.success() {
        return Err(LeanDockError::CommandFailed {
            cmd: format!("{} {}", bin, args.join(" ")),
            code: out.status.code(),
            stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        });
    }
    Ok(())
}
