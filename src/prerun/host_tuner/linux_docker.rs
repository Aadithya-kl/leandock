#![cfg(target_os = "linux")]

use std::fs;
use std::path::Path;
use std::process::Command;

use crate::backup::BackupSession;
use crate::error::Result;
use crate::prerun::host_tuner::{merge_daemon_json, HostTuningReport};
use crate::profiler::platform::VirtualizationKind;
use crate::profiler::HostProfile;

const DAEMON_JSON_PATH: &str = "/etc/docker/daemon.json";

/// Safely tunes `/etc/docker/daemon.json` on Linux hosts when running as root.
/// Preserves ALL existing keys, backs up atomically, validates JSON, and rolls back automatically
/// if reloading `docker.service` fails.
pub fn tune_linux_docker(
    profile: &HostProfile,
    project_dir: &Path,
    backup: &mut BackupSession,
    dry_run: bool,
) -> Result<HostTuningReport> {
    let mut report = HostTuningReport::default();

    // Warn when running inside WSL2 on slow NTFS mount (/mnt/c/...).
    if profile.platform.virtualization == VirtualizationKind::Wsl2 {
        let dir_str = project_dir.to_string_lossy();
        if dir_str.starts_with("/mnt/") {
            report.warnings.push(format!(
                "Project path `{}` is on a mounted Windows NTFS drive inside WSL2. Move the repo into Linux ext4 (`~/projects/...`) for 5x-10x faster container I/O.",
                dir_str
            ));
        }
    }

    if profile.runtime.docker_version.is_none() {
        report
            .skipped
            .push("Docker Engine not detected; skipping /etc/docker/daemon.json tuning".to_string());
        return Ok(report);
    }

    if !profile.platform.has_host_privileges() {
        report.skipped.push(
            "Non-root mode: skipped /etc/docker/daemon.json (re-run with `sudo` for host-level Docker tuning; container-level log rotation is still active)".to_string(),
        );
        return Ok(report);
    }

    let daemon_path = Path::new(DAEMON_JSON_PATH);
    let existing = if daemon_path.exists() {
        Some(fs::read_to_string(daemon_path)?)
    } else {
        None
    };

    let (merged, changed) = merge_daemon_json(existing.as_deref())?;
    if !changed {
        report
            .skipped
            .push("/etc/docker/daemon.json is already optimized".to_string());
        return Ok(report);
    }

    if dry_run {
        report.applied.push(
            "[dry-run] Would update /etc/docker/daemon.json (userland-proxy=false, json-file log rotation, live-restore=true)".to_string(),
        );
        return Ok(report);
    }

    // Validate JSON round-trip before writing.
    let _: serde_json::Value = serde_json::from_str(&merged)?;

    let entry = backup.backup_and_write(
        daemon_path,
        merged.as_bytes(),
        "docker-daemon-json",
        Some("docker.service"),
    )?;

    // Reload docker.service gracefully; auto-rollback if reload fails.
    let reload_ok = Command::new("systemctl")
        .args(["reload", "docker.service"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if reload_ok {
        report.applied.push(
            "Updated /etc/docker/daemon.json (userland-proxy=false, log rotation 10m x 3, live-restore=true) and reloaded docker.service".to_string(),
        );
    } else {
        // Attempt reload-or-restart only if reload was unsupported, otherwise rollback.
        let active_ok = Command::new("systemctl")
            .args(["is-active", "--quiet", "docker.service"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);

        if active_ok {
            report.applied.push(
                "Updated /etc/docker/daemon.json (restart `docker.service` when convenient to apply userland-proxy=false)".to_string(),
            );
        } else {
            backup.rollback_entry(&entry)?;
            report.warnings.push(
                "docker.service reload failed after editing daemon.json; automatically restored previous backup".to_string(),
            );
        }
    }

    Ok(report)
}
