use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HostOs {
    Linux,
    Windows,
    MacOs,
    Other,
}

impl std::fmt::Display for HostOs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostOs::Linux => write!(f, "Linux"),
            HostOs::Windows => write!(f, "Windows"),
            HostOs::MacOs => write!(f, "macOS"),
            HostOs::Other => write!(f, "Other"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VirtualizationKind {
    BareMetal,
    Kvm,
    Vmware,
    Xen,
    HyperV,
    Lxc,
    OpenVz,
    Wsl2,
    MacVm,
    Unknown,
}

impl std::fmt::Display for VirtualizationKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VirtualizationKind::BareMetal => write!(f, "bare-metal"),
            VirtualizationKind::Kvm => write!(f, "kvm"),
            VirtualizationKind::Vmware => write!(f, "vmware"),
            VirtualizationKind::Xen => write!(f, "xen"),
            VirtualizationKind::HyperV => write!(f, "hyper-v"),
            VirtualizationKind::Lxc => write!(f, "lxc"),
            VirtualizationKind::OpenVz => write!(f, "openvz"),
            VirtualizationKind::Wsl2 => write!(f, "wsl2"),
            VirtualizationKind::MacVm => write!(f, "macos-vm"),
            VirtualizationKind::Unknown => write!(f, "unknown"),
        }
    }
}

impl VirtualizationKind {
    /// Returns true if the kernel is shared/restricted and cannot load zram or tune KSM.
    pub fn is_restricted_container_vps(self) -> bool {
        matches!(
            self,
            VirtualizationKind::Lxc | VirtualizationKind::OpenVz | VirtualizationKind::Wsl2
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CapabilitySupport {
    Full,
    Partial,
    Skipped,
}

impl std::fmt::Display for CapabilitySupport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CapabilitySupport::Full => write!(f, "Full"),
            CapabilitySupport::Partial => write!(f, "Partial"),
            CapabilitySupport::Skipped => write!(f, "Auto-Skipped"),
        }
    }
}

/// Tiered Capability Matrix computed for the detected environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityMatrix {
    /// Tier 1: Compose & DB auto-tuner, container log rotation, post-deploy cleanup.
    pub tier1_compose_and_db: CapabilitySupport,
    /// Tier 2: Wake-on-Request reverse proxy & opt-in static offload.
    pub tier2_wake_proxy: CapabilitySupport,
    /// Tier 3: Host runtime tuning (Linux daemon.json, Windows .wslconfig, macOS VM advisor).
    pub tier3_host_tuning: CapabilitySupport,
    pub tier3_reason: String,
    /// Tier 4: Linux kernel optimizations (zram zstd, KSM >= 6.1 + systemd >= 254, Podman Quadlet).
    pub tier4_kernel_opt: CapabilitySupport,
    pub tier4_reason: String,
    pub zram_supported: bool,
    pub ksm_supported: bool,
    pub quadlet_supported: bool,
}

/// Complete OS, kernel, privilege, and virtualization profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformProfile {
    pub os: HostOs,
    pub distro: String,
    pub kernel_version: Option<String>,
    pub kernel_major_minor: Option<(u32, u32)>,
    pub systemd_version: Option<u32>,
    pub virtualization: VirtualizationKind,
    pub is_root: bool,
    pub can_sudo_noninteractive: bool,
    pub ksm_sysfs_writable: bool,
    pub capabilities: CapabilityMatrix,
}

impl PlatformProfile {
    /// Probes the current OS and computes its Tiered Capability Matrix.
    pub fn detect() -> Self {
        let os = detect_os();
        let is_root = detect_is_root();
        let can_sudo_noninteractive = detect_can_sudo();
        let (
            distro,
            kernel_version,
            kernel_major_minor,
            systemd_version,
            virtualization,
            ksm_sysfs_writable,
        ) = detect_platform_details(os);

        Self::from_facts(PlatformFacts {
            os,
            distro,
            kernel_version,
            kernel_major_minor,
            systemd_version,
            virtualization,
            is_root,
            can_sudo_noninteractive,
            ksm_sysfs_writable,
        })
    }

