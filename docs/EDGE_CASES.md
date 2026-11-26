# Edge Cases, Platform Quirks & Safety Guarantees

LeanDock is engineered around **Graceful Degradation Over Clever Breakage**. This document explains how every tricky real-world edge case is handled and verified in `tests/`.

---

## 1. Per-Service Escape Hatch (`leandock.ignore=true`)
- Checked in **both** YAML map labels (`leandock.ignore: "true"` / `true`) and string-array labels (`"leandock.ignore=true"`).
- Ignored services are omitted completely from `.leandock/compose.override.yml`, skipped by `--extract-static` and `--minify`, and ignored by `leandock proxy` discovery.
- Verified in `tests/db_tuner_tests.rs` and `tests/proxy_singleflight_tests.rs`.

## 2. Non-Root Execution on Linux
- Running `leandock up` as a normal user (e.g., member of the `docker` group) **never crashes or demands `sudo`**.
- Tier 3 (`/etc/docker/daemon.json`) and Tier 4 (`zram` / `KSM`) are skipped cleanly with a single explanatory message.
- Tier 1 (Compose memory caps, DB cache tuning, container-level `logging:` rotation, post-deploy cleanup) and Tier 2 (Wake-on-Request proxy) remain 100% active.

## 3. Restricted Container VPS (OpenVZ / Unprivileged LXC / WSL2)
- `src/profiler/platform.rs` detects OpenVZ (`/proc/user_beancounters`), LXC (`systemd-detect-virt`, `/proc/1/environ`), and WSL2 (`microsoft`/`wsl` kernel release).
- Because these environments share a host kernel with read-only `/sys` or blocked `modprobe`, LeanDock automatically skips `zram` and `KSM` without aborting `leandock up`.
- Verified in `tests/platform_tier_tests.rs`.

## 4. Windows & macOS Compilation and Runtime Safety
- All Linux-only code is gated behind `#[cfg(target_os = "linux")]`; Windows code behind `#[cfg(target_os = "windows")]`; macOS code behind `#[cfg(target_os = "macos")]`.
- `nix` is placed strictly under `[target.'cfg(unix)'.dependencies]`.
- On Windows, `leandock proxy` uses `CliContainerEngine` (async `docker` / `podman` subprocess calls) rather than brittle Named Pipe HTTP transports.
- On Windows/WSL2, LeanDock warns explicitly if the repository sits on `/mnt/c/...` or Windows NTFS paths where bind-mount I/O degrades sharply.

## 5. Custom Database Commands & Shell Wrappers
- **Array commands** (e.g., `["mongod", "--replSet", "rs0"]`): LeanDock preserves every existing argument, deduplicates flags the user already set, and appends only missing memory caps.
- **Ambiguous shell commands** (e.g., `sh -c "exec postgres ..."` or commands containing `&&`, `||`, `;`, `|`, `$`): LeanDock refuses to mutate the command and logs a clear warning instead of risking syntax corruption.
- Verified in `tests/db_tuner_tests.rs`.

## 6. Compose YAML Complexities
- **Interpolation (`${VAR:-default}`):** Because the original Compose file is never rewritten, environment interpolation expressions are guaranteed 100% intact.
- **Compose v2 vs v3 limits:** Both `mem_limit` and `deploy.resources.limits.memory` are detected and respected without overwriting.
- **`build:`-only services:** Supported seamlessly without requiring an `image:` field.
- Verified in `tests/compose_override_tests.rs`.

## 7. Ultra-Low RAM VPS (512 MB / 1 GB)
- Hard safety floors prevent databases from receiving invalid `0 MB` flags (MongoDB `>= 0.25 GB`, PostgreSQL `shared_buffers >= 64MB`, Redis `>= 32mb`, MySQL `>= 64M`).
- If the sum of minimum safe service floors exceeds available RAM + swap, LeanDock prints an explicit recommendation to enable `zram` or Wake-on-Request (`leandock.idle.enable=true`).

## 8. Concurrent Wake-on-Request Stampede
- When 50 simultaneous HTTP requests arrive for a sleeping container, `WakeManager::ensure_awake` uses a `Mutex<WakePhase>` + `tokio::sync::Notify` singleflight latch.
- Request #1 transitions `Sleeping -> Waking`, calls `engine.start_container()` once, and polls TCP readiness.
- Requests #2–50 await `Notify::notified()`. Once ready, all 50 proceed; on timeout, all receive HTTP `503 Service Unavailable` with `Retry-After: 5` and state resets cleanly to `Sleeping`.
- Verified with a 50-task concurrency test in `tests/proxy_singleflight_tests.rs`.

## 9. Post-Deploy Crash-Loop Auto-Rollback
- After `leandock up`, `health_guard.rs` watches all project containers for 10 seconds (configurable via `--health-window`).
- If any container enters `restarting`, `dead`, `unhealthy`, or exits non-zero, LeanDock prints the last 30 lines of container logs and automatically runs `<compose> -f <original> up -d --force-recreate` so your service never stays broken.

## 10. Atomic Config Writes & Instant `leandock revert`
- Every host file write uses a sibling temp file + atomic `rename()` after backing up the original into a timestamped snapshot directory with `manifest.json`.
- `leandock revert` restores every modified file, removes newly created drop-ins/sysctl files, reloads affected systemd services, and cleans `.leandock/`.
