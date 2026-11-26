use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_yaml_ng::{Mapping, Value};

use crate::error::Result;
use crate::prerun::compose_parser::{
    ComposeCommand, DatabaseKind, ParsedCompose, ParsedService, ServiceRole,
};

/// Tuning configuration options passed from CLI flags.
#[derive(Debug, Clone, Default)]
pub struct TunerOptions {
    /// Total RAM available to the container host/VM in MB.
    pub effective_ram_mb: u64,
    /// Logical CPU cores on host.
    pub logical_cores: usize,
    /// Available swap/zram capacity in MB (used for low-RAM overcommit warnings).
    pub swap_and_zram_mb: u64,
    /// Opt-in flag to configure Redis/Valkey as an ephemeral cache (`allkeys-lru` + `--save ""`).
    pub ephemeral_cache: bool,
    /// Map of services disabled via `--extract-static` (will receive `profiles: ["leandock-offloaded"]`).
    pub offloaded_frontends: Vec<String>,
    /// Map of service -> minified image tag when `--minify` succeeds.
    pub minified_images: BTreeMap<String, String>,
}

/// Complete optimization plan generated for a Compose project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationPlan {
    pub effective_ram_mb: u64,
    pub os_reserve_mb: u64,
    pub allocatable_ram_mb: u64,
    pub total_budgeted_mb: u64,
    pub estimated_default_ram_mb: u64,
    pub estimated_savings_mb: u64,
    pub services: Vec<ServiceTuningPlan>,
    pub warnings: Vec<String>,
}

/// Per-service planned changes for reporting and `.leandock/compose.override.yml` emission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceTuningPlan {
    pub name: String,
    pub role: ServiceRole,
    pub db_kind: Option<DatabaseKind>,
    pub ignored: bool,
    pub mem_limit_mb: Option<u64>,
    pub mem_reservation_mb: Option<u64>,
    pub inject_log_rotation: bool,
    pub tuned_command: Option<Vec<String>>,
    pub extra_env: BTreeMap<String, String>,
    pub offloaded_static: bool,
    pub minified_image: Option<String>,
    pub notes: Vec<String>,
}

