pub mod hardware;
pub mod platform;
pub mod runtime;

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{LeanDockError, Result};
pub use hardware::HardwareInfo;
pub use platform::{
    CapabilityMatrix, CapabilitySupport, HostOs, PlatformProfile, VirtualizationKind,
};
pub use runtime::{ComposeDriver, ContainerRuntimeKind, RuntimeInfo};

/// Complete pre-flight snapshot of the host machine, platform capabilities, and runtimes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostProfile {
    pub hardware: HardwareInfo,
    pub platform: PlatformProfile,
    pub runtime: RuntimeInfo,
}

impl HostProfile {
    /// Probes the current machine safely without mutating any state.
    pub fn detect() -> Self {
        let hardware = HardwareInfo::detect();
        let platform = PlatformProfile::detect();
        let runtime = RuntimeInfo::detect(&platform);
        Self {
            hardware,
            platform,
            runtime,
        }
    }

    /// Computes effective RAM budget in MB for container budgeting.
    /// On Windows/macOS where containers run inside a Linux VM, budgets conservatively
    /// against the container VM allocation when detectable or a safe fraction of host RAM.
    pub fn effective_ram_mb(&self) -> u64 {
        match self.platform.os {
            HostOs::Linux => self.hardware.total_ram_mb,
            HostOs::Windows | HostOs::MacOs => {
                if let Some(vm_mb) = self.runtime.vm_memory_limit_mb {
                    vm_mb
                } else {
                    // Conservative default when container VM limit is unknown (~50% of host RAM, capped at 8GB)
                    (self.hardware.total_ram_mb / 2).clamp(1024, 8192)
                }
            }
            HostOs::Other => self.hardware.total_ram_mb,
        }
    }
}

/// Auto-discovers a Docker Compose file in `dir` if no explicit `-f` path was supplied.
pub fn discover_compose_file(explicit: Option<&Path>, dir: &Path) -> Result<PathBuf> {
    if let Some(path) = explicit {
        if path.exists() {
            return Ok(path.to_path_buf());
        }
        return Err(LeanDockError::ComposeFileNotFound(path.to_path_buf()));
    }

    const CANDIDATES: [&str; 4] = [
        "compose.yaml",
        "compose.yml",
        "docker-compose.yaml",
        "docker-compose.yml",
    ];

    for name in CANDIDATES {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    Err(LeanDockError::ComposeFileNotFound(dir.to_path_buf()))
}
