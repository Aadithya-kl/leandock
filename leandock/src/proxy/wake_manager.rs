use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tokio::sync::{Mutex, Notify};

use crate::error::{LeanDockError, Result};
use crate::proxy::engine_client::ContainerEngine;

/// Configuration for a single Wake-on-Request managed service route.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdleRouteConfig {
    pub service_name: String,
    pub container_id: String,
    pub listen_port: u16,
    pub target_host: String,
    pub target_port: u16,
    pub idle_timeout: Duration,
    pub wake_timeout: Duration,
}

impl IdleRouteConfig {
    pub fn target_addr_string(&self) -> String {
        format!("{}:{}", self.target_host, self.target_port)
    }
}

/// Readiness probe abstraction (default probes TCP connect; injectable for tests).
#[async_trait]
pub trait ReadinessProbe: Send + Sync {
    async fn is_ready(&self, host: &str, port: u16) -> bool;
}

/// Default TCP connect readiness probe.
#[derive(Debug, Default)]
pub struct TcpReadinessProbe;

#[async_trait]
impl ReadinessProbe for TcpReadinessProbe {
    async fn is_ready(&self, host: &str, port: u16) -> bool {
        let addr_str = format!("{}:{}", host, port);
        if let Ok(addr) = addr_str.parse::<SocketAddr>() {
            return tokio::time::timeout(Duration::from_millis(400), TcpStream::connect(addr))
                .await
                .map(|r| r.is_ok())
                .unwrap_or(false);
        }
        tokio::time::timeout(Duration::from_millis(400), TcpStream::connect(addr_str))
            .await
            .map(|r| r.is_ok())
            .unwrap_or(false)
    }
}

/// Current lifecycle phase of a Wake-on-Request service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WakePhase {
    Sleeping,
    Waking,
    Running,
}

#[derive(Debug)]
struct ServiceRuntimeState {
    config: IdleRouteConfig,
    phase: WakePhase,
    last_activity: Instant,
    last_error: Option<String>,
    notify: Arc<Notify>,
}

/// Coordinates singleflight wake-ups and idle container stopping across all managed services.
pub struct WakeManager {
    engine: Arc<dyn ContainerEngine>,
    probe: Arc<dyn ReadinessProbe>,
    services: HashMap<String, Arc<Mutex<ServiceRuntimeState>>>,
}

impl WakeManager {
    /// Creates a `WakeManager` with the default TCP readiness probe.
    pub fn new(
        engine: Arc<dyn ContainerEngine>,
        routes: Vec<IdleRouteConfig>,
        initially_running: &HashMap<String, bool>,
    ) -> Self {
        Self::with_probe(engine, Arc::new(TcpReadinessProbe), routes, initially_running)
    }

    /// Creates a `WakeManager` with a custom `ReadinessProbe` (used in unit/integration tests).
    pub fn with_probe(
        engine: Arc<dyn ContainerEngine>,
        probe: Arc<dyn ReadinessProbe>,
        routes: Vec<IdleRouteConfig>,
        initially_running: &HashMap<String, bool>,
    ) -> Self {
        let mut services = HashMap::new();
        for route in routes {
            let running = *initially_running.get(&route.service_name).unwrap_or(&false);
            let phase = if running {
                WakePhase::Running
            } else {
                WakePhase::Sleeping
            };
            let state = ServiceRuntimeState {
                config: route.clone(),
                phase,
                last_activity: Instant::now(),
                last_error: None,
                notify: Arc::new(Notify::new()),
            };
            services.insert(route.service_name, Arc::new(Mutex::new(state)));
        }

        Self {
            engine,
            probe,
            services,
        }
    }