/// Computes a hardware-aware, non-destructive `OptimizationPlan` for all services.
pub fn build_optimization_plan(
    compose: &ParsedCompose,
    opts: &TunerOptions,
) -> OptimizationPlan {
    let total_ram_mb = opts.effective_ram_mb.max(256);
    let os_reserve_mb = ((total_ram_mb as f64 * 0.15).round() as u64).max(384);
    let allocatable_ram_mb = total_ram_mb.saturating_sub(os_reserve_mb).max(256);

    let mut warnings = Vec::new();

    // Assign relative budgeting weights to non-ignored services that lack explicit limits.
    let mut weight_sum: u64 = 0;
    let mut weights: BTreeMap<String, u64> = BTreeMap::new();
    for (name, svc) in &compose.services {
        if svc.is_ignored() || opts.offloaded_frontends.contains(name) {
            continue;
        }
        let w = role_weight(svc.role, svc.db_kind);
        weights.insert(name.clone(), w);
        weight_sum += w;
    }

    let mut service_plans = Vec::new();
    let mut total_budgeted_mb: u64 = 0;
    let mut estimated_default_ram_mb: u64 = 0;

    for (name, svc) in &compose.services {
        if svc.is_ignored() {
            service_plans.push(ServiceTuningPlan {
                name: name.clone(),
                role: svc.role,
                db_kind: svc.db_kind,
                ignored: true,
                mem_limit_mb: None,
                mem_reservation_mb: None,
                inject_log_rotation: false,
                tuned_command: None,
                extra_env: BTreeMap::new(),
                offloaded_static: false,
                minified_image: None,
                notes: vec!["Skipped completely (`leandock.ignore=true`)".to_string()],
            });
            continue;
        }

        let offloaded_static = opts.offloaded_frontends.contains(name);
        if offloaded_static {
            estimated_default_ram_mb += 96;
            service_plans.push(ServiceTuningPlan {
                name: name.clone(),
                role: svc.role,
                db_kind: svc.db_kind,
                ignored: false,
                mem_limit_mb: None,
                mem_reservation_mb: None,
                inject_log_rotation: false,
                tuned_command: None,
                extra_env: BTreeMap::new(),
                offloaded_static: true,
                minified_image: None,
                notes: vec![
                    "Offloaded to LeanDock built-in Rust static server (`--extract-static`)".to_string(),
                ],
            });
            continue;
        }

        let mut notes = Vec::new();
        let w = *weights.get(name).unwrap_or(&2);
        let raw_share_mb = if weight_sum > 0 {
            (allocatable_ram_mb.saturating_mul(w)) / weight_sum
        } else {
            256
        };

        let floor_mb = minimum_service_ram_mb(svc.role, svc.db_kind);
        let ceiling_mb = maximum_service_ram_mb(svc.role, svc.db_kind, total_ram_mb);
        let target_ram_mb = raw_share_mb.clamp(floor_mb, ceiling_mb);

        estimated_default_ram_mb += estimate_unbounded_default_mb(svc, total_ram_mb);
        total_budgeted_mb += target_ram_mb;

        let (mem_limit_mb, mem_reservation_mb) = if svc.has_existing_memory_limit() {
            notes.push("Preserved user-defined memory limit from Compose file".to_string());
            (None, None)
        } else {
            let reservation = ((target_ram_mb as f64 * 0.45).round() as u64).max(32);
            (Some(target_ram_mb), Some(reservation))
        };

        let inject_log_rotation = !svc.has_logging;
        if !inject_log_rotation {
            notes.push("Preserved existing `logging:` config".to_string());
        }

        let mut tuned_command = None;
        let mut extra_env = BTreeMap::new();

        if let Some(db_kind) = svc.db_kind {
            tune_database_service(
                svc,
                db_kind,
                target_ram_mb,
                opts,
                &mut tuned_command,
                &mut extra_env,
                &mut notes,
                &mut warnings,
            );
        }

        let minified_image = opts.minified_images.get(name).cloned();
        if let Some(ref min_img) = minified_image {
            notes.push(format!("Using verified minified image `{}`", min_img));
        }

        service_plans.push(ServiceTuningPlan {
            name: name.clone(),
            role: svc.role,
            db_kind: svc.db_kind,
            ignored: false,
            mem_limit_mb,
            mem_reservation_mb,
            inject_log_rotation,
            tuned_command,
            extra_env,
            offloaded_static: false,
            minified_image,
            notes,
        });
    }

    let capacity_with_swap = total_ram_mb.saturating_add(opts.swap_and_zram_mb);
    if total_budgeted_mb > allocatable_ram_mb {
        let msg = if total_budgeted_mb > capacity_with_swap {
            format!(
                "Ultra-low RAM warning: minimum safe stack footprint (~{} MB) exceeds available RAM + swap ({} MB). Consider enabling zram or Wake-on-Request (`leandock.idle.enable=true`) for low-traffic services.",
                total_budgeted_mb, capacity_with_swap
            )
        } else {
            format!(
                "Tight RAM budget: safe minimum floors total ~{} MB vs {} MB allocatable RAM (headroom covered by swap/zram).",
                total_budgeted_mb, allocatable_ram_mb
            )
        };
        warnings.push(msg);
    }

    let estimated_savings_mb = estimated_default_ram_mb.saturating_sub(total_budgeted_mb);

    OptimizationPlan {
        effective_ram_mb: total_ram_mb,
        os_reserve_mb,
        allocatable_ram_mb,
        total_budgeted_mb,
        estimated_default_ram_mb,
        estimated_savings_mb,
        services: service_plans,
        warnings,
    }
}

