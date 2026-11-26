use leandock::profiler::platform::{
    parse_kernel_major_minor, parse_systemd_version, CapabilitySupport, HostOs, PlatformFacts,
    PlatformProfile, VirtualizationKind,
};

#[test]
fn linux_kvm_root_modern_kernel_enables_all_four_tiers() {
    let profile = PlatformProfile::from_facts(PlatformFacts {
        os: HostOs::Linux,
        distro: "Ubuntu 24.04 LTS".into(),
        kernel_version: Some("6.8.0-45-generic".into()),
        kernel_major_minor: parse_kernel_major_minor("6.8.0-45-generic"),
        systemd_version: parse_systemd_version("systemd 255 (255.4-1ubuntu8)"),
        virtualization: VirtualizationKind::Kvm,
        is_root: true,
        can_sudo_noninteractive: true,
        ksm_sysfs_writable: true,
    });

    assert_eq!(profile.capabilities.tier1_compose_and_db, CapabilitySupport::Full);
    assert_eq!(profile.capabilities.tier2_wake_proxy, CapabilitySupport::Full);
    assert_eq!(profile.capabilities.tier3_host_tuning, CapabilitySupport::Full);
    assert_eq!(profile.capabilities.tier4_kernel_opt, CapabilitySupport::Full);
    assert!(profile.capabilities.zram_supported);
    assert!(profile.capabilities.ksm_supported);
    assert!(profile.capabilities.quadlet_supported);
}

#[test]
fn linux_kvm_older_kernel_or_systemd_skips_ksm_cleanly_while_keeping_zram() {
    let profile = PlatformProfile::from_facts(PlatformFacts {
        os: HostOs::Linux,
        distro: "Ubuntu 22.04 LTS".into(),
        kernel_version: Some("5.15.0-112-generic".into()),
        kernel_major_minor: parse_kernel_major_minor("5.15.0-112-generic"),
        systemd_version: Some(249),
        virtualization: VirtualizationKind::Kvm,
        is_root: true,
        can_sudo_noninteractive: true,
        ksm_sysfs_writable: true,
    });

    assert_eq!(profile.capabilities.tier4_kernel_opt, CapabilitySupport::Partial);
    assert!(profile.capabilities.zram_supported);
    assert!(!profile.capabilities.ksm_supported);
    assert!(profile.capabilities.tier4_reason.contains("kernel < 6.1"));
}

#[test]
fn linux_lxc_and_openvz_auto_skip_tier4_without_failing() {
    for virt in [VirtualizationKind::Lxc, VirtualizationKind::OpenVz] {
        let profile = PlatformProfile::from_facts(PlatformFacts {
            os: HostOs::Linux,
            distro: "Debian 12".into(),
            kernel_version: Some("6.8.0".into()),
            kernel_major_minor: Some((6, 8)),
            systemd_version: Some(255),
            virtualization: virt,
            is_root: true,
            can_sudo_noninteractive: true,
            ksm_sysfs_writable: false,
        });

        assert_eq!(profile.capabilities.tier1_compose_and_db, CapabilitySupport::Full);
        assert_eq!(profile.capabilities.tier2_wake_proxy, CapabilitySupport::Full);
        assert_eq!(profile.capabilities.tier3_host_tuning, CapabilitySupport::Partial);
        assert_eq!(profile.capabilities.tier4_kernel_opt, CapabilitySupport::Skipped);
        assert!(!profile.capabilities.zram_supported);
        assert!(!profile.capabilities.ksm_supported);
    }
}

#[test]
fn linux_non_root_keeps_tier1_and_tier2_full_and_skips_privileged_tiers() {
    let profile = PlatformProfile::from_facts(PlatformFacts {
        os: HostOs::Linux,
        distro: "Ubuntu 24.04 LTS".into(),
        kernel_version: Some("6.8.0".into()),
        kernel_major_minor: Some((6, 8)),
        systemd_version: Some(255),
        virtualization: VirtualizationKind::Kvm,
        is_root: false,
        can_sudo_noninteractive: false,
        ksm_sysfs_writable: false,
    });

    assert_eq!(profile.capabilities.tier1_compose_and_db, CapabilitySupport::Full);
    assert_eq!(profile.capabilities.tier2_wake_proxy, CapabilitySupport::Full);
    assert_eq!(profile.capabilities.tier3_host_tuning, CapabilitySupport::Partial);
    assert_eq!(profile.capabilities.tier4_kernel_opt, CapabilitySupport::Skipped);
}

#[test]
fn windows_and_macos_enable_appropriate_tiers_and_skip_linux_kernel_tier() {
    let win = PlatformProfile::from_facts(PlatformFacts {
        os: HostOs::Windows,
        distro: "Windows 11".into(),
        kernel_version: None,
        kernel_major_minor: None,
        systemd_version: None,
        virtualization: VirtualizationKind::Wsl2,
        is_root: false,
        can_sudo_noninteractive: false,
        ksm_sysfs_writable: false,
    });
    assert_eq!(win.capabilities.tier1_compose_and_db, CapabilitySupport::Full);
    assert_eq!(win.capabilities.tier2_wake_proxy, CapabilitySupport::Full);
    assert_eq!(win.capabilities.tier3_host_tuning, CapabilitySupport::Full);
    assert_eq!(win.capabilities.tier4_kernel_opt, CapabilitySupport::Skipped);

    let mac = PlatformProfile::from_facts(PlatformFacts {
        os: HostOs::MacOs,
        distro: "macOS 15.1".into(),
        kernel_version: None,
        kernel_major_minor: None,
        systemd_version: None,
        virtualization: VirtualizationKind::MacVm,
        is_root: false,
        can_sudo_noninteractive: false,
        ksm_sysfs_writable: false,
    });
    assert_eq!(mac.capabilities.tier1_compose_and_db, CapabilitySupport::Full);
    assert_eq!(mac.capabilities.tier2_wake_proxy, CapabilitySupport::Full);
    assert_eq!(mac.capabilities.tier3_host_tuning, CapabilitySupport::Partial);
    assert_eq!(mac.capabilities.tier4_kernel_opt, CapabilitySupport::Skipped);
}
