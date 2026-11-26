use serde::{Deserialize, Serialize};
use sysinfo::{Disks, System};

/// Cross-platform hardware telemetry collected in Phase 0.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardwareInfo {
    pub cpu_arch: String,
    pub logical_cores: usize,
    pub total_ram_mb: u64,
    pub available_ram_mb: u64,
    pub total_swap_mb: u64,
    pub used_swap_mb: u64,
    pub free_disk_gb: f64,
    pub total_disk_gb: f64,
}

impl HardwareInfo {
    /// Collects hardware metrics across Linux, macOS, and Windows via `sysinfo`.
    pub fn detect() -> Self {
        let mut sys = System::new_all();
        sys.refresh_memory();
        sys.refresh_cpu_all();

        let bytes_to_mb = |b: u64| b / (1024 * 1024);
        let total_ram_mb = bytes_to_mb(sys.total_memory()).max(256);
        let available_ram_mb = bytes_to_mb(sys.available_memory());
        let total_swap_mb = bytes_to_mb(sys.total_swap());
        let used_swap_mb = bytes_to_mb(sys.used_swap());
        let logical_cores = sys.cpus().len().max(1);

        let (free_disk_gb, total_disk_gb) = detect_primary_disk_gb();

        Self {
            cpu_arch: std::env::consts::ARCH.to_string(),
            logical_cores,
            total_ram_mb,
            available_ram_mb,
            total_swap_mb,
            used_swap_mb,
            free_disk_gb,
            total_disk_gb,
        }
    }

    /// Returns true if the host is an ultra-low-RAM VPS (<= 1.5 GB RAM).
    pub fn is_ultra_low_ram(&self) -> bool {
        self.total_ram_mb <= 1536
    }

    /// Returns true if the host has <= 2 GB RAM (triggers extra conservative DB tweaks).
    pub fn is_low_ram(&self) -> bool {
        self.total_ram_mb <= 2048
    }
}

fn detect_primary_disk_gb() -> (f64, f64) {
    let disks = Disks::new_with_refreshed_list();
    let mut best_total: u64 = 0;
    let mut best_free: u64 = 0;

    for disk in disks.list() {
        let mount = disk.mount_point().to_string_lossy();
        let is_root = mount == "/" || mount.eq_ignore_ascii_case("C:\\");
        if is_root {
            let total = disk.total_space();
            let avail = disk.available_space();
            return (
                avail as f64 / 1_073_741_824.0,
                total as f64 / 1_073_741_824.0,
            );
        }
        if disk.total_space() > best_total {
            best_total = disk.total_space();
            best_free = disk.available_space();
        }
    }

    (
        best_free as f64 / 1_073_741_824.0,
        best_total as f64 / 1_073_741_824.0,
    )
}