/// Emits the non-destructive `.leandock/compose.override.yml` YAML string.
/// Original Compose files and their `${VAR:-default}` expressions are never modified.
pub fn render_override_yaml(plan: &OptimizationPlan) -> Result<String> {
    let mut services_map = Mapping::new();

    for svc in &plan.services {
        if svc.ignored {
            continue;
        }

        let mut entry = Mapping::new();

        if svc.offloaded_static {
            let profiles = Value::Sequence(vec![Value::String("leandock-offloaded".into())]);
            entry.insert(Value::String("profiles".into()), profiles);
            services_map.insert(Value::String(svc.name.clone()), Value::Mapping(entry));
            continue;
        }

        if let Some(limit_mb) = svc.mem_limit_mb {
            entry.insert(
                Value::String("mem_limit".into()),
                Value::String(format!("{}m", limit_mb)),
            );
        }

        if let Some(res_mb) = svc.mem_reservation_mb {
            entry.insert(
                Value::String("mem_reservation".into()),
                Value::String(format!("{}m", res_mb)),
            );
        }

        if svc.inject_log_rotation {
            let mut log_opts = Mapping::new();
            log_opts.insert(Value::String("max-size".into()), Value::String("10m".into()));
            log_opts.insert(Value::String("max-file".into()), Value::String("3".into()));

            let mut logging = Mapping::new();
            logging.insert(
                Value::String("driver".into()),
                Value::String("json-file".into()),
            );
            logging.insert(Value::String("options".into()), Value::Mapping(log_opts));

            entry.insert(Value::String("logging".into()), Value::Mapping(logging));
        }

        if let Some(cmd) = &svc.tuned_command {
            let seq = cmd.iter().cloned().map(Value::String).collect();
            entry.insert(Value::String("command".into()), Value::Sequence(seq));
        }

        if !svc.extra_env.is_empty() {
            let mut env_map = Mapping::new();
            for (k, v) in &svc.extra_env {
                env_map.insert(Value::String(k.clone()), Value::String(v.clone()));
            }
            entry.insert(Value::String("environment".into()), Value::Mapping(env_map));
        }

        if let Some(ref min_img) = svc.minified_image {
            entry.insert(Value::String("image".into()), Value::String(min_img.clone()));
        }

        if !entry.is_empty() {
            services_map.insert(Value::String(svc.name.clone()), Value::Mapping(entry));
        }
    }

    let mut root = Mapping::new();
    root.insert(
        Value::String("services".into()),
        Value::Mapping(services_map),
    );

    let header = "# Generated by LeanDock — DO NOT EDIT MANUALLY.\n# Original Compose file is untouched. Run `leandock revert` to remove.\n";
    let body = serde_yaml_ng::to_string(&Value::Mapping(root))?;
    Ok(format!("{}{}", header, body))
}

