use leandock::prerun::compose_parser::ParsedCompose;
use leandock::prerun::db_tuner::{build_optimization_plan, render_override_yaml, TunerOptions};

#[test]
fn ignore_label_works_for_both_map_and_list_formats() {
    let yaml = r#"
services:
  db_map_ignored:
    image: mongo:7
    labels:
      leandock.ignore: "true"
  db_list_ignored:
    image: postgres:16
    labels:
      - "leandock.ignore=true"
  api:
    image: python:3.12-slim
"#;

    let compose = ParsedCompose::from_yaml_str(yaml, None).expect("parse compose");
    let plan = build_optimization_plan(
        &compose,
        &TunerOptions {
            effective_ram_mb: 2048,
            logical_cores: 2,
            ..Default::default()
        },
    );

    let map_svc = plan
        .services
        .iter()
        .find(|s| s.name == "db_map_ignored")
        .unwrap();
    let list_svc = plan
        .services
        .iter()
        .find(|s| s.name == "db_list_ignored")
        .unwrap();
    let api_svc = plan.services.iter().find(|s| s.name == "api").unwrap();

    assert!(map_svc.ignored);
    assert!(map_svc.mem_limit_mb.is_none());
    assert!(map_svc.tuned_command.is_none());

    assert!(list_svc.ignored);
    assert!(list_svc.mem_limit_mb.is_none());
    assert!(list_svc.tuned_command.is_none());

    assert!(!api_svc.ignored);
    assert!(api_svc.mem_limit_mb.is_some());

    let override_yaml = render_override_yaml(&plan).expect("render override");
    assert!(!override_yaml.contains("db_map_ignored"));
    assert!(!override_yaml.contains("db_list_ignored"));
    assert!(override_yaml.contains("api:"));
}

#[test]
fn custom_db_commands_preserve_args_deduplicate_and_skip_shell_strings() {
    let yaml = r#"
services:
  mongo_repl:
    image: mongo:7.0
    command: ["mongod", "--replSet", "rs0"]
  mongo_already_tuned:
    image: mongo:7.0
    command: ["mongod", "--wiredTigerCacheSizeGB", "1.5"]
  postgres_shell:
    image: postgres:16
    command: sh -c "exec postgres -c log_statement=all"
"#;

    let compose = ParsedCompose::from_yaml_str(yaml, None).expect("parse compose");
    let plan = build_optimization_plan(
        &compose,
        &TunerOptions {
            effective_ram_mb: 4096,
            logical_cores: 2,
            ..Default::default()
        },
    );

    let repl = plan
        .services
        .iter()
        .find(|s| s.name == "mongo_repl")
        .unwrap();
    let repl_cmd = repl.tuned_command.as_ref().expect("mongo_repl tuned");
    assert_eq!(repl_cmd[0], "mongod");
    assert!(repl_cmd.contains(&"--replSet".to_string()));
    assert!(repl_cmd.contains(&"rs0".to_string()));
    assert!(repl_cmd.contains(&"--wiredTigerCacheSizeGB".to_string()));

    let tuned = plan
        .services
        .iter()
        .find(|s| s.name == "mongo_already_tuned")
        .unwrap();
    assert!(
        tuned.tuned_command.is_none(),
        "must not duplicate user's --wiredTigerCacheSizeGB"
    );

    let pg_shell = plan
        .services
        .iter()
        .find(|s| s.name == "postgres_shell")
        .unwrap();
    assert!(
        pg_shell.tuned_command.is_none(),
        "must skip shell-wrapper commands safely"
    );
    assert!(
        plan.warnings.iter().any(|w| w.contains("postgres_shell")),
        "must emit a warning explaining why shell command was skipped"
    );
}

