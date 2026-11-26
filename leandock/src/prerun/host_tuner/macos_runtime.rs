#![cfg(target_os = "macos")]

use crate::prerun::host_tuner::HostTuningReport;
use crate::profiler::HostProfile;

/// macOS Tier 3 inspector: detects Docker Desktop / OrbStack / Colima / Lima
/// and reports actionable VM memory guidance without touching macOS system internals.
pub fn inspect_macos_runtime(profile: &HostProfile) -> HostTuningReport {
    let mut report = HostTuningReport::default();

    let provider = profile
        .runtime
        .macos_vm_provider
        .as_deref()
        .unwrap_or("Unknown Container VM");

    let vm_ram_desc = profile
        .runtime
        .vm_memory_limit_mb
        .map(|mb| format!("{} MB allocated to Linux VM", mb))
        .unwrap_or_else(|| "VM memory limit not reported by daemon".to_string());

    report.skipped.push(format!(
        "macOS detected ({} — {}): host OS internals left untouched; Tier 1 Compose & DB caps active",
        provider, vm_ram_desc
    ));

    if provider.contains("Docker Desktop") {
        if let Some(vm_mb) = profile.runtime.vm_memory_limit_mb {
            if vm_mb > profile.hardware.total_ram_mb / 2 {
                report.warnings.push(format!(
                    "Docker Desktop VM is allocated {} MB (>50% of host RAM). Consider lowering Resources -> Memory or switching to OrbStack/Colima for dynamic memory reclamation.",
                    vm_mb
                ));
            }
        }
    }

    report
}