    /// Pure deterministic constructor used both by runtime detection and cross-platform tests.
    pub fn from_facts(facts: PlatformFacts) -> Self {
        let capabilities = compute_capabilities(&facts);
        Self {
            os: facts.os,
            distro: facts.distro,
            kernel_version: facts.kernel_version,
            kernel_major_minor: facts.kernel_major_minor,
            systemd_version: facts.systemd_version,
            virtualization: facts.virtualization,
            is_root: facts.is_root,
            can_sudo_noninteractive: facts.can_sudo_noninteractive,
            ksm_sysfs_writable: facts.ksm_sysfs_writable,
            capabilities,
        }
    }

    /// True if the process can perform privileged host/kernel edits on Linux.
    pub fn has_host_privileges(&self) -> bool {
        self.is_root
    }
}

/// Input facts for computing a `PlatformProfile` deterministically.
#[derive(Debug, Clone)]
pub struct PlatformFacts {
    pub os: HostOs,
    pub distro: String,
    pub kernel_version: Option<String>,
    pub kernel_major_minor: Option<(u32, u32)>,
    pub systemd_version: Option<u32>,
    pub virtualization: VirtualizationKind,
    pub is_root: bool,
    pub can_sudo_noninteractive: bool,
    pub ksm_sysfs_writable: bool,
}

/// Computes the 4-tier capability matrix strictly adhering to Graceful Degradation rules.
pub fn compute_capabilities(facts: &PlatformFacts) -> CapabilityMatrix {
    match facts.os {
        HostOs::Linux => {
            let is_restricted = facts.virtualization.is_restricted_container_vps();
            let has_priv = facts.is_root;

            let (tier3_host_tuning, tier3_reason) = if !has_priv {
                (
                    CapabilitySupport::Partial,
                    "Non-root mode: skipping /etc/docker/daemon.json edits (container-level log caps still active)".to_string(),
                )
            } else if is_restricted {
                (
                    CapabilitySupport::Partial,
                    format!(
                        "Containerized Linux ({}): daemon.json tuning available if Docker is installed",
                        facts.virtualization
                    ),
                )
            } else {
                (
                    CapabilitySupport::Full,
                    "Full Linux Docker daemon.json tuning (userland-proxy=false, log rotation, live-restore)".to_string(),
                )
            };

            let kernel_supports_ksm = facts
                .kernel_major_minor
                .map(|(maj, min)| maj > 6 || (maj == 6 && min >= 1))
                .unwrap_or(false);
            let systemd_supports_ksm = facts.systemd_version.map(|v| v >= 254).unwrap_or(false);

            let zram_supported = !is_restricted && has_priv;
            let ksm_supported = !is_restricted
                && has_priv
                && kernel_supports_ksm
                && systemd_supports_ksm
                && facts.ksm_sysfs_writable;
            let quadlet_supported = facts.systemd_version.is_some();

            let (tier4_kernel_opt, tier4_reason) = if is_restricted {
                (
                    CapabilitySupport::Skipped,
                    format!(
                        "Virtualization `{}` shares host kernel (read-only /sys or no modprobe); zram/KSM safely skipped",
                        facts.virtualization
                    ),
                )
            } else if !has_priv {
                (
                    CapabilitySupport::Skipped,
                    "Non-root execution: zram/KSM kernel optimizations safely skipped (run with sudo to enable)".to_string(),
                )
            } else if zram_supported && ksm_supported {
                (
                    CapabilitySupport::Full,
                    "Full Linux kernel optimizations: zram (zstd) + KSM (kernel >= 6.1 & systemd >= 254)".to_string(),
                )
            } else {
                let mut missing = Vec::new();
                if !kernel_supports_ksm {
                    missing.push("kernel < 6.1");
                }
                if !systemd_supports_ksm {
                    missing.push("systemd < 254");
                }
                if !facts.ksm_sysfs_writable {
                    missing.push("/sys/kernel/mm/ksm/run not writable");
                }
                (
                    CapabilitySupport::Partial,
                    format!(
                        "zram (zstd) available; KSM skipped ({})",
                        missing.join(", ")
                    ),
                )
            };

            CapabilityMatrix {
                tier1_compose_and_db: CapabilitySupport::Full,
                tier2_wake_proxy: CapabilitySupport::Full,
                tier3_host_tuning,
                tier3_reason,
                tier4_kernel_opt,
                tier4_reason,
                zram_supported,
                ksm_supported,
                quadlet_supported,
            }
        }
        HostOs::Windows => CapabilityMatrix {
            tier1_compose_and_db: CapabilitySupport::Full,
            tier2_wake_proxy: CapabilitySupport::Full,
            tier3_host_tuning: CapabilitySupport::Full,
            tier3_reason: "Windows WSL2 optimizer: safe %UserProfile%\\.wslconfig tuning & NTFS I/O warning".to_string(),
            tier4_kernel_opt: CapabilitySupport::Skipped,
            tier4_reason: "Windows host: Linux kernel zram/KSM skipped cleanly (use .wslconfig autoMemoryReclaim=gradual)".to_string(),
            zram_supported: false,
            ksm_supported: false,
            quadlet_supported: false,
        },
        HostOs::MacOs => CapabilityMatrix {
            tier1_compose_and_db: CapabilitySupport::Full,
            tier2_wake_proxy: CapabilitySupport::Full,
            tier3_host_tuning: CapabilitySupport::Partial,
            tier3_reason: "macOS runtime inspector: detects Docker Desktop / OrbStack / Colima VM memory guidance".to_string(),
            tier4_kernel_opt: CapabilitySupport::Skipped,
            tier4_reason: "macOS host: Linux kernel zram/KSM skipped cleanly (managed by container Linux VM)".to_string(),
            zram_supported: false,
            ksm_supported: false,
            quadlet_supported: false,
        },
        HostOs::Other => CapabilityMatrix {
            tier1_compose_and_db: CapabilitySupport::Full,
            tier2_wake_proxy: CapabilitySupport::Partial,
            tier3_host_tuning: CapabilitySupport::Skipped,
            tier3_reason: "Unsupported OS for host-level tuning".to_string(),
            tier4_kernel_opt: CapabilitySupport::Skipped,
            tier4_reason: "Non-Linux kernel: zram/KSM skipped cleanly".to_string(),
            zram_supported: false,
            ksm_supported: false,
            quadlet_supported: false,
        },
    }
}