#[test]
fn redis_preserves_persistence_by_default_and_only_evicts_when_explicitly_ephemeral() {
    let yaml = r#"
services:
  redis_queue:
    image: redis:7-alpine
  redis_cache:
    image: valkey/valkey:8
    labels:
      leandock.redis.ephemeral: "true"
"#;

    let compose = ParsedCompose::from_yaml_str(yaml, None).expect("parse compose");
    let plan = build_optimization_plan(
        &compose,
        &TunerOptions {
            effective_ram_mb: 2048,
            logical_cores: 2,
            ephemeral_cache: false,
            ..Default::default()
        },
    );

    let queue_cmd = plan
        .services
        .iter()
        .find(|s| s.name == "redis_queue")
        .unwrap()
        .tuned_command
        .as_ref()
        .expect("redis_queue command");
    assert!(queue_cmd.contains(&"--maxmemory".to_string()));
    assert!(queue_cmd.contains(&"noeviction".to_string()));
    assert!(
        !queue_cmd.contains(&"--save".to_string()),
        "default Redis mode must NEVER disable RDB persistence"
    );
    assert!(
        !queue_cmd.contains(&"allkeys-lru".to_string()),
        "default Redis mode must NEVER silently switch to allkeys-lru"
    );

    let cache_cmd = plan
        .services
        .iter()
        .find(|s| s.name == "redis_cache")
        .unwrap()
        .tuned_command
        .as_ref()
        .expect("redis_cache command");
    assert!(cache_cmd.contains(&"allkeys-lru".to_string()));
    assert!(cache_cmd.contains(&"--save".to_string()));
}

#[test]
fn ultra_low_ram_vps_enforces_safe_minimum_floors_and_warns() {
    let yaml = r#"
services:
  mongo:
    image: mongo:7
  postgres:
    image: postgres:16
  mysql:
    image: mariadb:11
  redis:
    image: redis:7-alpine
  qdrant:
    image: qdrant/qdrant:latest
"#;

    let compose = ParsedCompose::from_yaml_str(yaml, None).expect("parse compose");
    let plan = build_optimization_plan(
        &compose,
        &TunerOptions {
            effective_ram_mb: 512,
            logical_cores: 1,
            swap_and_zram_mb: 0,
            ..Default::default()
        },
    );

    // MongoDB WiredTiger floor >= 0.25 GB
    let mongo_cmd = plan
        .services
        .iter()
        .find(|s| s.name == "mongo")
        .unwrap()
        .tuned_command
        .as_ref()
        .unwrap();
    let wt_idx = mongo_cmd
        .iter()
        .position(|a| a == "--wiredTigerCacheSizeGB")
        .unwrap();
    let wt_val: f64 = mongo_cmd[wt_idx + 1].parse().unwrap();
    assert!(
        wt_val >= 0.25,
        "WiredTiger floor must be >= 0.25, got {}",
        wt_val
    );

    // Postgres shared_buffers floor >= 64MB
    let pg_cmd = plan
        .services
        .iter()
        .find(|s| s.name == "postgres")
        .unwrap()
        .tuned_command
        .as_ref()
        .unwrap();
    assert!(pg_cmd.iter().any(|a| a == "shared_buffers=64MB"));

    // MySQL/MariaDB disables performance-schema on <= 2GB VPS and enforces >= 64M buffer pool
    let mysql_cmd = plan
        .services
        .iter()
        .find(|s| s.name == "mysql")
        .unwrap()
        .tuned_command
        .as_ref()
        .unwrap();
    assert!(mysql_cmd.iter().any(|a| a == "--performance-schema=OFF"));
    assert!(mysql_cmd
        .iter()
        .any(|a| a.starts_with("--innodb-buffer-pool-size=")));

    // Qdrant caps optimization threads on <= 2-core VPS
    let qdrant_env = &plan
        .services
        .iter()
        .find(|s| s.name == "qdrant")
        .unwrap()
        .extra_env;
    assert_eq!(
        qdrant_env.get("QDRANT__STORAGE__PERFORMANCE__MAX_OPTIMIZATION_THREADS"),
        Some(&"1".to_string())
    );

    // Must warn when minimum stack footprint exceeds available RAM + swap
    assert!(
        plan.warnings.iter().any(|w| w.contains("Ultra-low RAM")),
        "expected ultra-low RAM warning, got: {:?}",
        plan.warnings
    );
}
