use std::collections::BTreeMap;
use std::process::Command;

use crate::error::{LeanDockError, Result};
use crate::prerun::compose_parser::{ParsedCompose, ServiceRole};

/// Outcome of attempting opt-in SlimToolkit minification for a single service.
#[derive(Debug, Clone)]
pub struct MinifyOutcome {
    pub service: String,
    pub original_image: String,
    pub minified_image: Option<String>,
    pub message: String,
}

/// Runs opt-in image minification via SlimToolkit (`slim`) ONLY on non-database, non-ignored
/// application images. Each minified image is startup-verified in an isolated container; any
/// verification failure automatically falls back to the original untouched image.
pub fn minify_eligible_services(
    compose: &ParsedCompose,
    runtime_bin: &str,
    has_slim: bool,
) -> Result<(BTreeMap<String, String>, Vec<MinifyOutcome>)> {
    if !has_slim {
        return Err(LeanDockError::MissingExternalTool {
            tool: "slim".to_string(),
            hint: "Install SlimToolkit (https://github.com/slimtoolkit/slim) or omit `--minify`."
                .to_string(),
        });
    }

    let mut minified_map = BTreeMap::new();
    let mut outcomes = Vec::new();

    for (name, svc) in &compose.services {
        if svc.is_ignored() {
            continue;
        }
        // Never minify databases or caches (high risk of stripping locale/storage binaries).
        if matches!(svc.role, ServiceRole::Database | ServiceRole::Cache) {
            continue;
        }
        let Some(ref image) = svc.image else {
            continue;
        };

        let target_tag = format!("{}.leandock-slim", image);
        let build_out = Command::new("slim")
            .args([
                "build",
                "--target",
                image,
                "--tag",
                &target_tag,
                "--http-probe=false",
                "--continue-after",
                "10",
            ])
            .output()?;

        if !build_out.status.success() {
            outcomes.push(MinifyOutcome {
                service: name.clone(),
                original_image: image.clone(),
                minified_image: None,
                message: format!(
                    "SlimToolkit failed for `{}`; reverted safely to `{}`",
                    name, image
                ),
            });
            continue;
        }

        // Verify the minified image boots cleanly without immediate crash.
        if verify_minified_image(runtime_bin, &target_tag) {
            minified_map.insert(name.clone(), target_tag.clone());
            outcomes.push(MinifyOutcome {
                service: name.clone(),
                original_image: image.clone(),
                minified_image: Some(target_tag),
                message: format!("Minified and startup-verified `{}`", name),
            });
        } else {
            let _ = Command::new(runtime_bin)
                .args(["rmi", "-f", &target_tag])
                .output();
            outcomes.push(MinifyOutcome {
                service: name.clone(),
                original_image: image.clone(),
                minified_image: None,
                message: format!(
                    "Minified image for `{}` failed startup verification; rolled back to `{}`",
                    name, image
                ),
            });
        }
    }

    Ok((minified_map, outcomes))
}

fn verify_minified_image(runtime_bin: &str, image_tag: &str) -> bool {
    let run_out = Command::new(runtime_bin)
        .args(["run", "-d", image_tag])
        .output();

    let Ok(out) = run_out else {
        return false;
    };
    if !out.status.success() {
        return false;
    }

    let cid = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if cid.is_empty() {
        return false;
    }

    std::thread::sleep(std::time::Duration::from_secs(3));

    let running = Command::new(runtime_bin)
        .args(["inspect", "-f", "{{.State.Running}}", &cid])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "true")
        .unwrap_or(false);

    let _ = Command::new(runtime_bin).args(["rm", "-f", &cid]).output();
    running
}
