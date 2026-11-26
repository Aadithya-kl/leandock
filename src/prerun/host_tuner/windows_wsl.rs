#![cfg(target_os = "windows")]

use std::fs;
use std::path::Path;

use crate::backup::{user_home_dir, BackupSession};
use crate::error::Result;
use crate::prerun::host_tuner::{merge_wslconfig_ini, HostTuningReport};
use crate::profiler::HostProfile;

/// Windows Tier 3 optimizer: safely analyzes and tunes `%UserProfile%\.wslconfig`
/// (`memory`, `swap`, `autoMemoryReclaim=gradual`, `sparseVhd=true`) and warns on slow NTFS bind mounts.
pub fn tune_windows_wsl(
    profile: &HostProfile,
    project_dir: &Path,
    backup: &mut BackupSession,
    dry_run: bool,
) -> Result<HostTuningReport> {
    let mut report = HostTuningReport::default();

    // Warn when bind-mounting Windows NTFS paths into WSL2 containers.
    let dir_str = project_dir.to_string_lossy();
    if dir_str.contains(":\\") || dir_str.starts_with("\\\\") {
        report.warnings.push(format!(
            "Project `{}` resides on Windows NTFS. Running Docker Compose directly inside WSL2 ext4 (`\\\\wsl$\\...` or `~/projects`) dramatically reduces bind-mount I/O overhead.",
            dir_str
        ));
    }

    let wslconfig_path = user_home_dir().join(".wslconfig");
    let existing = if wslconfig_path.exists() {
        Some(fs::read_to_string(&wslconfig_path)?)
    } else {
        None
    };

    let (merged, changed) = merge_wslconfig_ini(existing.as_deref(), profile.hardware.total_ram_mb);
    if !changed {
        report
            .skipped
            .push(format!("{} is already tuned for WSL2 memory reclamation", wslconfig_path.display()));
        return Ok(report);
    }

    if dry_run {
        report.applied.push(format!(
            "[dry-run] Would safely update {} (memory cap, autoMemoryReclaim=gradual, sparseVhd=true)",
            wslconfig_path.display()
        ));
        return Ok(report);
    }

    backup.backup_and_write(
        &wslconfig_path,
        merged.as_bytes(),
        "windows-wslconfig",
        None,
    )?;

    report.applied.push(format!(
        "Updated {} with safe WSL2 RAM cap, autoMemoryReclaim=gradual, and sparseVhd=true (run `wsl --shutdown` once to activate)",
        wslconfig_path.display()
    ));

    Ok(report)
}
