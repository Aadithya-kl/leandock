use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::error::Result;

/// Summary of safe post-deploy disk cleanup (dangling images & unused build cache only).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CleanupReport {
    pub reclaimed_images_mb: f64,
    pub reclaimed_build_cache_mb: f64,
    pub total_reclaimed_mb: f64,
    pub notes: Vec<String>,
}

/// Prunes ONLY dangling intermediate images and unused build cache.
/// Strictly protects ALL named volumes, bind mounts, and tagged/active images.
pub fn run_safe_cleanup(runtime_bin: &str, dry_run: bool) -> Result<CleanupReport> {
    let mut report = CleanupReport::default();

    if dry_run {
        report.notes.push(format!(
            "[dry-run] Would run `{} image prune -f` and `{} builder prune -f` (volumes and active images untouched)",
            runtime_bin, runtime_bin
        ));
        return Ok(report);
    }

    // 1. Prune strictly dangling (<none>) intermediate layers (`image prune -f`, NEVER `-a`).
    if let Ok(out) = Command::new(runtime_bin)
        .args(["image", "prune", "-f"])
        .output()
    {
        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let mb = parse_reclaimed_mb(&stdout);
            report.reclaimed_images_mb = mb;
            report
                .notes
                .push(format!("Pruned dangling images: {:.1} MB reclaimed", mb));
        }
    }

    // 2. Prune unused BuildKit cache (docker only; podman builder prune may not exist on older versions).
    if let Ok(out) = Command::new(runtime_bin)
        .args(["builder", "prune", "-f"])
        .output()
    {
        if out.status.success() {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let mb = parse_reclaimed_mb(&stdout);
            report.reclaimed_build_cache_mb = mb;
            report
                .notes
                .push(format!("Pruned unused build cache: {:.1} MB reclaimed", mb));
        }
    }

    report.total_reclaimed_mb = report.reclaimed_images_mb + report.reclaimed_build_cache_mb;
    Ok(report)
}

/// Parses Docker/Podman output lines such as `"Total reclaimed space: 1.42GB"` or `"Total: 342.5MB"` into MB.
pub fn parse_reclaimed_mb(output: &str) -> f64 {
    for line in output.lines() {
        let lower = line.to_lowercase();
        if lower.contains("reclaimed") || lower.starts_with("total:") {
            if let Some(val_str) = line.split(':').nth(1) {
                return parse_size_to_mb(val_str.trim());
            }
        }
    }
    0.0
}

pub fn parse_size_to_mb(raw: &str) -> f64 {
    let trimmed = raw.trim();
    let idx = trimmed
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(trimmed.len());
    let num: f64 = trimmed[..idx].trim().parse().unwrap_or(0.0);
    let unit = trimmed[idx..].trim().to_uppercase();

    match unit.as_str() {
        "B" => num / (1024.0 * 1024.0),
        "KB" | "KIB" => num / 1024.0,
        "MB" | "MIB" => num,
        "GB" | "GIB" => num * 1024.0,
        "TB" | "TIB" => num * 1024.0 * 1024.0,
        _ => 0.0,
    }
}