#[allow(clippy::too_many_arguments)]
fn tune_database_service(
    svc: &ParsedService,
    db_kind: DatabaseKind,
    budget_mb: u64,
    opts: &TunerOptions,
    tuned_command: &mut Option<Vec<String>>,
    extra_env: &mut BTreeMap<String, String>,
    notes: &mut Vec<String>,
    warnings: &mut Vec<String>,
) {
    // Qdrant is tuned cleanly via environment variables, never touching command.
    if db_kind == DatabaseKind::Qdrant {
        if opts.logical_cores <= 2
            && !svc
                .environment
                .contains_key("QDRANT__STORAGE__PERFORMANCE__MAX_OPTIMIZATION_THREADS")
        {
            extra_env.insert(
                "QDRANT__STORAGE__PERFORMANCE__MAX_OPTIMIZATION_THREADS".to_string(),
                "1".to_string(),
            );
            notes.push("Qdrant: set MAX_OPTIMIZATION_THREADS=1 for <=2-core host".to_string());
        }
        return;
    }

    if svc.command.is_ambiguous_shell() {
        let msg = format!(
            "Service `{}` ({}) uses a shell-wrapper `command:`; skipping DB flag injection to avoid breaking custom startup logic",
            svc.name, db_kind
        );
        notes.push("Skipped DB flag injection (shell-string command detected)".to_string());
        warnings.push(msg);
        return;
    }

    let existing_args = extract_command_tokens(&svc.command);

    match db_kind {
        DatabaseKind::MongoDb => {
            if has_flag(&existing_args, "--wiredTigerCacheSizeGB") {
                notes.push("MongoDB: preserved user `--wiredTigerCacheSizeGB`".to_string());
                return;
            }
            // Scale WiredTiger cache to ~55% of container budget, hard floor 0.25 GB.
            let cache_gb = (((budget_mb as f64 * 0.55) / 1024.0) * 100.0).round() / 100.0;
            let safe_cache_gb = cache_gb.max(0.25);
            let mut cmd = if existing_args.is_empty() {
                vec!["mongod".to_string()]
            } else {
                existing_args
            };
            cmd.push("--wiredTigerCacheSizeGB".to_string());
            cmd.push(format!("{:.2}", safe_cache_gb));
            notes.push(format!(
                "MongoDB: capped WiredTiger cache to {:.2} GB",
                safe_cache_gb
            ));
            *tuned_command = Some(cmd);
        }
        DatabaseKind::Postgres => {
            let shared_buffers_mb = ((budget_mb as f64 * 0.25).round() as u64).max(64);
            let effective_cache_mb = ((budget_mb as f64 * 0.60).round() as u64).max(128);
            let work_mem_mb = (budget_mb / 128).clamp(2, 16);
            let max_connections = if opts.effective_ram_mb <= 1024 {
                30
            } else if opts.effective_ram_mb <= 2048 {
                50
            } else {
                100
            };

            let mut cmd = if existing_args.is_empty() {
                vec!["postgres".to_string()]
            } else {
                existing_args
            };

            let mut added = Vec::new();
            let params = [
                ("shared_buffers", format!("{}MB", shared_buffers_mb)),
                ("effective_cache_size", format!("{}MB", effective_cache_mb)),
                ("work_mem", format!("{}MB", work_mem_mb)),
                ("max_connections", max_connections.to_string()),
            ];

            for (key, val) in params {
                if !has_postgres_param(&cmd, key) {
                    cmd.push("-c".to_string());
                    cmd.push(format!("{}={}", key, val));
                    added.push(format!("{}={}", key, val));
                }
            }

            if !added.is_empty() {
                notes.push(format!("PostgreSQL: {}", added.join(", ")));
                *tuned_command = Some(cmd);
            } else {
                notes.push("PostgreSQL: preserved all user-specified `-c` parameters".to_string());
            }
        }
        DatabaseKind::RedisOrValkey => {
            let maxmemory_mb = ((budget_mb as f64 * 0.75).round() as u64).max(32);
            let ephemeral = opts.ephemeral_cache
                || svc
                    .label_bool("leandock.redis.ephemeral")
                    .unwrap_or(false);

            let mut cmd = if existing_args.is_empty() {
                vec!["redis-server".to_string()]
            } else {
                existing_args
            };

            let mut changed = false;
            if !has_flag(&cmd, "--maxmemory") {
                cmd.push("--maxmemory".to_string());
                cmd.push(format!("{}mb", maxmemory_mb));
                changed = true;
            }

            if ephemeral {
                if !has_flag(&cmd, "--maxmemory-policy") {
                    cmd.push("--maxmemory-policy".to_string());
                    cmd.push("allkeys-lru".to_string());
                    changed = true;
                }
                if !has_flag(&cmd, "--save") {
                    cmd.push("--save".to_string());
                    cmd.push(String::new());
                    changed = true;
                }
                notes.push(format!(
                    "Redis/Valkey: ephemeral cache mode (`maxmemory {}mb`, `allkeys-lru`, `--save \"\"`)",
                    maxmemory_mb
                ));
            } else {
                // Safe persistence default: never inject `--save ""` or evict keys silently!
                if !has_flag(&cmd, "--maxmemory-policy") {
                    cmd.push("--maxmemory-policy".to_string());
                    cmd.push("noeviction".to_string());
                    changed = true;
                }
                notes.push(format!(
                    "Redis/Valkey: safe persistence mode (`maxmemory {}mb`, `noeviction`; RDB/AOF untouched)",
                    maxmemory_mb
                ));
            }

            if changed {
                *tuned_command = Some(cmd);
            }
        }
        DatabaseKind::MySqlOrMariaDb => {
            let pool_mb = ((budget_mb as f64 * 0.50).round() as u64).max(64);
            let mut cmd = if existing_args.is_empty() {
                vec!["mysqld".to_string()]
            } else {
                existing_args
            };
            let mut changed = false;

            if !has_flag_prefix(&cmd, "--innodb-buffer-pool-size") {
                cmd.push(format!("--innodb-buffer-pool-size={}M", pool_mb));
                changed = true;
            }
            if opts.effective_ram_mb <= 2048 && !has_flag_prefix(&cmd, "--performance-schema") {
                cmd.push("--performance-schema=OFF".to_string());
                changed = true;
            }

            if changed {
                notes.push(format!(
                    "MySQL/MariaDB: innodb-buffer-pool-size={}M{}",
                    pool_mb,
                    if opts.effective_ram_mb <= 2048 {
                        ", performance-schema=OFF"
                    } else {
                        ""
                    }
                ));
                *tuned_command = Some(cmd);
            } else {
                notes.push("MySQL/MariaDB: preserved user InnoDB flags".to_string());
            }
        }
        DatabaseKind::Qdrant => {}
    }
}

