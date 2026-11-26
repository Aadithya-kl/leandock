pub mod cleaner;
pub mod health_guard;
#[cfg(target_os = "linux")]
pub mod kernel_opt;

use serde::{Deserialize, Serialize};

use crate::backup::BackupSession;
use crate::error::Result;
use crate::profiler::HostProfile;

/// Summary of Phase 2 kernel memory optimizations (`zram`, `KSM`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KernelOptReport {
    pub applied: Vec<String>,
    pub skipped: Vec<String>,
    pub warnings: Vec<String>,
}

/// Dispatches Tier 4 Linux kernel optimizations (`zram` + `KSM`) with clean cross-platform fallback.
pub fn apply_kernel_optimizations(
    profile: &HostProfile,
    backup: &mut BackupSession,
    dry_run: bool,
) -> Result<KernelOptReport> {
    #[cfg(target_os = "linux")]
    {
        kernel_opt::apply_linux_kernel_optimizations(profile, backup, dry_run)
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = (backup, dry_run);
        Ok(KernelOptReport {
            applied: Vec::new(),
            skipped: vec![profile.platform.capabilities.tier4_reason.clone()],
            warnings: Vec::new(),
        })
    }
}

/// Live zram & KSM kernel memory telemetry (returns `None` on non-Linux or when inactive).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KernelMemoryStats {
    pub zram_active: bool,
    pub zram_orig_data_mb: f64,
    pub zram_compr_data_mb: f64,
    pub zram_ratio: f64,
    pub ksm_active: bool,
    pub ksm_pages_sharing: u64,
    pub ksm_saved_mb: f64,
}

/// Collects live kernel memory statistics (zram compression ratio + KSM pages saved).
pub fn read_kernel_memory_stats() -> KernelMemoryStats {
    #[cfg(target_os = "linux")]
    {
        kernel_opt::read_linux_kernel_stats()
    }
    #[cfg(not(target_os = "linux"))]
    {
        KernelMemoryStats::default()
    }
}
