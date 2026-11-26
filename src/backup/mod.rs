use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::error::{LeanDockError, Result};

/// Metadata for a single host configuration file backed up before modification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupEntry {
    pub original_path: PathBuf,
    pub backup_path: Option<PathBuf>,
    pub existed_before: bool,
    pub component: String,
    pub reload_service: Option<String>,
}

/// Snapshot manifest written atomically for `leandock revert`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    pub snapshot_id: String,
    pub created_at: String,
    pub entries: Vec<BackupEntry>,
}

/// Manages transactional host config backups and atomic file writes.
#[derive(Debug, Clone)]
pub struct BackupManager {
    base_dir: PathBuf,
}

impl BackupManager {
    /// Selects `/var/lib/leandock/backups` for Linux root, or `~/.leandock/backups` otherwise.
    pub fn default_for_host(is_root: bool) -> Self {
        let base_dir = if cfg!(target_os = "linux") && is_root {
            PathBuf::from("/var/lib/leandock/backups")
        } else {
            user_home_dir().join(".leandock").join("backups")
        };
        Self { base_dir }
    }

    /// Creates a backup manager anchored at a custom directory (used in tests).
    pub fn with_base_dir(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    pub fn base_dir(&self) -> &Path {
        &self.base_dir
    }

    /// Starts a new lazy timestamped snapshot session.
    /// Does NOT touch the filesystem until `backup_and_write` is actually invoked,
    /// guaranteeing true zero-touch behavior during `analyze` and `--dry-run`.
    pub fn start_session(&self) -> Result<BackupSession> {
        let snapshot_id = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        let snapshot_dir = self.base_dir.join(&snapshot_id);

        Ok(BackupSession {
            snapshot_dir,
            manifest: BackupManifest {
                snapshot_id,
                created_at: Utc::now().to_rfc3339(),
                entries: Vec::new(),
            },
        })
    }

    /// Lists available snapshot manifests sorted newest-first.
    pub fn list_manifests(&self) -> Result<Vec<(PathBuf, BackupManifest)>> {
        if !self.base_dir.exists() {
            return Ok(Vec::new());
        }

        let mut manifests = Vec::new();
        for entry in fs::read_dir(&self.base_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                let manifest_path = path.join("manifest.json");
                if manifest_path.is_file() {
                    let raw = fs::read_to_string(&manifest_path)?;
                    if let Ok(manifest) = serde_json::from_str::<BackupManifest>(&raw) {
                        manifests.push((path, manifest));
                    }
                }
            }
        }

        manifests.sort_by(|a, b| b.1.snapshot_id.cmp(&a.1.snapshot_id));
        Ok(manifests)
    }

    /// Restores the latest snapshot (or all snapshots when `all == true`),
    /// tears down live Linux zram/KSM state when applicable, and reloads affected services.
    pub fn revert(&self, all: bool) -> Result<Vec<String>> {
        let manifests = self.list_manifests()?;
        if manifests.is_empty() {
            return Ok(vec![
                "No host configuration backups found to revert.".to_string()
            ]);
        }

        let targets = if all {
            manifests
        } else {
            manifests.into_iter().take(1).collect()
        };

        let mut actions = Vec::new();
        let mut services_to_reload: Vec<String> = Vec::new();
        let mut reverted_components: Vec<String> = Vec::new();

        for (dir, manifest) in targets {
            for item in manifest.entries.iter().rev() {
                if item.existed_before {
                    if let Some(backup_file) = &item.backup_path {
                        if backup_file.exists() {
                            let bytes = fs::read(backup_file)?;
                            atomic_write_bytes(&item.original_path, &bytes)?;
                            actions.push(format!(
                                "Restored {} ({})",
                                item.original_path.display(),
                                item.component
                            ));
                        }
                    }
                } else if item.original_path.exists() {
                    fs::remove_file(&item.original_path)?;
                    actions.push(format!(
                        "Removed created file {} ({})",
                        item.original_path.display(),
                        item.component
                    ));
                }

                if !reverted_components.contains(&item.component) {
                    reverted_components.push(item.component.clone());
                }

                if let Some(svc) = &item.reload_service {
                    if !services_to_reload.contains(svc) {
                        services_to_reload.push(svc.clone());
                    }
                }
            }

            let _ = fs::remove_dir_all(&dir);
        }

        for comp in &reverted_components {
            if let Some(msg) = teardown_live_component_best_effort(comp) {
                actions.push(msg);
            }
        }

        for svc in services_to_reload {
            if let Some(msg) = reload_service_best_effort(&svc) {
                actions.push(msg);
            }
        }

        Ok(actions)
    }
}

/// Active transactional backup session for a single `leandock up` invocation.
pub struct BackupSession {
    snapshot_dir: PathBuf,
    manifest: BackupManifest,
}