fn extract_command_tokens(cmd: &ComposeCommand) -> Vec<String> {
    match cmd {
        ComposeCommand::None => Vec::new(),
        ComposeCommand::Array(v) => v.clone(),
        ComposeCommand::String(s) => s.split_whitespace().map(|t| t.to_string()).collect(),
    }
}

fn has_flag(args: &[String], flag: &str) -> bool {
    let prefix = format!("{}=", flag);
    args.iter().any(|a| a == flag || a.starts_with(&prefix))
}

fn has_flag_prefix(args: &[String], flag: &str) -> bool {
    let prefix = format!("{}=", flag);
    let underscore_flag = flag.replace('-', "_");
    let underscore_prefix = format!("{}=", underscore_flag);
    args.iter().any(|a| {
        a.as_str() == flag
            || a.starts_with(&prefix)
            || a.as_str() == underscore_flag.as_str()
            || a.starts_with(&underscore_prefix)
    })
}

fn has_postgres_param(args: &[String], param: &str) -> bool {
    let eq = format!("{}=", param);
    args.iter()
        .any(|a| a.starts_with(&eq) || a.starts_with(&format!("-c{}=", param)))
}

fn role_weight(role: ServiceRole, db_kind: Option<DatabaseKind>) -> u64 {
    match (role, db_kind) {
        (ServiceRole::Database, Some(DatabaseKind::MongoDb)) => 5,
        (ServiceRole::Database, Some(DatabaseKind::MySqlOrMariaDb)) => 5,
        (ServiceRole::Database, Some(DatabaseKind::Postgres)) => 4,
        (ServiceRole::Database, _) => 4,
        (ServiceRole::App, _) => 3,
        (ServiceRole::Worker, _) => 3,
        (ServiceRole::Cache, _) => 2,
        (ServiceRole::Frontend, _) => 1,
    }
}

fn minimum_service_ram_mb(role: ServiceRole, db_kind: Option<DatabaseKind>) -> u64 {
    match (role, db_kind) {
        (ServiceRole::Database, Some(DatabaseKind::MongoDb)) => 384,
        (ServiceRole::Database, Some(DatabaseKind::MySqlOrMariaDb)) => 256,
        (ServiceRole::Database, Some(DatabaseKind::Postgres)) => 192,
        (ServiceRole::Database, _) => 192,
        (ServiceRole::Cache, _) => 64,
        (ServiceRole::Frontend, _) => 64,
        (ServiceRole::App, _) => 128,
        (ServiceRole::Worker, _) => 128,
    }
}

fn maximum_service_ram_mb(role: ServiceRole, db_kind: Option<DatabaseKind>, host_mb: u64) -> u64 {
    let cap = (host_mb / 2).max(512);
    match (role, db_kind) {
        (ServiceRole::Frontend, _) => 256,
        (ServiceRole::Cache, _) => cap.min(1024),
        (ServiceRole::Database, _) => cap.min(4096),
        (ServiceRole::App | ServiceRole::Worker, _) => cap.min(2048),
    }
}

fn estimate_unbounded_default_mb(svc: &ParsedService, host_ram_mb: u64) -> u64 {
    match svc.db_kind {
        Some(DatabaseKind::MongoDb) => {
            // Default WiredTiger reserves 50% of (RAM - 1GB), min 256MB + overhead
            let wt = ((host_ram_mb.saturating_sub(1024)) / 2).max(256);
            wt + 200
        }
        Some(DatabaseKind::Postgres) => 384.max(host_ram_mb / 5),
        Some(DatabaseKind::MySqlOrMariaDb) => 450.max(host_ram_mb / 4),
        Some(DatabaseKind::RedisOrValkey) => 256.max(host_ram_mb / 8),
        Some(DatabaseKind::Qdrant) => 512.max(host_ram_mb / 4),
        None => match svc.role {
            ServiceRole::Frontend => 96,
            ServiceRole::Worker => 384,
            _ => 320,
        },
    }
}
