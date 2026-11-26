#![cfg(target_os = "linux")]

use std::fs;
use std::path::Path;
use std::process::Command;

use crate::backup::BackupSession;
use crate::error::Result;
use crate::postrun::{KernelMemoryStats, KernelOptReport};
use crate::profiler::HostProfile;

const SYSCTL_PATH: &str = "/etc/sysctl.d/99-leandock.conf";
const ZRAM_GENERATOR_PATH: &str = "/etc/systemd/zram-generator.conf.d/99-leandock.conf";
const KSM_TMPFILES_PATH: &str = "/etc/tmpfiles.d/99-leandock-ksm.conf";
const DOCKER_KSM_DROPIN_PATH: &str = "/etc/systemd/system/docker.service.d/99-leandock-ksm.conf";
const CONTAINERD_KSM_DROPIN_PATH: &str =
    "/etc/systemd/system/containerd.service.d/99-leandock-ksm.conf";

/// Applies real Linux kernel memory optimizations (`zram` zstd + `KSM` with `MemoryKSM=yes`)
/// strictly gated by hardware, virtualization, kernel >= 6.1, systemd >= 254, and root privileges.
pub fn apply_linux_kernel_optimizations(
    profile: &HostProfile,
    backup: &mut BackupSession,
    dry_run: bool,
) -> Result<KernelOptReport> {
    let mut report = KernelOptReport::default();
    let caps = &profile.platform.capabilities;

    if !caps.zram_supported {
        report.skipped.push(caps.tier4_reason.clone());
        return Ok(report);
    }

    configure_zram(profile, backup, dry_run, &mut report)?;
    configure_ksm(profile, backup, dry_run, &mut report)?;

    Ok(report)
}

fn configure_zram(
    profile: &HostProfile,
    backup: &mut BackupSession,
    dry_run: bool,
    report: &mut KernelOptReport,
) -> Result<()> {
    // Skip cleanly if any zram swap device is already active.
    if is_zram_already_active() {
        report.skipped.push(
            "zram: already active on host (/dev/zram*); preserving existing setup".to_string(),
        );
        return Ok(());
    }

    // Target zram size: 100% of RAM on <= 2GB VPS, 50% on larger VPS, capped at 8192 MB.
    let zram_mb = if profile.hardware.total_ram_mb <= 2048 {
        profile.hardware.total_ram_mb
    } else {
        (profile.hardware.total_ram_mb / 2).min(8192)
    }
    .max(512);

    let sysctl_content = "\
# Managed by LeanDock — optimal zram virtual memory parameters
vm.swappiness = 120
vm.page-cluster = 0
vm.watermark_boost_factor = 0
vm.watermark_scale_factor = 125
";

    let zram_gen_content = format!(
        "# Managed by LeanDock — persistent zram swap (zstd)\n\
[zram0]\n\
zram-size = {}\n\
compression-algorithm = zstd\n\
swap-priority = 100\n",
        zram_mb
    );

    if dry_run {
        report.applied.push(format!(
            "[dry-run] Would configure {} MB zram (zstd) + {}",
            zram_mb, SYSCTL_PATH
        ));
        return Ok(());
    }

    // Load zram kernel module safely; skip gracefully if kernel lacks CONFIG_ZRAM.
    let modprobe_ok = Command::new("modprobe")
        .arg("zram")
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    if !modprobe_ok && !Path::new("/sys/block/zram0").exists() {
        report.skipped.push(
            "zram: kernel module `zram` could not be loaded (`modprobe zram` failed); skipped cleanly".to_string(),
        );
        return Ok(());
    }

    // Persist sysctl tuning + zram-generator config via transactional backup.
    backup.backup_and_write(
        Path::new(SYSCTL_PATH),
        sysctl_content.as_bytes(),
        "sysctl-zram",
        Some("sysctl"),
    )?;

    backup.backup_and_write(
        Path::new(ZRAM_GENERATOR_PATH),
        zram_gen_content.as_bytes(),
        "zram-generator",
        None,
    )?;

    // Activate zram0 immediately via sysfs + mkswap/swapon if /sys/block/zram0 is ready.
    if let Err(err) = activate_zram_live(zram_mb) {
        report.warnings.push(format!(
            "zram: persisted config for next boot, live activation skipped ({})",
            err
        ));
    } else {
        let _ = Command::new("sysctl").args(["-p", SYSCTL_PATH]).status();
        report.applied.push(format!(
            "zram: activated {} MB zstd compressed swap + tuned vm.swappiness=120",
            zram_mb
        ));
    }

    Ok(())
}

