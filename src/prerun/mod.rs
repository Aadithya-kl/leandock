pub mod compose_parser;
pub mod db_tuner;
pub mod host_tuner;
pub mod minifier;
pub mod static_offload;

use std::path::{Path, PathBuf};

use crate::backup::atomic_write_bytes;
use crate::error::Result;
use crate::prerun::db_tuner::{render_override_yaml, OptimizationPlan};

/// Writes `.leandock/compose.override.yml` atomically next to the user's Compose file.
pub fn write_override_file(project_dir: &Path, plan: &OptimizationPlan) -> Result<PathBuf> {
    let leandock_dir = project_dir.join(".leandock");
    let override_path = leandock_dir.join("compose.override.yml");
    let yaml = render_override_yaml(plan)?;
    atomic_write_bytes(&override_path, yaml.as_bytes())?;
    Ok(override_path)
}
