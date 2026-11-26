pub mod engine_client;
pub mod server;
pub mod wake_manager;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use crate::error::{LeanDockError, Result};
use crate::prerun::static_offload::OffloadedSite;
use crate::profiler::HostProfile;
use crate::proxy::engine_client::{create_default_engine, ManagedContainer};
use crate::proxy::server::run_proxy_server;
use crate::proxy::wake_manager::{IdleRouteConfig, WakeManager};

/// Parses container labels (`leandock.idle.*`) into `IdleRouteConfig` entries,
/// skipping any container marked with `leandock.ignore=true`.
pub fn discover_idle_routes(
    containers: &[ManagedContainer],
) -> (Vec<IdleRouteConfig>, HashMap<String, bool>) {
    let mut routes = Vec::new();
    let mut running_map = HashMap::new();

    for c in containers {
        if label_truthy(&c.labels, "leandock.ignore") {
            continue;
        }
        if !label_truthy(&c.labels, "leandock.idle.enable") {
            continue;
        }

        let listen_port = c
            .labels
            .get("leandock.idle.listen_port")
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(8080);
        let target_port = c
            .labels
            .get("leandock.idle.target_port")
            .and_then(|v| v.parse::<u16>().ok())
            .unwrap_or(3000);
        let target_host = c
            .labels
            .get("leandock.idle.target_host")
            .cloned()
            .unwrap_or_else(|| "127.0.0.1".to_string());
        let idle_timeout = c
            .labels
            .get("leandock.idle.timeout")
            .and_then(|v| humantime::parse_duration(v).ok())
            .unwrap_or_else(|| Duration::from_secs(600));
        let wake_timeout = c
            .labels
            .get("leandock.idle.wake_timeout")
            .and_then(|v| humantime::parse_duration(v).ok())
            .unwrap_or_else(|| Duration::from_secs(30));

        let service_name = c
            .labels
            .get("com.docker.compose.service")
            .cloned()
            .unwrap_or_else(|| c.name.clone());

        running_map.insert(service_name.clone(), c.running);
        routes.push(IdleRouteConfig {
            service_name,
            container_id: c.id.clone(),
            listen_port,
            target_host,
            target_port,
            idle_timeout,
            wake_timeout,
        });
    }

    (routes, running_map)
}

/// Parses manual `--port-map service:listen_port:target_port[:timeout]` overrides.
pub fn parse_port_map_spec(spec: &str, containers: &[ManagedContainer]) -> Result<IdleRouteConfig> {
    let parts: Vec<&str> = spec.split(':').collect();
    if parts.len() < 3 {
        return Err(LeanDockError::InvalidConfig {
            field: "--port-map".to_string(),
            reason: format!(
                "Expected `service:listen_port:target_port[:idle_timeout]`, got `{}`",
                spec
            ),
        });
    }

    let service_name = parts[0].trim().to_string();
    let listen_port = parts[1]
        .parse::<u16>()
        .map_err(|_| LeanDockError::InvalidConfig {
            field: "--port-map listen_port".to_string(),
            reason: format!("Invalid port `{}`", parts[1]),
        })?;
    let target_port = parts[2]
        .parse::<u16>()
        .map_err(|_| LeanDockError::InvalidConfig {
            field: "--port-map target_port".to_string(),
            reason: format!("Invalid port `{}`", parts[2]),
        })?;
    let idle_timeout = parts
        .get(3)
        .and_then(|d| humantime::parse_duration(d).ok())
        .unwrap_or_else(|| Duration::from_secs(600));

    let container_id = containers
        .iter()
        .find(|c| {
            c.name == service_name
                || c.labels
                    .get("com.docker.compose.service")
                    .map(|s| s == &service_name)
                    .unwrap_or(false)
        })
        .map(|c| c.id.clone())
        .unwrap_or_else(|| service_name.clone());

    Ok(IdleRouteConfig {
        service_name,
        container_id,
        listen_port,
        target_host: "127.0.0.1".to_string(),
        target_port,
        idle_timeout,
        wake_timeout: Duration::from_secs(30),
    })
}

/// Boots the cross-platform Wake-on-Request reverse proxy and optional static offload server.
pub async fn start_proxy_daemon(
    profile: &HostProfile,
    project_dir: &Path,
    port_maps: &[String],
) -> Result<()> {
    let runtime_bin = match profile.runtime.active_runtime {
        crate::profiler::ContainerRuntimeKind::Podman => "podman".to_string(),
        _ => "docker".to_string(),
    };

    let engine: Arc<dyn engine_client::ContainerEngine> = Arc::from(create_default_engine(
        profile.runtime.socket_path.clone(),
        runtime_bin,
    ));

    let containers = engine.list_containers().await.unwrap_or_default();
    let (mut routes, mut running_map) = discover_idle_routes(&containers);

    for spec in port_maps {
        let route = parse_port_map_spec(spec, &containers)?;
        let is_running = containers
            .iter()
            .find(|c| c.id == route.container_id)
            .map(|c| c.running)
            .unwrap_or(false);
        running_map.insert(route.service_name.clone(), is_running);
        routes.push(route);
    }

    let static_sites = load_static_sites(project_dir);

    if routes.is_empty() && static_sites.is_empty() {
        return Err(LeanDockError::Proxy(
            "No containers with `leandock.idle.enable=true`, `--port-map`, or `.leandock/static/sites.json` found."
                .to_string(),
        ));
    }

    let wake_manager = Arc::new(WakeManager::new(engine, routes.clone(), &running_map));
    run_proxy_server(wake_manager, routes, static_sites).await
}

fn load_static_sites(project_dir: &Path) -> Vec<OffloadedSite> {
    let manifest = project_dir
        .join(".leandock")
        .join("static")
        .join("sites.json");
    std::fs::read_to_string(manifest)
        .ok()
        .and_then(|raw| serde_json::from_str::<Vec<OffloadedSite>>(&raw).ok())
        .unwrap_or_default()
}

fn label_truthy(labels: &std::collections::BTreeMap<String, String>, key: &str) -> bool {
    labels
        .get(key)
        .map(|v| {
            matches!(
                v.trim().to_lowercase().as_str(),
                "true" | "1" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}
