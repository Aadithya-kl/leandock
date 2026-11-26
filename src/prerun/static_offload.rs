use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::error::{LeanDockError, Result};
use crate::prerun::compose_parser::{ParsedCompose, ParsedService};

/// Metadata for a single static frontend extracted from a container image.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OffloadedSite {
    pub service_name: String,
    pub output_dir: PathBuf,
    pub listen_port: u16,
    pub spa_fallback: bool,
}

/// Extracts static assets from services carrying `leandock.static_dir=<path>` into `.leandock/static/<service>/`.
/// Strictly opt-in (`--extract-static`), skips `leandock.ignore=true` services, and verifies extracted index files.
pub fn extract_static_frontends(
    compose: &ParsedCompose,
    runtime_bin: &str,
    leandock_dir: &Path,
) -> Result<Vec<OffloadedSite>> {
    let mut sites = Vec::new();
    let static_root = leandock_dir.join("static");

    for (name, svc) in &compose.services {
        if svc.is_ignored() {
            continue;
        }
        let Some(static_dir) = svc.label("leandock.static_dir") else {
            continue;
        };
        let Some(ref image) = svc.image else {
            tracing::warn!(
                service = %name,
                "Skipping static offload for `{}`: no `image:` defined (build the image first)",
                name
            );
            continue;
        };

        let dest_dir = static_root.join(name);
        if dest_dir.exists() {
            fs::remove_dir_all(&dest_dir)?;
        }
        fs::create_dir_all(&dest_dir)?;

        match extract_from_image(runtime_bin, image, static_dir, &dest_dir) {
            Ok(()) => {
                if !dir_has_files(&dest_dir) {
                    tracing::warn!(
                        service = %name,
                        "Static extraction produced an empty directory; falling back to container"
                    );
                    let _ = fs::remove_dir_all(&dest_dir);
                    continue;
                }
                let listen_port = parse_frontend_port(svc);
                let spa_fallback = svc.label_bool("leandock.spa").unwrap_or(false);
                sites.push(OffloadedSite {
                    service_name: name.clone(),
                    output_dir: dest_dir,
                    listen_port,
                    spa_fallback,
                });
            }
            Err(err) => {
                tracing::warn!(
                    service = %name,
                    error = %err,
                    "Static extraction failed for `{}`; keeping original container safely",
                    name
                );
                let _ = fs::remove_dir_all(&dest_dir);
            }
        }
    }

    if !sites.is_empty() {
        let manifest_path = static_root.join("sites.json");
        let payload = serde_json::to_vec_pretty(&sites)?;
        crate::backup::atomic_write_bytes(&manifest_path, &payload)?;
    }

    Ok(sites)
}

fn extract_from_image(
    runtime_bin: &str,
    image: &str,
    container_dir: &str,
    dest_dir: &Path,
) -> Result<()> {
    let create_out = Command::new(runtime_bin)
        .args(["create", image])
        .output()
        .map_err(|e| LeanDockError::CommandFailed {
            cmd: format!("{} create {}", runtime_bin, image),
            code: None,
            stderr: e.to_string(),
        })?;

    if !create_out.status.success() {
        return Err(LeanDockError::CommandFailed {
            cmd: format!("{} create {}", runtime_bin, image),
            code: create_out.status.code(),
            stderr: String::from_utf8_lossy(&create_out.stderr)
                .trim()
                .to_string(),
        });
    }

    let container_id = String::from_utf8_lossy(&create_out.stdout)
        .trim()
        .to_string();

    let src = format!("{}:{}/.", container_id, container_dir.trim_end_matches('/'));
    let cp_out = Command::new(runtime_bin)
        .args(["cp", &src, &dest_dir.to_string_lossy()])
        .output();

    // Always clean up temporary container regardless of cp result.
    let _ = Command::new(runtime_bin)
        .args(["rm", "-f", &container_id])
        .output();

    let cp_out = cp_out?;
    if !cp_out.status.success() {
        return Err(LeanDockError::CommandFailed {
            cmd: format!("{} cp {}", runtime_bin, src),
            code: cp_out.status.code(),
            stderr: String::from_utf8_lossy(&cp_out.stderr).trim().to_string(),
        });
    }

    Ok(())
}

fn dir_has_files(dir: &Path) -> bool {
    fs::read_dir(dir)
        .ok()
        .and_then(|mut it| it.next())
        .is_some()
}

fn parse_frontend_port(svc: &ParsedService) -> u16 {
    if let Some(port) = svc
        .label("leandock.static_port")
        .and_then(|p| p.parse::<u16>().ok())
    {
        return port;
    }
    for port_spec in &svc.ports {
        if let Some(host_part) = port_spec.split(':').next() {
            if let Ok(p) = host_part.trim().parse::<u16>() {
                return p;
            }
        }
    }
    8080
}