    /// Ensures the container for `service_name` is running and accepting TCP connections.
    /// Concurrent callers hitting a sleeping service coalesce via singleflight (`Notify`) so
    /// EXACTLY ONE `start_container` call is issued.
    pub async fn ensure_awake(&self, service_name: &str) -> Result<IdleRouteConfig> {
        let state_arc = self
            .services
            .get(service_name)
            .ok_or_else(|| LeanDockError::Proxy(format!("Unknown service `{}`", service_name)))?
            .clone();

        loop {
            let (is_leader, notify, cfg) = {
                let mut guard = state_arc.lock().await;
                guard.last_activity = Instant::now();

                match guard.phase {
                    WakePhase::Running => {
                        return Ok(guard.config.clone());
                    }
                    WakePhase::Waking => {
                        // Another task is already waking this container; wait for its notification.
                        (false, guard.notify.clone(), guard.config.clone())
                    }
                    WakePhase::Sleeping => {
                        // Claim singleflight leadership.
                        guard.phase = WakePhase::Waking;
                        guard.last_error = None;
                        (true, guard.notify.clone(), guard.config.clone())
                    }
                }
            };

            if !is_leader {
                // Wait until the leader finishes waking (or fails).
                notify.notified().await;
                let guard = state_arc.lock().await;
                match guard.phase {
                    WakePhase::Running => return Ok(guard.config.clone()),
                    WakePhase::Sleeping => {
                        if let Some(ref err_msg) = guard.last_error {
                            return Err(LeanDockError::Proxy(err_msg.clone()));
                        }
                        // Retry loop if woken state raced with a stop.
                        continue;
                    }
                    WakePhase::Waking => continue,
                }
            }

            // Leader path: start the container and poll target TCP readiness.
            let wake_result = self.perform_wake(&cfg).await;

            let mut guard = state_arc.lock().await;
            match wake_result {
                Ok(()) => {
                    guard.phase = WakePhase::Running;
                    guard.last_activity = Instant::now();
                    guard.last_error = None;
                    guard.notify.notify_waiters();
                    return Ok(cfg);
                }
                Err(err) => {
                    guard.phase = WakePhase::Sleeping;
                    guard.last_error = Some(err.to_string());
                    guard.notify.notify_waiters();
                    return Err(err);
                }
            }
        }
    }

    async fn perform_wake(&self, cfg: &IdleRouteConfig) -> Result<()> {
        tracing::info!(
            service = %cfg.service_name,
            container = %cfg.container_id,
            "Waking sleeping container on incoming request"
        );

        self.engine.start_container(&cfg.container_id).await?;

        let deadline = Instant::now() + cfg.wake_timeout;
        let poll_step = Duration::from_millis(100);

        while Instant::now() < deadline {
            if self.probe.is_ready(&cfg.target_host, cfg.target_port).await {
                tracing::info!(
                    service = %cfg.service_name,
                    target = %cfg.target_addr_string(),
                    "Container is awake and accepting connections"
                );
                return Ok(());
            }
            tokio::time::sleep(poll_step).await;
        }

        Err(LeanDockError::WakeTimeout {
            service: cfg.service_name.clone(),
            timeout_secs: cfg.wake_timeout.as_secs().max(1),
        })
    }

    /// Scans all managed services and gracefully stops any running service idle beyond `idle_timeout`.
    pub async fn reap_idle_once(&self) -> Vec<String> {
        let mut stopped = Vec::new();

        for (name, state_arc) in &self.services {
            let should_stop = {
                let guard = state_arc.lock().await;
                guard.phase == WakePhase::Running
                    && guard.last_activity.elapsed() >= guard.config.idle_timeout
            };

            if should_stop {
                let container_id = {
                    let guard = state_arc.lock().await;
                    guard.config.container_id.clone()
                };

                tracing::info!(
                    service = %name,
                    container = %container_id,
                    "Stopping idle container to reclaim RAM (0 MB idle)"
                );

                if let Err(err) = self.engine.stop_container(&container_id).await {
                    tracing::warn!(service = %name, error = %err, "Failed stopping idle container");
                } else {
                    let mut guard = state_arc.lock().await;
                    if guard.phase == WakePhase::Running
                        && guard.last_activity.elapsed() >= guard.config.idle_timeout
                    {
                        guard.phase = WakePhase::Sleeping;
                        stopped.push(name.clone());
                    }
                }
            }
        }

        stopped
    }

    /// Returns a snapshot of `(service_name, WakePhase)` for status display or tests.
    pub async fn snapshot_phases(&self) -> HashMap<String, WakePhase> {
        let mut out = HashMap::new();
        for (name, state_arc) in &self.services {
            let guard = state_arc.lock().await;
            out.insert(name.clone(), guard.phase.clone());
        }
        out
    }
}
