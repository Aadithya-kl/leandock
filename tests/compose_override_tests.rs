use std::fs;

use leandock::backup::BackupManager;
use leandock::postrun::health_guard::{is_container_failing, ContainerHealthState};
use leandock::prerun::compose_parser::ParsedCompose;
use leandock::prerun::db_tuner::{build_optimization_plan, TunerOptions};
use leandock::prerun::host_tuner::{merge_daemon_json, merge_wslconfig_ini};
use leandock::prerun::write_override_file;
use tempfile::tempdir;

#[test]
fn non_destructive_override_preserves_original_file_and_v2_v3_limits() {
    let tmp = tempdir().expect("tempdir");
    let original_yaml = r#"services:
  api:
    build: ./api
    environment:
      DATABASE_URL: ${DATABASE_URL:-postgres://user:pass@db:5432/app}
    mem_limit: 512m
  worker:
    image: myorg/worker:${TAG:-latest}
    deploy:
      resources:
        limits:
          memory: 768M
  frontend:
    image: nginx:alpine
"#;
    let compose_path = tmp.path().join("docker-compose.yml");
    fs::write(&compose_path, original_yaml).expect("write original compose");

    let parsed = ParsedCompose::from_file(&compose_path).expect("parse file");
    let plan = build_optimization_plan(
        &parsed,
        &TunerOptions {
            effective_ram_mb: 2048,
            logical_cores: 2,
            ..Default::default()
        },
    );

    let override_path = write_override_file(tmp.path(), &plan).expect("write override");
    assert!(override_path.exists());

    // Original Compose file MUST remain byte-for-byte untouched (preserving ${VAR:-default}).
    let after_original = fs::read_to_string(&compose_path).expect("read original");
    assert_eq!(after_original, original_yaml);

    // Override respects existing v2 `mem_limit` and v3 `deploy.resources.limits.memory`.
    let api_plan = plan.services.iter().find(|s| s.name == "api").unwrap();
    let worker_plan = plan.services.iter().find(|s| s.name == "worker").unwrap();
    let fe_plan = plan.services.iter().find(|s| s.name == "frontend").unwrap();

    assert!(api_plan.mem_limit_mb.is_none(), "must preserve v2 mem_limit");
    assert!(
        worker_plan.mem_limit_mb.is_none(),
        "must preserve v3 deploy.resources.limits.memory"
    );
    assert!(
        fe_plan.mem_limit_mb.is_some(),
        "frontend without memory limit must receive a cap"
    );

    let override_content = fs::read_to_string(&override_path).expect("read override");
    assert!(override_content.contains("json-file"));
    assert!(override_content.contains("max-size: 10m"));
}

#[test]
fn daemon_json_deep_merge_preserves_existing_keys_and_backups_revert_cleanly() {
    let existing = r#"{
  "storage-driver": "overlay2",
  "insecure-registries": ["localhost:5000"]
}"#;
    let (merged, changed) = merge_daemon_json(Some(existing)).expect("merge daemon.json");
    assert!(changed);
    let parsed: serde_json::Value = serde_json::from_str(&merged).expect("valid json");
    assert_eq!(parsed["storage-driver"], "overlay2");
    assert_eq!(parsed["insecure-registries"][0], "localhost:5000");
    assert_eq!(parsed["userland-proxy"], false);
    assert_eq!(parsed["live-restore"], true);
    assert_eq!(parsed["log-driver"], "json-file");
    assert_eq!(parsed["log-opts"]["max-size"], "10m");
    assert_eq!(parsed["log-opts"]["max-file"], "3");

    // Verify transactional backup and `leandock revert` restoration.
    let tmp = tempdir().expect("tempdir");
    let target_file = tmp.path().join("daemon.json");
    fs::write(&target_file, existing).expect("write initial");

    let mgr = BackupManager::with_base_dir(tmp.path().join("backups"));
    let mut session = mgr.start_session().expect("session");
    session
        .backup_and_write(&target_file, merged.as_bytes(), "docker-daemon-json", None)
        .expect("backup_and_write");

    assert_eq!(fs::read_to_string(&target_file).unwrap(), merged);

    let actions = mgr.revert(false).expect("revert");
    assert!(!actions.is_empty());
    assert_eq!(fs::read_to_string(&target_file).unwrap(), existing);
}

#[test]
fn wslconfig_ini_merger_preserves_user_keys_and_adds_reclaim_defaults() {
    let existing = "[wsl2]\nprocessors=4\n";
    let (merged, changed) = merge_wslconfig_ini(Some(existing), 16384);
    assert!(changed);
    assert!(merged.contains("processors=4"));
    assert!(merged.contains("memory=6GB"));
    assert!(merged.contains("autoMemoryReclaim=gradual"));
    assert!(merged.contains("sparseVhd=true"));
}

#[test]
fn health_guard_detects_crash_loops_and_nonzero_exits() {
    let healthy = ContainerHealthState {
        id: "1".into(),
        name: "api".into(),
        status: "running".into(),
        restart_count: 0,
        exit_code: 0,
        health: Some("healthy".into()),
    };
    assert!(!is_container_failing(&healthy));

    let restarting = ContainerHealthState {
        id: "2".into(),
        name: "db".into(),
        status: "restarting".into(),
        restart_count: 3,
        exit_code: 137,
        health: None,
    };
    assert!(is_container_failing(&restarting));

    let exited_err = ContainerHealthState {
        id: "3".into(),
        name: "worker".into(),
        status: "exited".into(),
        restart_count: 0,
        exit_code: 1,
        health: None,
    };
    assert!(is_container_failing(&exited_err));
}

#[test]
fn cli_analyze_and_dry_run_are_strictly_zero_touch() {
    use assert_cmd::Command;
    use predicates::prelude::*;

    let tmp = tempdir().expect("tempdir");
    let compose_path = tmp.path().join("docker-compose.yml");
    let sample = r#"services:
  mongo:
    image: mongo:7.0
    command: ["mongod", "--replSet", "rs0"]
  redis:
    image: redis:7-alpine
  billing:
    image: ghcr.io/example/billing:1.0
    labels:
      leandock.ignore: "true"
"#;
    fs::write(&compose_path, sample).expect("write sample compose");

    // 1. `leandock analyze` succeeds and leaves `.leandock/` uncreated.
    Command::cargo_bin("leandock")
        .expect("binary")
        .args(["analyze", "-f", compose_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("LeanDock Pre-Flight Analysis"))
        .stdout(predicate::str::contains("IGNORED"));

    assert!(
        !tmp.path().join(".leandock").exists(),
        "`leandock analyze` must be strictly zero-touch"
    );

    // 2. `leandock up --dry-run` succeeds without creating `.leandock/`.
    Command::cargo_bin("leandock")
        .expect("binary")
        .args(["up", "--dry-run", "-f", compose_path.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Dry run complete"));

    assert!(
        !tmp.path().join(".leandock").exists(),
        "`leandock up --dry-run` must not write `.leandock/`"
    );

    // 3. `leandock doctor` runs cleanly on every OS (Linux, macOS, Windows).
    Command::cargo_bin("leandock")
        .expect("binary")
        .arg("doctor")
        .assert()
        .success()
        .stdout(predicate::str::contains("LeanDock Environment Doctor"));
}
