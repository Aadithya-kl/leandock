use std::path::PathBuf;
use thiserror::Error;

/// Unified error type for all LeanDock operations.
#[derive(Debug, Error)]
pub enum LeanDockError {
    #[error("Compose file not found in {0}. Pass an explicit path with `-f <file>`.")]
    ComposeFileNotFound(PathBuf),

    #[error("Failed to parse Compose YAML at {path}: {source}")]
    ComposeParse {
        path: PathBuf,
        #[source]
        source: serde_yaml_ng::Error,
    },

    #[error("Invalid Compose schema: {0}")]
    ComposeSchema(String),

    #[error("No supported container runtime (docker compose / podman compose) found on PATH.")]
    RuntimeNotFound,

    #[error("Container runtime command `{cmd}` failed (exit {code:?}): {stderr}")]
    CommandFailed {
        cmd: String,
        code: Option<i32>,
        stderr: String,
    },

    #[error("Container health verification failed: {0}")]
    HealthCheckFailed(String),

    #[error("Backup operation failed: {0}")]
    BackupFailed(String),

    #[error("Host tuning error: {0}")]
    HostTuner(String),

    #[error("Kernel optimization error: {0}")]
    KernelOpt(String),

    #[error("Proxy error: {0}")]
    Proxy(String),

    #[error("Container `{service}` timed out after {timeout_secs}s while waking up")]
    WakeTimeout { service: String, timeout_secs: u64 },

    #[error("External tool `{tool}` is required for this feature. {hint}")]
    MissingExternalTool { tool: String, hint: String },

    #[error("Invalid configuration value for `{field}`: {reason}")]
    InvalidConfig { field: String, reason: String },

    #[error("I/O error at {path}: {source}")]
    IoPath {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("YAML serialization error: {0}")]
    Yaml(#[from] serde_yaml_ng::Error),
}

pub type Result<T> = std::result::Result<T, LeanDockError>;
