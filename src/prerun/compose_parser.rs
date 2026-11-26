use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_yaml_ng::Value;

use crate::error::{LeanDockError, Result};

/// Parsed representation of a Compose service's `command` field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComposeCommand {
    /// No explicit `command` field in the Compose file.
    None,
    /// Sequence form, e.g. `["mongod", "--replSet", "rs0"]`.
    Array(Vec<String>),
    /// Scalar string form, e.g. `"mongod --replSet rs0"` or `"sh -c 'exec mongod'"`.
    String(String),
}

impl ComposeCommand {
    /// Returns true if the command is an ambiguous shell script string that must not be mutated.
    pub fn is_ambiguous_shell(&self) -> bool {
        match self {
            ComposeCommand::None => false,
            ComposeCommand::String(s) => {
                let trimmed = s.trim();
                let lower = trimmed.to_lowercase();
                lower.starts_with("sh ")
                    || lower.starts_with("/bin/sh ")
                    || lower.starts_with("bash ")
                    || lower.starts_with("/bin/bash ")
                    || lower.contains(" -c ")
                    || trimmed.contains("&&")
                    || trimmed.contains("||")
                    || trimmed.contains(';')
                    || trimmed.contains('|')
                    || trimmed.contains('$')
                    || trimmed.contains('"')
                    || trimmed.contains('\'')
            }
            ComposeCommand::Array(items) => {
                if let Some(first) = items.first() {
                    let f = first.to_lowercase();
                    (f == "sh" || f == "/bin/sh" || f == "bash" || f == "/bin/bash")
                        && items.iter().any(|a| a == "-c")
                } else {
                    false
                }
            }
        }
    }
}

/// High-level functional role inferred for memory budgeting and tuning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceRole {
    Database,
    Cache,
    App,
    Frontend,
    Worker,
}

impl std::fmt::Display for ServiceRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ServiceRole::Database => write!(f, "database"),
            ServiceRole::Cache => write!(f, "cache"),
            ServiceRole::App => write!(f, "app"),
            ServiceRole::Frontend => write!(f, "frontend"),
            ServiceRole::Worker => write!(f, "worker"),
        }
    }
}

/// Recognized database / datastore engines for safe flag tuning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseKind {
    MongoDb,
    Postgres,
    RedisOrValkey,
    MySqlOrMariaDb,
    Qdrant,
}

impl std::fmt::Display for DatabaseKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DatabaseKind::MongoDb => write!(f, "MongoDB"),
            DatabaseKind::Postgres => write!(f, "PostgreSQL"),
            DatabaseKind::RedisOrValkey => write!(f, "Redis/Valkey"),
            DatabaseKind::MySqlOrMariaDb => write!(f, "MySQL/MariaDB"),
            DatabaseKind::Qdrant => write!(f, "Qdrant"),
        }
    }
}

/// Parsed Compose service metadata (read-only view of original file).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedService {
    pub name: String,
    pub image: Option<String>,
    pub has_build: bool,
    pub command: ComposeCommand,
    pub labels: BTreeMap<String, String>,
    pub environment: BTreeMap<String, String>,
    pub volumes: Vec<String>,
    pub ports: Vec<String>,
    pub has_mem_limit: bool,
    pub has_deploy_mem_limit: bool,
    pub has_logging: bool,
    pub has_swarm_keys: bool,
    pub mounts_docker_sock: bool,
    pub role: ServiceRole,
    pub db_kind: Option<DatabaseKind>,
}

impl ParsedService {
    /// Returns true if the user opted this service out via `leandock.ignore=true`.
    pub fn is_ignored(&self) -> bool {
        self.label_bool("leandock.ignore").unwrap_or(false)
    }

    /// Returns true if the service already defines a memory limit (v2 `mem_limit` or v3 `deploy.resources.limits.memory`).
    pub fn has_existing_memory_limit(&self) -> bool {
        self.has_mem_limit || self.has_deploy_mem_limit
    }

    /// Reads a boolean label value (`"true"`, `"1"`, `"yes"`).
    pub fn label_bool(&self, key: &str) -> Option<bool> {
        self.labels.get(key).map(|v| {
            let lower = v.trim().to_lowercase();
            matches!(lower.as_str(), "true" | "1" | "yes" | "on")
        })
    }

    /// Reads a string label value.
    pub fn label(&self, key: &str) -> Option<&str> {
        self.labels.get(key).map(|s| s.as_str())
    }
}

