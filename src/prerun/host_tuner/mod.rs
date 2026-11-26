#[cfg(target_os = "linux")]
pub mod linux_docker;
#[cfg(target_os = "macos")]
pub mod macos_runtime;
#[cfg(target_os = "linux")]
pub mod podman_quadlet;
#[cfg(target_os = "windows")]
pub mod windows_wsl;

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::backup::BackupSession;
use crate::error::Result;
use crate::prerun::compose_parser::ParsedCompose;
use crate::profiler::HostProfile;

/// Result of running host/platform runtime tuning (Tier 3).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HostTuningReport {
    pub applied: Vec<String>,
    pub skipped: Vec<String>,
    pub warnings: Vec<String>,
}

/// Runs platform-appropriate host runtime tuning safely.
/// - Linux (root): deep-merges `/etc/docker/daemon.json` after backup + validates reload.
/// - Linux (non-root): skips cleanly with clear notice.
/// - Windows: inspects & safely optimizes `%UserProfile%\.wslconfig` + warns about NTFS I/O.
/// - macOS: detects Docker Desktop / OrbStack / Colima & emits VM RAM guidance.
pub fn apply_host_tuning(
    profile: &HostProfile,
    project_dir: &Path,
    backup: &mut BackupSession,
    dry_run: bool,
) -> Result<HostTuningReport> {
    #[cfg(target_os = "linux")]
    {
        linux_docker::tune_linux_docker(profile, project_dir, backup, dry_run)
    }

    #[cfg(target_os = "windows")]
    {
        windows_wsl::tune_windows_wsl(profile, project_dir, backup, dry_run)
    }

    #[cfg(target_os = "macos")]
    {
        let _ = (project_dir, backup, dry_run);
        Ok(macos_runtime::inspect_macos_runtime(profile))
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        let _ = (profile, project_dir, backup, dry_run);
        Ok(HostTuningReport {
            applied: Vec::new(),
            skipped: vec!["Host tuning is not applicable on this OS".to_string()],
            warnings: Vec::new(),
        })
    }
}

/// Cross-platform pure deep-merge for Docker `daemon.json` (preserves ALL existing keys).
pub fn merge_daemon_json(existing_raw: Option<&str>) -> Result<(String, bool)> {
    let mut root: Map<String, Value> = match existing_raw {
        Some(raw) if !raw.trim().is_empty() => {
            let parsed: Value = serde_json::from_str(raw)?;
            parsed.as_object().cloned().unwrap_or_default()
        }
        _ => Map::new(),
    };

    let mut changed = false;

    if root.get("userland-proxy") != Some(&Value::Bool(false)) {
        root.insert("userland-proxy".to_string(), Value::Bool(false));
        changed = true;
    }

    if root.get("live-restore") != Some(&Value::Bool(true)) {
        root.insert("live-restore".to_string(), Value::Bool(true));
        changed = true;
    }

    if !root.contains_key("log-driver") {
        root.insert(
            "log-driver".to_string(),
            Value::String("json-file".to_string()),
        );
        changed = true;
    }

    let log_opts = root
        .entry("log-opts".to_string())
        .or_insert_with(|| json!({}));
    if let Some(opts_obj) = log_opts.as_object_mut() {
        if !opts_obj.contains_key("max-size") {
            opts_obj.insert("max-size".to_string(), Value::String("10m".to_string()));
            changed = true;
        }
        if !opts_obj.contains_key("max-file") {
            opts_obj.insert("max-file".to_string(), Value::String("3".to_string()));
            changed = true;
        }
    }

    let rendered = serde_json::to_string_pretty(&Value::Object(root))?;
    Ok((format!("{}\n", rendered), changed))
}

/// Cross-platform pure INI merger for Windows `%UserProfile%\.wslconfig`.
pub fn merge_wslconfig_ini(existing_raw: Option<&str>, total_ram_mb: u64) -> (String, bool) {
    let recommended_gb = (((total_ram_mb as f64 * 0.38) / 1024.0).round() as u64).clamp(2, 16);

    let desired_wsl2 = [
        ("memory", format!("{}GB", recommended_gb)),
        ("swap", "2GB".to_string()),
        ("autoMemoryReclaim", "gradual".to_string()),
    ];
    let desired_experimental = [
        ("autoMemoryReclaim", "gradual".to_string()),
        ("sparseVhd", "true".to_string()),
    ];

    let mut sections: Vec<(String, Vec<(String, String)>)> = Vec::new();
    let mut current_section = String::new();

    if let Some(raw) = existing_raw {
        for line in raw.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
                continue;
            }
            if trimmed.starts_with('[') && trimmed.ends_with(']') {
                current_section = trimmed[1..trimmed.len() - 1].trim().to_string();
                if !sections.iter().any(|(s, _)| s.eq_ignore_ascii_case(&current_section)) {
                    sections.push((current_section.clone(), Vec::new()));
                }
            } else if let Some((k, v)) = trimmed.split_once('=') {
                if current_section.is_empty() {
                    current_section = "wsl2".to_string();
                    sections.push((current_section.clone(), Vec::new()));
                }
                if let Some((_, pairs)) = sections
                    .iter_mut()
                    .find(|(s, _)| s.eq_ignore_ascii_case(&current_section))
                {
                    pairs.push((k.trim().to_string(), v.trim().to_string()));
                }
            }
        }
    }

    let mut changed = false;

    for (sec_name, defaults) in [
        ("wsl2", desired_wsl2.as_slice()),
        ("experimental", desired_experimental.as_slice()),
    ] {
        if !sections.iter().any(|(s, _)| s.eq_ignore_ascii_case(sec_name)) {
            sections.push((sec_name.to_string(), Vec::new()));
        }
        if let Some((_, pairs)) = sections
            .iter_mut()
            .find(|(s, _)| s.eq_ignore_ascii_case(sec_name))
        {
            for (k, v) in defaults {
                if !pairs.iter().any(|(ek, _)| ek.eq_ignore_ascii_case(k)) {
                    pairs.push(((*k).to_string(), v.clone()));
                    changed = true;
                }
            }
        }
    }

    let mut out = String::from("# Managed safely by LeanDock — backup stored before edits\n");
    for (sec_name, pairs) in sections {
        out.push_str(&format!("[{}]\n", sec_name));
        for (k, v) in pairs {
            out.push_str(&format!("{}={}\n", k, v));
        }
        out.push('\n');
    }

    (out, changed)
}

/// Scans a Compose project for Docker-specific features before running under Podman/Quadlet.
pub fn scan_podman_compatibility(compose: &ParsedCompose) -> Vec<String> {
    let mut warnings = Vec::new();
    for (name, svc) in &compose.services {
        if svc.mounts_docker_sock {
            warnings.push(format!(
                "Service `{}` mounts `/var/run/docker.sock` — enable `podman.socket` and update socket path for rootless Podman.",
                name
            ));
        }
        if svc.has_swarm_keys {
            warnings.push(format!(
                "Service `{}` uses Docker Swarm `deploy` keys (placement/update_config/mode) which Podman ignores.",
                name
            ));
        }
    }
    warnings
}