/// Parses a kernel version string such as `"6.8.0-45-generic"` into `(6, 8)`.
pub fn parse_kernel_major_minor(raw: &str) -> Option<(u32, u32)> {
    let mut parts = raw.trim().split(|c: char| !c.is_ascii_digit());
    let major = parts.next()?.parse::<u32>().ok()?;
    let minor = parts.next()?.parse::<u32>().ok()?;
    Some((major, minor))
}

/// Parses `systemctl --version` first line such as `"systemd 255 (255.4-1ubuntu8)"` into `255`.
pub fn parse_systemd_version(raw: &str) -> Option<u32> {
    let first_line = raw.lines().next()?;
    for token in first_line.split_whitespace() {
        if let Ok(num) = token.parse::<u32>() {
            return Some(num);
        }
    }
    None
}

fn detect_os() -> HostOs {
    if cfg!(target_os = "linux") {
        HostOs::Linux
    } else if cfg!(target_os = "windows") {
        HostOs::Windows
    } else if cfg!(target_os = "macos") {
        HostOs::MacOs
    } else {
        HostOs::Other
    }
}

#[cfg(unix)]
fn detect_is_root() -> bool {
    nix::unistd::Uid::effective().is_root()
}

#[cfg(not(unix))]
fn detect_is_root() -> bool {
    false
}