/// Parsed Docker Compose document preserving raw YAML structure without mutating source files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedCompose {
    pub services: BTreeMap<String, ParsedService>,
}

impl ParsedCompose {
    /// Loads and parses a Compose YAML file from disk.
    pub fn from_file(path: &Path) -> Result<Self> {
        let raw = fs::read_to_string(path).map_err(|source| LeanDockError::IoPath {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_yaml_str(&raw, Some(path))
    }

    /// Parses a Compose YAML document from a string.
    pub fn from_yaml_str(raw: &str, path_hint: Option<&Path>) -> Result<Self> {
        let doc: Value = serde_yaml_ng::from_str(raw).map_err(|source| {
            if let Some(p) = path_hint {
                LeanDockError::ComposeParse {
                    path: p.to_path_buf(),
                    source,
                }
            } else {
                LeanDockError::Yaml(source)
            }
        })?;

        let root_map = doc.as_mapping().ok_or_else(|| {
            LeanDockError::ComposeSchema("Top-level Compose document must be a YAML mapping".into())
        })?;

        let services_val = root_map
            .get(Value::String("services".into()))
            .and_then(Value::as_mapping)
            .ok_or_else(|| {
                LeanDockError::ComposeSchema("Compose file must contain a `services:` mapping".into())
            })?;

        let mut services = BTreeMap::new();
        for (k, v) in services_val {
            let Some(name) = k.as_str() else {
                continue;
            };
            let Some(svc_map) = v.as_mapping() else {
                continue;
            };

            let image = svc_map
                .get(Value::String("image".into()))
                .and_then(Value::as_str)
                .map(|s| s.to_string());
            let has_build = svc_map.contains_key(Value::String("build".into()));
            let command = parse_command(svc_map.get(Value::String("command".into())));
            let labels = parse_labels(svc_map.get(Value::String("labels".into())));
            let environment = parse_environment(svc_map.get(Value::String("environment".into())));
            let volumes = parse_string_list(svc_map.get(Value::String("volumes".into())));
            let ports = parse_string_list(svc_map.get(Value::String("ports".into())));

            let has_mem_limit = svc_map.contains_key(Value::String("mem_limit".into()));
            let has_deploy_mem_limit = svc_map
                .get(Value::String("deploy".into()))
                .and_then(Value::as_mapping)
                .and_then(|d| d.get(Value::String("resources".into())))
                .and_then(Value::as_mapping)
                .and_then(|r| r.get(Value::String("limits".into())))
                .and_then(Value::as_mapping)
                .map(|l| l.contains_key(Value::String("memory".into())))
                .unwrap_or(false);
            let has_logging = svc_map.contains_key(Value::String("logging".into()));
            let has_swarm_keys = svc_map
                .get(Value::String("deploy".into()))
                .and_then(Value::as_mapping)
                .map(|d| {
                    d.contains_key(Value::String("placement".into()))
                        || d.contains_key(Value::String("update_config".into()))
                        || d.contains_key(Value::String("mode".into()))
                })
                .unwrap_or(false);
            let mounts_docker_sock = volumes
                .iter()
                .any(|vol| vol.contains("/var/run/docker.sock"));

            let db_kind = detect_database_kind(image.as_deref());
            let role = classify_service_role(name, image.as_deref(), db_kind, &labels);

            services.insert(
                name.to_string(),
                ParsedService {
                    name: name.to_string(),
                    image,
                    has_build,
                    command,
                    labels,
                    environment,
                    volumes,
                    ports,
                    has_mem_limit,
                    has_deploy_mem_limit,
                    has_logging,
                    has_swarm_keys,
                    mounts_docker_sock,
                    role,
                    db_kind,
                },
            );
        }

        Ok(Self { services })
    }
}

fn parse_command(val: Option<&Value>) -> ComposeCommand {
    match val {
        None | Some(Value::Null) => ComposeCommand::None,
        Some(Value::String(s)) => ComposeCommand::String(s.clone()),
        Some(Value::Sequence(seq)) => {
            let items = seq
                .iter()
                .filter_map(|item| match item {
                    Value::String(s) => Some(s.clone()),
                    Value::Number(n) => Some(n.to_string()),
                    Value::Bool(b) => Some(b.to_string()),
                    _ => None,
                })
                .collect();
            ComposeCommand::Array(items)
        }
        _ => ComposeCommand::None,
    }
}

/// Parses Compose labels supporting BOTH YAML mapping (`k: v`) and string-array (`"k=v"`) forms.
fn parse_labels(val: Option<&Value>) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    match val {
        Some(Value::Mapping(map)) => {
            for (k, v) in map {
                if let Some(key) = k.as_str() {
                    let value_str = value_to_scalar_string(v);
                    out.insert(key.trim().to_string(), value_str);
                }
            }
        }
        Some(Value::Sequence(seq)) => {
            for item in seq {
                if let Some(entry) = item.as_str() {
                    if let Some((k, v)) = entry.split_once('=') {
                        out.insert(k.trim().to_string(), v.trim().to_string());
                    } else {
                        out.insert(entry.trim().to_string(), "true".to_string());
                    }
                }
            }
        }
        _ => {}
    }
    out
}

fn parse_environment(val: Option<&Value>) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    match val {
        Some(Value::Mapping(map)) => {
            for (k, v) in map {
                if let Some(key) = k.as_str() {
                    out.insert(key.trim().to_string(), value_to_scalar_string(v));
                }
            }
        }
        Some(Value::Sequence(seq)) => {
            for item in seq {
                if let Some(entry) = item.as_str() {
                    if let Some((k, v)) = entry.split_once('=') {
                        out.insert(k.trim().to_string(), v.to_string());
                    }
                }
            }
        }
        _ => {}
    }
    out
}

