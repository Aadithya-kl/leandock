use std::path::PathBuf;
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::profiler::platform::{HostOs, PlatformProfile};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContainerRuntimeKind {
    Docker,
    Podman,
    None,
}

impl std::fmt::Display for ContainerRuntimeKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContainerRuntimeKind::Docker => write!(f, "docker"),
            ContainerRuntimeKind::Podman => write!(f, "podman"),
            ContainerRuntimeKind::None => write!(f, "none"),
        }
    }
}

/// Detected Compose invocation strategy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComposeDriver {
    /// `docker compose ...` (v2 plugin)
    DockerComposeV2,
    /// `docker-compose ...` (legacy v1 standalone)
    DockerComposeV1,
    /// `podman compose ...`
    PodmanCompose,
    None,
}

impl ComposeDriver {
    /// Returns the executable and fixed prefix args for invoking Compose.
    pub fn command_prefix(&self) -> Option<(&'static str, Vec<&'static str>)> {
        match self {
            ComposeDriver::DockerComposeV2 => Some(("docker", vec!["compose"])),
            ComposeDriver::DockerComposeV1 => Some(("docker-compose", vec![])),
            ComposeDriver::PodmanCompose => Some(("podman", vec!["compose"])),
            ComposeDriver::None => None,
        }
    }
}

impl std::fmt::Display for ComposeDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ComposeDriver::DockerComposeV2 => write!(f, "docker compose (v2)"),
            ComposeDriver::DockerComposeV1 => write!(f, "docker-compose (v1)"),
            ComposeDriver::PodmanCompose => write!(f, "podman compose"),
            ComposeDriver::None => write!(f, "none"),
        }
    }
}

/// Discovered container runtimes, sockets, and optional helper binaries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeInfo {
    pub active_runtime: ContainerRuntimeKind,
    pub compose_driver: ComposeDriver,
    pub docker_version: Option<String>,
    pub podman_version: Option<String>,
    pub socket_path: Option<PathBuf>,
    pub macos_vm_provider: Option<String>,
    pub vm_memory_limit_mb: Option<u64>,
    pub has_podlet: bool,
    pub has_slim: bool,
}

impl RuntimeInfo {
    /// Probes Docker, Podman, Compose plugins, Unix sockets, and helper tools (`podlet`, `slim`).
    pub fn detect(platform: &PlatformProfile) -> Self {
        let docker_version = run_version_cmd("docker", &["--version"]);
        let podman_version = run_version_cmd("podman", &["--version"]);

        let has_docker_compose_v2 = run_cmd_ok("docker", &["compose", "version"]);
        let has_docker_compose_v1 = run_cmd_ok("docker-compose", &["--version"]);
        let has_podman_compose = run_cmd_ok("podman", &["compose", "version"]);

        let (active_runtime, compose_driver) = if docker_version.is_some() && has_docker_compose_v2
        {
            (ContainerRuntimeKind::Docker, ComposeDriver::DockerComposeV2)
        } else if docker_version.is_some() && has_docker_compose_v1 {
            (ContainerRuntimeKind::Docker, ComposeDriver::DockerComposeV1)
        } else if podman_version.is_some() && has_podman_compose {
            (ContainerRuntimeKind::Podman, ComposeDriver::PodmanCompose)
        } else if docker_version.is_some() {
            (ContainerRuntimeKind::Docker, ComposeDriver::None)
        } else if podman_version.is_some() {
            (ContainerRuntimeKind::Podman, ComposeDriver::None)
        } else {
            (ContainerRuntimeKind::None, ComposeDriver::None)
        };

        let socket_path = discover_container_socket(platform.os);
        let macos_vm_provider = if platform.os == HostOs::MacOs {
            detect_macos_vm_provider(&socket_path)
        } else {
            None
        };
        let vm_memory_limit_mb = if matches!(platform.os, HostOs::Windows | HostOs::MacOs) {
            detect_docker_vm_memory_mb()
        } else {
            None
        };

        let has_podlet = run_cmd_ok("podlet", &["--version"]);
        let has_slim = run_cmd_ok("slim", &["--version"]);

        Self {
            active_runtime,
            compose_driver,
            docker_version,
            podman_version,
            socket_path,
            macos_vm_provider,
            vm_memory_limit_mb,
            has_podlet,
            has_slim,
        }
    }

    /// Resolves the compose driver honoring an explicit `--engine docker|podman` override.
    pub fn resolve_driver(&self, requested_engine: Option<&str>) -> ComposeDriver {
        match requested_engine {
            Some("podman") => {
                if self.podman_version.is_some() {
                    ComposeDriver::PodmanCompose
                } else {
                    ComposeDriver::None
                }
            }
            Some("docker") => {
                if run_cmd_ok("docker", &["compose", "version"]) {
                    ComposeDriver::DockerComposeV2
                } else if run_cmd_ok("docker-compose", &["--version"]) {
                    ComposeDriver::DockerComposeV1
                } else {
                    ComposeDriver::None
                }
            }
            _ => self.compose_driver.clone(),
        }
    }
}

fn run_cmd_ok(bin: &str, args: &[&str]) -> bool {
    Command::new(bin)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn run_version_cmd(bin: &str, args: &[&str]) -> Option<String> {
    Command::new(bin)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

fn detect_docker_vm_memory_mb() -> Option<u64> {
    let out = Command::new("docker")
        .args(["info", "--format", "{{.MemTotal}}"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let bytes: u64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
    Some(bytes / (1024 * 1024))
}

fn discover_container_socket(os: HostOs) -> Option<PathBuf> {
    if let Ok(host) = std::env::var("DOCKER_HOST") {
        if let Some(stripped) = host.strip_prefix("unix://") {
            let p = PathBuf::from(stripped);
            if p.exists() {
                return Some(p);
            }
        }
    }

    if os == HostOs::Windows {
        return None;
    }

    let mut candidates = vec![PathBuf::from("/var/run/docker.sock")];

    if let Ok(xdg) = std::env::var("XDG_RUNTIME_DIR") {
        candidates.push(PathBuf::from(&xdg).join("podman/podman.sock"));
        candidates.push(PathBuf::from(&xdg).join("docker.sock"));
    }

    if let Ok(home) = std::env::var("HOME") {
        let h = PathBuf::from(home);
        candidates.push(h.join(".orbstack/run/docker.sock"));
        candidates.push(h.join(".colima/default/docker.sock"));
        candidates.push(h.join(".docker/run/docker.sock"));
        candidates.push(h.join(".lima/default/sock/docker.sock"));
    }

    candidates.into_iter().find(|p| p.exists())
}

fn detect_macos_vm_provider(socket_path: &Option<PathBuf>) -> Option<String> {
    if let Some(path) = socket_path {
        let s = path.to_string_lossy();
        if s.contains(".orbstack") {
            return Some("OrbStack".to_string());
        }
        if s.contains(".colima") {
            return Some("Colima".to_string());
        }
        if s.contains(".lima") {
            return Some("Lima".to_string());
        }
        if s.contains(".docker/run") {
            return Some("Docker Desktop".to_string());
        }
    }
    if run_cmd_ok("orb", &["version"]) {
        return Some("OrbStack".to_string());
    }
    if run_cmd_ok("colima", &["version"]) {
        return Some("Colima".to_string());
    }
    Some("Docker Desktop / Unknown VM".to_string())
}
