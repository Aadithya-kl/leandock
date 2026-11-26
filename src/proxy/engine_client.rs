use std::collections::BTreeMap;
use std::path::PathBuf;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::process::Command;

use crate::error::{LeanDockError, Result};

/// Minimal container descriptor returned by `ContainerEngine::list_containers`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedContainer {
    pub id: String,
    pub name: String,
    pub running: bool,
    pub labels: BTreeMap<String, String>,
}

/// Cross-platform container lifecycle abstraction used by Wake-on-Request and `status`.
/// - Unix (`Linux` / `macOS`): direct HTTP-over-Unix-Socket via Hyper v1 (`UnixSocketEngine`).
/// - Windows (or fallback when socket is inaccessible): async CLI adapter (`CliContainerEngine`).
#[async_trait]
pub trait ContainerEngine: Send + Sync {
    async fn list_containers(&self) -> Result<Vec<ManagedContainer>>;
    async fn is_running(&self, container_id: &str) -> Result<bool>;
    async fn start_container(&self, container_id: &str) -> Result<()>;
    async fn stop_container(&self, container_id: &str) -> Result<()>;
}

/// Selects the optimal cross-platform `ContainerEngine` for the current host.
pub fn create_default_engine(
    socket_path: Option<PathBuf>,
    runtime_bin: String,
) -> Box<dyn ContainerEngine> {
    #[cfg(unix)]
    {
        if let Some(sock) = socket_path {
            if sock.exists() {
                return Box::new(UnixSocketEngine::new(sock));
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = socket_path;
    }

    Box::new(CliContainerEngine::new(runtime_bin))
}

/// Lightweight HTTP-over-Unix-Socket Docker/Podman Engine API client (Unix only).
#[cfg(unix)]
#[derive(Debug, Clone)]
pub struct UnixSocketEngine {
    socket_path: PathBuf,
}

#[cfg(unix)]
impl UnixSocketEngine {
    pub fn new(socket_path: PathBuf) -> Self {
        Self { socket_path }
    }

    async fn request(&self, method: &str, path: &str) -> Result<(u16, bytes::Bytes)> {
        use bytes::Bytes;
        use http_body_util::{BodyExt, Full};
        use hyper_util::rt::TokioIo;
        use tokio::net::UnixStream;

        let stream = UnixStream::connect(&self.socket_path).await.map_err(|e| {
            LeanDockError::Proxy(format!(
                "Failed to connect to Unix socket {}: {}",
                self.socket_path.display(),
                e
            ))
        })?;

        let io = TokioIo::new(stream);
        let (mut sender, conn) = hyper::client::conn::http1::handshake(io)
            .await
            .map_err(|e| {
                LeanDockError::Proxy(format!("Unix socket HTTP handshake failed: {}", e))
            })?;

        tokio::spawn(async move {
            if let Err(err) = conn.await {
                tracing::debug!(error = %err, "Unix socket HTTP connection closed");
            }
        });

        let req = http::Request::builder()
            .method(method)
            .uri(path)
            .header(http::header::HOST, "localhost")
            .body(Full::new(Bytes::new()))
            .map_err(|e| LeanDockError::Proxy(format!("Invalid HTTP request: {}", e)))?;

        let res = sender.send_request(req).await.map_err(|e| {
            LeanDockError::Proxy(format!("Docker Engine API request failed: {}", e))
        })?;

        let status = res.status().as_u16();
        let body = res
            .into_body()
            .collect()
            .await
            .map_err(|e| LeanDockError::Proxy(format!("Failed reading Engine API body: {}", e)))?
            .to_bytes();

        Ok((status, body))
    }
}

#[cfg(unix)]
#[derive(Debug, Deserialize)]
struct DockerApiContainer {
    #[serde(rename = "Id")]
    id: String,
    #[serde(rename = "Names", default)]
    names: Vec<String>,
    #[serde(rename = "State", default)]
    state: String,
    #[serde(rename = "Labels", default)]
    labels: Option<BTreeMap<String, String>>,
}

#[cfg(unix)]
#[derive(Debug, Deserialize)]
struct DockerInspectResponse {
    #[serde(rename = "State")]
    state: DockerInspectState,
}

#[cfg(unix)]
#[derive(Debug, Deserialize)]
struct DockerInspectState {
    #[serde(rename = "Running", default)]
    running: bool,
}

#[cfg(unix)]
#[async_trait]
impl ContainerEngine for UnixSocketEngine {
    async fn list_containers(&self) -> Result<Vec<ManagedContainer>> {
        let (status, body) = self.request("GET", "/containers/json?all=true").await?;
        if status >= 400 {
            return Err(LeanDockError::Proxy(format!(
                "Engine API /containers/json returned HTTP {}",
                status
            )));
        }

        let raw: Vec<DockerApiContainer> = serde_json::from_slice(&body)?;
        Ok(raw
            .into_iter()
            .map(|c| ManagedContainer {
                id: c.id,
                name: c
                    .names
                    .first()
                    .map(|n| n.trim_start_matches('/').to_string())
                    .unwrap_or_default(),
                running: c.state.eq_ignore_ascii_case("running"),
                labels: c.labels.unwrap_or_default(),
            })
            .collect())
    }

    async fn is_running(&self, container_id: &str) -> Result<bool> {
        let path = format!("/containers/{}/json", container_id);
        let (status, body) = self.request("GET", &path).await?;
        if status >= 400 {
            return Ok(false);
        }
        let parsed: DockerInspectResponse = serde_json::from_slice(&body)?;
        Ok(parsed.state.running)
    }

    async fn start_container(&self, container_id: &str) -> Result<()> {
        let path = format!("/containers/{}/start", container_id);
        let (status, body) = self.request("POST", &path).await?;
        // 204 = started, 304 = already started
        if status == 204 || status == 304 || status == 200 {
            Ok(())
        } else {
            Err(LeanDockError::Proxy(format!(
                "Failed to start container {} (HTTP {}): {}",
                container_id,
                status,
                String::from_utf8_lossy(&body).trim()
            )))
        }
    }

    async fn stop_container(&self, container_id: &str) -> Result<()> {
        let path = format!("/containers/{}/stop?t=10", container_id);
        let (status, body) = self.request("POST", &path).await?;
        // 204 = stopped, 304 = already stopped
        if status == 204 || status == 304 || status == 200 {
            Ok(())
        } else {
            Err(LeanDockError::Proxy(format!(
                "Failed to stop container {} (HTTP {}): {}",
                container_id,
                status,
                String::from_utf8_lossy(&body).trim()
            )))
        }
    }
}

/// Cross-platform async CLI adapter (`docker` / `podman`) used on Windows and as Unix fallback.
#[derive(Debug, Clone)]
pub struct CliContainerEngine {
    runtime_bin: String,
}

impl CliContainerEngine {
    pub fn new(runtime_bin: String) -> Self {
        Self { runtime_bin }
    }
}

#[async_trait]
impl ContainerEngine for CliContainerEngine {
    async fn list_containers(&self) -> Result<Vec<ManagedContainer>> {
        let out = Command::new(&self.runtime_bin)
            .args([
                "ps",
                "-a",
                "--no-trunc",
                "--format",
                "{{.ID}}|{{.Names}}|{{.State}}|{{.Labels}}",
            ])
            .output()
            .await?;

        if !out.status.success() {
            return Err(LeanDockError::CommandFailed {
                cmd: format!("{} ps -a", self.runtime_bin),
                code: out.status.code(),
                stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
            });
        }

        let stdout = String::from_utf8_lossy(&out.stdout);
        let mut containers = Vec::new();
        for line in stdout.lines() {
            let parts: Vec<&str> = line.splitn(4, '|').collect();
            if parts.len() < 4 {
                continue;
            }
            let labels = parse_comma_labels(parts[3]);
            containers.push(ManagedContainer {
                id: parts[0].trim().to_string(),
                name: parts[1].trim().to_string(),
                running: parts[2].trim().eq_ignore_ascii_case("running"),
                labels,
            });
        }

        Ok(containers)
    }

    async fn is_running(&self, container_id: &str) -> Result<bool> {
        let out = Command::new(&self.runtime_bin)
            .args(["inspect", "-f", "{{.State.Running}}", container_id])
            .output()
            .await?;
        Ok(out.status.success() && String::from_utf8_lossy(&out.stdout).trim() == "true")
    }

    async fn start_container(&self, container_id: &str) -> Result<()> {
        let out = Command::new(&self.runtime_bin)
            .args(["start", container_id])
            .output()
            .await?;
        if out.status.success() {
            Ok(())
        } else {
            Err(LeanDockError::CommandFailed {
                cmd: format!("{} start {}", self.runtime_bin, container_id),
                code: out.status.code(),
                stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
            })
        }
    }

    async fn stop_container(&self, container_id: &str) -> Result<()> {
        let out = Command::new(&self.runtime_bin)
            .args(["stop", "-t", "10", container_id])
            .output()
            .await?;
        if out.status.success() {
            Ok(())
        } else {
            Err(LeanDockError::CommandFailed {
                cmd: format!("{} stop {}", self.runtime_bin, container_id),
                code: out.status.code(),
                stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
            })
        }
    }
}

fn parse_comma_labels(raw: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for pair in raw.split(',') {
        if let Some((k, v)) = pair.split_once('=') {
            map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    map
}