#[cfg(unix)]
fn detect_can_sudo() -> bool {
    if detect_is_root() {
        return true;
    }
    std::process::Command::new("sudo")
        .args(["-n", "true"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn detect_can_sudo() -> bool {
    false
}

/// `(platform label, distro version, kernel major/minor, systemd version, virtualization, is_root)`.
type PlatformDetails = (
    String,
    Option<String>,
    Option<(u32, u32)>,
    Option<u32>,
    VirtualizationKind,
    bool,
);

#[cfg(target_os = "linux")]
fn detect_platform_details(_os: HostOs) -> PlatformDetails {
    let distro = read_linux_distro();
    let kernel_version = std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .ok()
        .map(|s| s.trim().to_string());
    let kernel_major_minor = kernel_version.as_deref().and_then(parse_kernel_major_minor);
    let systemd_version = std::process::Command::new("systemctl")
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| parse_systemd_version(&String::from_utf8_lossy(&o.stdout)));
    let virtualization = detect_linux_virtualization(kernel_version.as_deref().unwrap_or(""));
    let ksm_sysfs_writable = std::fs::OpenOptions::new()
        .write(true)
        .open("/sys/kernel/mm/ksm/run")
        .is_ok();

    (
        distro,
        kernel_version,
        kernel_major_minor,
        systemd_version,
        virtualization,
        ksm_sysfs_writable,
    )
}

#[cfg(target_os = "linux")]
fn read_linux_distro() -> String {
    if let Ok(contents) = std::fs::read_to_string("/etc/os-release") {
        for line in contents.lines() {
            if let Some(val) = line.strip_prefix("PRETTY_NAME=") {
                return val.trim_matches('"').to_string();
            }
        }
    }
    "Linux".to_string()
}

#[cfg(target_os = "linux")]
fn detect_linux_virtualization(kernel_release: &str) -> VirtualizationKind {
    let lower_kernel = kernel_release.to_lowercase();
    if lower_kernel.contains("microsoft") || lower_kernel.contains("wsl") {
        return VirtualizationKind::Wsl2;
    }

    if std::path::Path::new("/proc/user_beancounters").exists() {
        return VirtualizationKind::OpenVz;
    }

    if let Ok(out) = std::process::Command::new("systemd-detect-virt").output() {
        let virt = String::from_utf8_lossy(&out.stdout).trim().to_lowercase();
        match virt.as_str() {
            "none" | "" => return VirtualizationKind::BareMetal,
            "kvm" | "qemu" => return VirtualizationKind::Kvm,
            "vmware" => return VirtualizationKind::Vmware,
            "xen" => return VirtualizationKind::Xen,
            "microsoft" | "hyperv" => return VirtualizationKind::HyperV,
            "lxc" | "lxc-libvirt" => return VirtualizationKind::Lxc,
            "openvz" => return VirtualizationKind::OpenVz,
            "wsl" => return VirtualizationKind::Wsl2,
            _ => {}
        }
    }

    if let Ok(environ) = std::fs::read_to_string("/proc/1/environ") {
        if environ.contains("container=lxc") {
            return VirtualizationKind::Lxc;
        }
    }

    if let Ok(product) = std::fs::read_to_string("/sys/class/dmi/id/product_name") {
        let p = product.to_lowercase();
        if p.contains("kvm") || p.contains("qemu") {
            return VirtualizationKind::Kvm;
        }
        if p.contains("vmware") {
            return VirtualizationKind::Vmware;
        }
    }

    VirtualizationKind::Unknown
}

#[cfg(target_os = "windows")]
fn detect_platform_details(_os: HostOs) -> PlatformDetails {
    (
        "Windows 10/11 (WSL2 Host)".to_string(),
        None,
        None,
        None,
        VirtualizationKind::Wsl2,
        false,
    )
}

#[cfg(target_os = "macos")]
fn detect_platform_details(_os: HostOs) -> PlatformDetails {
    let ver = std::process::Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| format!("macOS {}", String::from_utf8_lossy(&o.stdout).trim()))
        .unwrap_or_else(|| "macOS".to_string());
    (ver, None, None, None, VirtualizationKind::MacVm, false)
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn detect_platform_details(_os: HostOs) -> PlatformDetails {
    (
        std::env::consts::OS.to_string(),
        None,
        None,
        None,
        VirtualizationKind::Unknown,
        false,
    )
}