impl BackupSession {
    /// Snapshots `target` before editing, then atomically writes `new_contents`.
    pub fn backup_and_write(
        &mut self,
        target: &Path,
        new_contents: &[u8],
        component: &str,
        reload_service: Option<&str>,
    ) -> Result<BackupEntry> {
        fs::create_dir_all(&self.snapshot_dir).map_err(|source| LeanDockError::IoPath {
            path: self.snapshot_dir.clone(),
            source,
        })?;

        let existed_before = target.exists();
        let backup_path = if existed_before {
            let safe_name = target
                .to_string_lossy()
                .replace(['/', '\\', ':'], "_")
                .trim_start_matches('_')
                .to_string();
            let dest = self.snapshot_dir.join(format!("{}.bak", safe_name));
            let original_bytes = fs::read(target).map_err(|source| LeanDockError::IoPath {
                path: target.to_path_buf(),
                source,
            })?;
            atomic_write_bytes(&dest, &original_bytes)?;
            Some(dest)
        } else {
            None
        };

        atomic_write_bytes(target, new_contents)?;

        let entry = BackupEntry {
            original_path: target.to_path_buf(),
            backup_path,
            existed_before,
            component: component.to_string(),
            reload_service: reload_service.map(|s| s.to_string()),
        };

        self.manifest.entries.push(entry.clone());
        self.save_manifest()?;
        Ok(entry)
    }

    /// Restores a single entry immediately (e.g., if `systemctl reload docker` fails).
    pub fn rollback_entry(&self, entry: &BackupEntry) -> Result<()> {
        if entry.existed_before {
            if let Some(bak) = &entry.backup_path {
                let bytes = fs::read(bak)?;
                atomic_write_bytes(&entry.original_path, &bytes)?;
            }
        } else if entry.original_path.exists() {
            fs::remove_file(&entry.original_path)?;
        }
        Ok(())
    }

    fn save_manifest(&self) -> Result<()> {
        let manifest_path = self.snapshot_dir.join("manifest.json");
        let payload = serde_json::to_vec_pretty(&self.manifest)?;
        atomic_write_bytes(&manifest_path, &payload)
    }
}

/// Writes `bytes` to a sibling temporary file in the same directory and atomically renames it.
pub fn atomic_write_bytes(target: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = target.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|source| LeanDockError::IoPath {
                path: parent.to_path_buf(),
                source,
            })?;
        }
    }

    let file_name = target
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("leandock.tmp");
    let pid = std::process::id();
    let nonce = Utc::now().timestamp_nanos_opt().unwrap_or(0);
    let tmp_path = target.with_file_name(format!(".{}.{}.{}.tmp", file_name, pid, nonce));

    fs::write(&tmp_path, bytes).map_err(|source| LeanDockError::IoPath {
        path: tmp_path.clone(),
        source,
    })?;

    if let Err(err) = fs::rename(&tmp_path, target) {
        // On Windows, fs::rename may fail if the destination already exists.
        #[cfg(target_os = "windows")]
        {
            if target.exists() {
                let _ = fs::remove_file(target);
                if fs::rename(&tmp_path, target).is_ok() {
                    return Ok(());
                }
            }
        }
        let _ = fs::remove_file(&tmp_path);
        return Err(LeanDockError::IoPath {
            path: target.to_path_buf(),
            source: err,
        });
    }

    Ok(())
}

/// Resolves the user's home directory across Linux, macOS, and Windows without external crates.
pub fn user_home_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Ok(profile) = std::env::var("USERPROFILE") {
            return PathBuf::from(profile);
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home);
    }
    PathBuf::from(".")
}

#[cfg(target_os = "linux")]
fn teardown_live_component_best_effort(component: &str) -> Option<String> {
    match component {
        "ksm-tmpfiles" => {
            if fs::write("/sys/kernel/mm/ksm/run", "0").is_ok() {
                return Some("Disabled live KSM (/sys/kernel/mm/ksm/run = 0)".to_string());
            }
            None
        }
        "zram-generator" => {
            let ok = std::process::Command::new("swapoff")
                .arg("/dev/zram0")
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if ok {
                let _ = fs::write("/sys/block/zram0/reset", "1");
                return Some("Deactivated live zram swap (/dev/zram0)".to_string());
            }
            None
        }
        "proxy-systemd-service" => {
            let _ = std::process::Command::new("systemctl")
                .args(["disable", "--now", "leandock-proxy.service"])
                .status();
            let _ = std::process::Command::new("systemctl")
                .args(["--user", "disable", "--now", "leandock-proxy.service"])
                .status();
            Some("Stopped and disabled leandock-proxy.service".to_string())
        }
        _ => None,
    }
}

#[cfg(not(target_os = "linux"))]
fn teardown_live_component_best_effort(_component: &str) -> Option<String> {
    None
}

#[cfg(target_os = "linux")]
fn reload_service_best_effort(service: &str) -> Option<String> {
    if service == "sysctl" {
        let ok = std::process::Command::new("sysctl")
            .arg("--system")
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        return Some(if ok {
            "Reloaded sysctl --system".to_string()
        } else {
            "Warning: could not reload sysctl --system automatically".to_string()
        });
    }

    let _ = std::process::Command::new("systemctl")
        .arg("daemon-reload")
        .status();

    let ok = std::process::Command::new("systemctl")
        .args(["reload-or-restart", service])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    Some(if ok {
        format!("Reloaded systemd service `{}`", service)
    } else {
        format!(
            "Note: service `{}` could not be reloaded automatically; restart manually if needed",
            service
        )
    })
}

#[cfg(not(target_os = "linux"))]
fn reload_service_best_effort(service: &str) -> Option<String> {
    Some(format!(
        "Note: restart `{}` manually to apply restored configuration",
        service
    ))
}