fn parse_string_list(val: Option<&Value>) -> Vec<String> {
    match val {
        Some(Value::Sequence(seq)) => seq
            .iter()
            .filter_map(|item| match item {
                Value::String(s) => Some(s.clone()),
                Value::Mapping(m) => {
                    // Long syntax volume/port: capture source or target for inspection
                    m.get(Value::String("source".into()))
                        .and_then(Value::as_str)
                        .map(|s| s.to_string())
                }
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn value_to_scalar_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Null => String::new(),
        _ => String::new(),
    }
}

/// Identifies supported database engines from the image repository name.
pub fn detect_database_kind(image: Option<&str>) -> Option<DatabaseKind> {
    let img = image?.trim().to_lowercase();
    // Strip digest and tag, keeping registry/repo path.
    let without_digest = img.split('@').next().unwrap_or(&img);
    let repo = without_digest.split(':').next().unwrap_or(without_digest);
    let leaf = repo.rsplit('/').next().unwrap_or(repo);

    if leaf == "mongo" || leaf == "mongodb-community-server" {
        Some(DatabaseKind::MongoDb)
    } else if leaf == "postgres" || leaf == "postgis" || leaf == "timescaledb" || leaf == "pgvector" {
        Some(DatabaseKind::Postgres)
    } else if leaf == "redis" || leaf == "redis-stack" || leaf == "redis-stack-server" || leaf == "valkey" {
        Some(DatabaseKind::RedisOrValkey)
    } else if leaf == "mysql" || leaf == "mariadb" || leaf == "percona" {
        Some(DatabaseKind::MySqlOrMariaDb)
    } else if leaf == "qdrant" {
        Some(DatabaseKind::Qdrant)
    } else {
        None
    }
}

fn classify_service_role(
    name: &str,
    image: Option<&str>,
    db_kind: Option<DatabaseKind>,
    labels: &BTreeMap<String, String>,
) -> ServiceRole {
    if let Some(kind) = db_kind {
        return match kind {
            DatabaseKind::RedisOrValkey => ServiceRole::Cache,
            _ => ServiceRole::Database,
        };
    }

    if labels.contains_key("leandock.static_dir") {
        return ServiceRole::Frontend;
    }

    let lname = name.to_lowercase();
    let limage = image.unwrap_or("").to_lowercase();

    if lname.contains("frontend")
        || lname.contains("web")
        || lname.contains("ui")
        || lname.contains("nginx")
        || limage.contains("nginx")
        || limage.contains("caddy")
        || limage.contains("httpd")
    {
        return ServiceRole::Frontend;
    }

    if lname.contains("worker")
        || lname.contains("celery")
        || lname.contains("queue")
        || lname.contains("cron")
        || lname.contains("scheduler")
        || lname.contains("ocr")
        || lname.contains("scraper")
    {
        return ServiceRole::Worker;
    }

    ServiceRole::App
}