fn activate_zram_live(zram_mb: u64) -> std::io::Result<()> {
    let zram_sys = Path::new("/sys/block/zram0");
    if !zram_sys.exists() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "/sys/block/zram0 unavailable",
        ));
    }

    let _ = fs::write(zram_sys.join("reset"), "1");
    let _ = fs::write(zram_sys.join("comp_algorithm"), "zstd");
    let disksize_bytes = zram_mb * 1024 * 1024;
    fs::write(zram_sys.join("disksize"), disksize_bytes.to_string())?;

    let mkswap = Command::new("mkswap").arg("/dev/zram0").status()?;
    if !mkswap.success() {
        return Err(std::io::Error::other("mkswap /dev/zram0 failed"));
    }

    let swapon = Command::new("swapon")
        .args(["-p", "100", "/dev/zram0"])
        .status()?;
    if !swapon.success() {
        return Err(std::io::Error::other("swapon /dev/zram0 failed"));
    }
    Ok(())
}

fn configure_ksm(
    profile: &HostProfile,
    backup: &mut BackupSession,
    dry_run: bool,
    report: &mut KernelOptReport,
) -> Result<()> {
    if !profile.platform.capabilities.ksm_supported {
        report.skipped.push(
            "KSM: skipped cleanly (requires Linux kernel >= 6.1, systemd >= 254, root, and writable /sys/kernel/mm/ksm/run)".to_string(),
        );
        return Ok(());
    }

    let tmpfiles_content = "\
# Managed by LeanDock — conservative KSM scan parameters
w /sys/kernel/mm/ksm/pages_to_scan - - - - 200
w /sys/kernel/mm/ksm/sleep_millisecs - - - - 200
w /sys/kernel/mm/ksm/run - - - - 1
";

    let dropin_content = "\
# Managed by LeanDock — enables PR_SET_MEMORY_MERGE for containers on kernel >= 6.1
[Service]
MemoryKSM=yes
";

    if dry_run {
        report.applied.push(
            "[dry-run] Would enable KSM (pages_to_scan=200, sleep_millisecs=200) and install MemoryKSM=yes systemd drop-in".to_string(),
        );
        return Ok(());
    }

    backup.backup_and_write(
        Path::new(KSM_TMPFILES_PATH),
        tmpfiles_content.as_bytes(),
        "ksm-tmpfiles",
        None,
    )?;

    if Path::new("/etc/systemd/system").exists() {
        backup.backup_and_write(
            Path::new(DOCKER_KSM_DROPIN_PATH),
            dropin_content.as_bytes(),
            "ksm-docker-dropin",
            Some("docker.service"),
        )?;
        backup.backup_and_write(
            Path::new(CONTAINERD_KSM_DROPIN_PATH),
            dropin_content.as_bytes(),
            "ksm-containerd-dropin",
            Some("containerd.service"),
        )?;
        let _ = Command::new("systemctl").arg("daemon-reload").status();
    }

    // Apply live sysfs KSM knobs conservatively.
    let _ = fs::write("/sys/kernel/mm/ksm/pages_to_scan", "200");
    let _ = fs::write("/sys/kernel/mm/ksm/sleep_millisecs", "200");
    let _ = fs::write("/sys/kernel/mm/ksm/run", "1");

    report.applied.push(
        "KSM: enabled kernel page deduplication (sleep_millisecs=200, pages_to_scan=200) + installed MemoryKSM=yes systemd drop-ins".to_string(),
    );

    Ok(())
}

fn is_zram_already_active() -> bool {
    fs::read_to_string("/proc/swaps")
        .map(|s| s.contains("/dev/zram"))
        .unwrap_or(false)
}

/// Reads live zram compression statistics (`/sys/block/zram0/mm_stat`) and KSM metrics.
pub fn read_linux_kernel_stats() -> KernelMemoryStats {
    let mut stats = KernelMemoryStats::default();

    if let Ok(mm_stat) = fs::read_to_string("/sys/block/zram0/mm_stat") {
        let fields: Vec<u64> = mm_stat
            .split_whitespace()
            .filter_map(|t| t.parse::<u64>().ok())
            .collect();
        if fields.len() >= 2 && fields[0] > 0 {
            stats.zram_active = true;
            stats.zram_orig_data_mb = fields[0] as f64 / (1024.0 * 1024.0);
            stats.zram_compr_data_mb = fields[1] as f64 / (1024.0 * 1024.0);
            stats.zram_ratio = if fields[1] > 0 {
                fields[0] as f64 / fields[1] as f64
            } else {
                0.0
            };
        }
    }

    let ksm_run = fs::read_to_string("/sys/kernel/mm/ksm/run")
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .unwrap_or(0);
    if ksm_run == 1 {
        stats.ksm_active = true;
        let pages_sharing = fs::read_to_string("/sys/kernel/mm/ksm/pages_sharing")
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(0);
        stats.ksm_pages_sharing = pages_sharing;
        // Standard 4 KiB page size calculation.
        stats.ksm_saved_mb = (pages_sharing as f64 * 4096.0) / (1024.0 * 1024.0);
    }

    stats
}
