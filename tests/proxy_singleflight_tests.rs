use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use leandock::error::Result;
use leandock::proxy::discover_idle_routes;
use leandock::proxy::engine_client::{ContainerEngine, ManagedContainer};
use leandock::proxy::wake_manager::{IdleRouteConfig, ReadinessProbe, WakeManager, WakePhase};

struct MockEngine {
    start_calls: AtomicUsize,
    stop_calls: AtomicUsize,
    running: AtomicBool,
    start_delay: Duration,
}

impl MockEngine {
    fn new(start_delay: Duration) -> Self {
        Self {
            start_calls: AtomicUsize::new(0),
            stop_calls: AtomicUsize::new(0),
            running: AtomicBool::new(false),
            start_delay,
        }
    }
}

#[async_trait]
impl ContainerEngine for MockEngine {
    async fn list_containers(&self) -> Result<Vec<ManagedContainer>> {
        Ok(Vec::new())
    }

    async fn is_running(&self, _container_id: &str) -> Result<bool> {
        Ok(self.running.load(Ordering::SeqCst))
    }

    async fn start_container(&self, _container_id: &str) -> Result<()> {
        self.start_calls.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(self.start_delay).await;
        self.running.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn stop_container(&self, _container_id: &str) -> Result<()> {
        self.stop_calls.fetch_add(1, Ordering::SeqCst);
        self.running.store(false, Ordering::SeqCst);
        Ok(())
    }
}

struct MockProbe {
    ready: AtomicBool,
}

#[async_trait]
impl ReadinessProbe for MockProbe {
    async fn is_ready(&self, _host: &str, _port: u16) -> bool {
        self.ready.load(Ordering::SeqCst)
    }
}

#[tokio::test]
async fn fifty_concurrent_requests_trigger_exactly_one_container_start() {
    let engine = Arc::new(MockEngine::new(Duration::from_millis(80)));
    let probe = Arc::new(MockProbe {
        ready: AtomicBool::new(true),
    });

    let route = IdleRouteConfig {
        service_name: "demo-app".into(),
        container_id: "cid-demo-1".into(),
        listen_port: 18080,
        target_host: "127.0.0.1".into(),
        target_port: 13000,
        idle_timeout: Duration::from_secs(60),
        wake_timeout: Duration::from_secs(5),
    };

    let mut initial = HashMap::new();
    initial.insert("demo-app".to_string(), false);

    let manager = Arc::new(WakeManager::with_probe(
        engine.clone(),
        probe,
        vec![route],
        &initial,
    ));

    let mut handles = Vec::new();
    for _ in 0..50 {
        let mgr = manager.clone();
        handles.push(tokio::spawn(
            async move { mgr.ensure_awake("demo-app").await },
        ));
    }

    for h in handles {
        let res = h.await.expect("task join");
        assert!(res.is_ok(), "all 50 concurrent waiters must succeed");
    }

    assert_eq!(
        engine.start_calls.load(Ordering::SeqCst),
        1,
        "singleflight MUST coalesce 50 concurrent requests into exactly 1 start_container call"
    );

    let phases = manager.snapshot_phases().await;
    assert_eq!(phases.get("demo-app"), Some(&WakePhase::Running));
}

#[tokio::test]
async fn wake_timeout_returns_error_and_resets_phase_to_sleeping() {
    let engine = Arc::new(MockEngine::new(Duration::from_millis(10)));
    let probe = Arc::new(MockProbe {
        ready: AtomicBool::new(false),
    });

    let route = IdleRouteConfig {
        service_name: "slow-app".into(),
        container_id: "cid-slow".into(),
        listen_port: 18081,
        target_host: "127.0.0.1".into(),
        target_port: 13001,
        idle_timeout: Duration::from_secs(60),
        wake_timeout: Duration::from_millis(250),
    };

    let mut initial = HashMap::new();
    initial.insert("slow-app".to_string(), false);

    let manager = WakeManager::with_probe(engine, probe, vec![route], &initial);
    let err = manager
        .ensure_awake("slow-app")
        .await
        .expect_err("must time out");
    assert!(err.to_string().contains("timed out"));

    let phases = manager.snapshot_phases().await;
    assert_eq!(phases.get("slow-app"), Some(&WakePhase::Sleeping));
}

#[tokio::test]
async fn idle_reaper_stops_idle_containers_and_respects_ignore_label() {
    let engine = Arc::new(MockEngine::new(Duration::from_millis(10)));
    engine.running.store(true, Ordering::SeqCst);
    let probe = Arc::new(MockProbe {
        ready: AtomicBool::new(true),
    });

    let route = IdleRouteConfig {
        service_name: "idle-app".into(),
        container_id: "cid-idle".into(),
        listen_port: 18082,
        target_host: "127.0.0.1".into(),
        target_port: 13002,
        idle_timeout: Duration::from_millis(50),
        wake_timeout: Duration::from_secs(5),
    };

    let mut initial = HashMap::new();
    initial.insert("idle-app".to_string(), true);

    let manager = WakeManager::with_probe(engine.clone(), probe, vec![route], &initial);
    tokio::time::sleep(Duration::from_millis(80)).await;

    let stopped = manager.reap_idle_once().await;
    assert_eq!(stopped, vec!["idle-app".to_string()]);
    assert_eq!(engine.stop_calls.load(Ordering::SeqCst), 1);

    // Ensure `leandock.ignore=true` skips Wake-on-Request discovery completely.
    let mut ignored_labels = BTreeMap::new();
    ignored_labels.insert("leandock.ignore".into(), "true".into());
    ignored_labels.insert("leandock.idle.enable".into(), "true".into());
    let containers = vec![ManagedContainer {
        id: "c1".into(),
        name: "protected".into(),
        running: true,
        labels: ignored_labels,
    }];
    let (discovered, _) = discover_idle_routes(&containers);
    assert!(discovered.is_empty());
}
