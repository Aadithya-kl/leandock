# Project Specification & Build Prompt: LeanDock (`leandock`) — v2

You are a Principal Systems Engineer, Rust Specialist, and Cross-Platform Infrastructure Architect. Your task is to design, implement, test, and document a production-grade, open-source CLI tool named **LeanDock** (`leandock`), written in **Rust**, ready to be published publicly on GitHub.

---

## 1. Origin Story, Mission & Core Philosophy (The "Soul" of LeanDock)

Before writing a single line of code or documentation, internalize **why LeanDock exists**. Every architectural decision, CLI message, and README paragraph must reflect this identity:

### Why It Was Born
Docker Compose solved the hardest workflow problem in modern development: you can clone a repository onto a brand-new server or laptop, run `docker compose up -d`, and have your entire stack (databases, APIs, queues, workers, frontend) running cleanly in seconds.

**The painful catch:** Running that convenient stack with default settings wastes massive amounts of RAM, CPU, and disk space — especially on budget VPS instances (1 GB–8 GB RAM) and developer laptops:
- On **Linux VPS**, Docker containers are already native processes (no VM), yet defaults still waste resources heavily: `dockerd` eats ~140–180 MB idle RAM, `docker-proxy` spawns extra userland processes per port, databases inside containers greedily reserve huge host-proportional memory caches (e.g., MongoDB WiredTiger grabbing 50% of `RAM - 1GB`), JSON logs grow without bound, leftover build layers eat gigabytes of SSD, modern Linux kernel memory features (`zram`, `KSM`) sit unused, and low-traffic services burn RAM 24/7 while idle.
- On **Windows (WSL2) & macOS**, developers suffer double overhead: a Linux VM layer (often holding 50% of host RAM via `VmmemWSL` without reclaiming it) stacked on top of greedy default database containers and bloated build caches.

Developers shouldn't have to abandon Docker Compose, manually rewrite fragile native install scripts, or become Linux kernel tuning experts just to run a clean, lightweight stack.

### Mission Statement
**LeanDock is a drop-in, zero-config, non-destructive optimizer wrapper for Docker Compose.**
Clone your existing Dockerized project anywhere, run `leandock up` instead of `docker compose up -d`, and get the **exact same working application** with dramatically lower RAM, disk, and background overhead — automatically tailored to the exact OS, CPU architecture, and hardware it runs on.

### The Three Non-Negotiable Oaths (Core Philosophy)
1. **Zero Workflow Friction:** Never force users to abandon Docker, rewrite their `Dockerfile`, or learn a proprietary manifest format. Their existing `docker-compose.yml` remains the single source of truth.
2. **Non-Destructive & 100% Reversible:** Never edit the user's original Compose files in place. Always layer optimizations via an isolated override file (`.leandock/compose.override.yml`) and snapshot every host-level config before touching it, guaranteeing one-command full rollback (`leandock revert`).
3. **Graceful Degradation Over Clever Breakage:** *It is ALWAYS better to safely skip an optimization with a clear explanation than to risk breaking a running application.* If an OS, virtualization layer, kernel version, or custom service configuration does not safely support a specific optimization, LeanDock skips it cleanly and keeps the stack healthy.

---

## 2. Target Use Cases & Scope (Must Be Documented Clearly in `README.md`)

LeanDock must be brutally honest with GitHub users about where it shines and where it should NOT be used:

### Ideal Use Cases (The Sweet Spot)
- **Full-Stack Web / SaaS / MVP Projects on Small VPS (1 GB – 8 GB RAM):** Stacks combining Frontend + Backend API (FastAPI, Node, Go, Laravel, etc.) + 1–2 Databases (MongoDB, PostgreSQL, Redis, MySQL, Qdrant).
- **Multi-Project VPS Consolidation (Side Projects, Demos, Staging, Portfolios):** Packing 5–10 projects onto a single 4 GB VPS by combining database memory caps with **Wake-on-Request (Scale-to-Zero)** for idle projects.
- **AI, Automation, Bot & Internal Tooling Stacks:** Systems with heavy on-demand workers (OCR, PDF generation, media processing, scrapers) or vector databases that sit idle most of the day.
- **Solo Developers & Small Teams Without Dedicated DevOps:** Instant production hygiene on any fresh VPS (log rotation, OOM protection, post-build cleanup, zram) in a single command.
- **Local Development Machines (Windows WSL2 / macOS):** Bounding runaway local database RAM, optimizing `.wslconfig`, and auto-reclaiming disk bloat.

### When NOT to Use LeanDock (Explicit Anti-Goals)
- **Large Multi-Node Orchestrators (Kubernetes, Docker Swarm, Nomad):** Those platforms already have dedicated resource scheduling, autoscaling, and CNI networking.
- **Ultra-Low-Latency / High-Throughput Production Systems:** Real-time trading engines, real-time game servers, or heavily saturated primary databases sustaining thousands of writes/sec should NOT use Wake-on-Request cold starts or automatic cache capping without manual DBA benchmarking.
- **Pure Windows Native Containers** (legacy `.NET Framework` Windows-kernel containers).
- **Sensitive/Special Individual Services:** Users can exclude any specific container from ALL LeanDock interventions by adding a single label in their Compose file: `leandock.ignore=true`.

---

## 3. Cross-Platform Tiered Capability Architecture

LeanDock MUST compile cleanly and run smoothly without crashes across **Linux, Windows, and macOS** on both **x86_64 and ARM64 (aarch64)** architectures.

Because OS-level capabilities vary fundamentally, LeanDock implements a **Tiered Capability Profiler** that probes the host at runtime and enables ONLY the supported tiers:

| Environment / Platform | Tier 1: Compose & DB Auto-Tuner + Log Cap + Cleanup | Tier 2: Wake-on-Request Proxy & Static Offload | Tier 3: Host Runtime & OS Tuning | Tier 4: Linux Kernel Optimizations (`zram`, `KSM`, `Quadlet`) |
|---|---|---|---|---|
| **Linux VPS (KVM / Bare-metal / Xen / VMware)** *(Ubuntu, Debian, Rocky, Alpine, Arch — x86_64 & ARM64)* | Full | Full (via Unix Socket) | Full (`daemon.json`: disable `userland-proxy`, log rotation, `live-restore`) | Full (`zram` zstd, `KSM` on kernel >= 6.1 + systemd >= 254, optional Podman Quadlet) |
| **Linux Container VPS (OpenVZ / Unprivileged LXC)** | Full | Full (via Unix Socket) | Partial (`daemon.json` if root) | Auto-skipped cleanly (read-only `/sys` / no `modprobe`) |
| **Windows 10/11 (WSL2 + Docker Desktop / Rancher / Native WSL Docker)** | Full | Full (via CLI/HTTP adapter) | **Windows-Specific:** Safe `%UserProfile%\.wslconfig` analyzer & optimizer (`memory`, `swap`, `autoMemoryReclaim=gradual`, `sparseVhd=true`) + `/mnt/c` I/O warning | Auto-skipped cleanly (with guidance if running inside WSL2) |
| **macOS (Apple Silicon ARM64 & Intel x86_64)** *(Docker Desktop, OrbStack, Colima, Lima)* | Full | Full (via Unix Socket / Colima socket detection) | Detects active container VM runtime & reports VM RAM guidance | Auto-skipped cleanly |

### Mandatory Cross-Compilation Rules in Rust
To guarantee first-try compilation across Linux, Windows, and macOS:
- Gate ALL Linux-only modules (`zram`, `KSM`, `systemd`, `daemon.json` Linux paths, `/proc` parsers) strictly with `#[cfg(target_os = "linux")]`.
- Gate Windows-specific `.wslconfig` handling strictly with `#[cfg(target_os = "windows")]`.
- Gate `nix` or Unix-only crates strictly under `[target.'cfg(unix)'.dependencies]` in `Cargo.toml`.
- Abstract container lifecycle operations behind a cross-platform `ContainerEngine` trait:
  - On Unix (`Linux` / `macOS`): Direct lightweight HTTP-over-Unix-Socket client (`/var/run/docker.sock`, `$DOCKER_HOST`, Podman socket, Colima socket) with zero heavy external SDKs.
  - On Windows (or fallback when socket is inaccessible): Clean async wrapper over `docker` / `podman` CLI or TCP `DOCKER_HOST` so `leandock proxy` works identically on Windows without fragile Named Pipe Hyper hacks.
- Never hardcode `x86_64` architecture assumptions when invoking external helpers (`slim`, `podlet`, `install.sh`).

---

## 4. Strict Engineering Honesty & Anti-Breakage Safeguards

You MUST enforce every one of these rules:

1. **Per-Service Escape Hatch (`leandock.ignore=true`):** Before tuning ANY service, check its `labels` (both map format `leandock.ignore: "true"` and list format `"leandock.ignore=true"`). If set, skip that service completely across DB tuning, memory caps, static offloading, minification, and proxy control.
2. **Non-Destructive Override:** Generate `.leandock/compose.override.yml` and execute via `<compose-cmd> -f <original> -f .leandock/compose.override.yml ...`.
3. **Transactional Host Backups & Instant Rollback:** Every host modification (`/etc/docker/daemon.json`, `/etc/sysctl.d/99-leandock.conf`, systemd drop-ins, `%UserProfile%\.wslconfig`) MUST be atomically backed up to a state directory (`/var/lib/leandock/backups/<timestamp>/` on Linux root, or `~/.leandock/backups/<timestamp>/` for non-root/Windows/macOS) before writing via temp-file + atomic rename. `leandock revert` MUST cleanly restore every touched file and reload affected services.
4. **No Fragile CRIU or Blind Container Destruction:**
   - Wake-on-Request uses rock-solid `stop` / `start` container lifecycle management with singleflight request queuing — NEVER CRIU checkpoint/restore.
   - Static frontend offloading (`--extract-static`) and image minification (`--minify` via SlimToolkit) are **strictly opt-in** with automatic health-check verification and instant fallback on failure.
5. **Real Linux Kernel Mechanics (No Cargo-Cult Tweaks):**
   - **KSM:** Must verify Linux kernel >= 6.1 AND systemd >= 254, enable `/sys/kernel/mm/ksm/run = 1`, tune scan rate conservatively (`sleep_millisecs = 200`, `pages_to_scan = 200`), AND write a systemd drop-in (`MemoryKSM=yes`) for the container runtime slice so containers actually benefit via `PR_SET_MEMORY_MERGE`. Skip gracefully with an explanation if prerequisites aren't met.
   - **Redis/Valkey Data Safety:** NEVER silently inject `--save ""` or `allkeys-lru` on Redis by default (that could destroy persistent queues/jobs!). By default, only cap `--maxmemory` with safe policy (`noeviction` or user's existing policy). Only switch to ephemeral cache settings if user explicitly passes `--ephemeral-cache` or label `leandock.redis.ephemeral=true`.
6. **Zero Placeholders:** Every module, subcommand, error path, and test must be 100% implemented and compilable. No `todo!()`, `unimplemented!()`, or placeholder comments.

---

## 5. Detailed Architecture & Feature Modules

### Phase 0: Pre-Flight Hardware, OS & Runtime Profiler (`src/profiler/`)
- Detect OS (`linux`, `windows`, `macos`), CPU architecture (`x86_64`, `aarch64`), logical cores, total/available RAM, swap, and free disk space.
- On Linux: detect kernel version, distro (`/etc/os-release`), systemd version, virtualization type (`kvm`, `bare-metal`, `vmware`, `xen`, `lxc`, `openvz`, `wsl`), and root/sudo capability.
- Detect available container runtimes & sockets: `docker compose` (v2), `docker-compose` (v1), `podman` + `podman compose`, `podlet`, `slim` (SlimToolkit), Colima/OrbStack sockets on macOS.
- Auto-discover Compose files (`compose.yaml`, `compose.yml`, `docker-compose.yaml`, `docker-compose.yml`, or explicit `-f`).

### Phase 1: Pre-Run Interceptor (`src/prerun/`)

#### Module 1.1: Host & Platform Runtime Tuner (`host_tuner/`)
- **Linux — Docker Engine (`linux_docker.rs`):** Safely deep-merge into `/etc/docker/daemon.json` (preserving ALL existing keys):
  - `"userland-proxy": false`
  - `"log-driver": "json-file"` with `"log-opts": { "max-size": "10m", "max-file": "3" }`
  - `"live-restore": true`
  - Validate JSON before reloading `docker.service`; auto-restore backup if reload fails.
- **Linux — Daemonless Podman Mode (`podman_quadlet.rs`, `--engine podman`):**
  - Pre-flight compatibility scanner warns about Docker-only features (e.g., `/var/run/docker.sock` mount, Swarm-only keys).
  - Supports running via `podman compose` or generating native systemd Quadlet units (`leandock quadlet`) to eliminate the ~140–180 MB `dockerd` background daemon completely.
- **Windows — WSL2 Optimizer (`windows_wsl.rs`):**
  - Inspects `%UserProfile%\.wslconfig` (creates or safely merges INI sections after backup).
  - Recommends/applies sane `memory=` cap (e.g., 35–40% of physical RAM, min 2GB), `swap=0` (or bounded), `autoMemoryReclaim=gradual`, and `sparseVhd=true`.
  - Warns if the current project directory resides on Windows NTFS (`/mnt/c/...` or `C:\...` mounted into WSL2) causing severe I/O slowdown.
- **macOS — Runtime Inspector (`macos_runtime.rs`):**
  - Detects Docker Desktop vs. OrbStack vs. Colima/Lima and displays actionable VM memory tips without touching macOS system internals.

#### Module 1.2: Hardware-Aware Compose & Database Auto-Tuner (`compose_tuner.rs`, `db_tuner.rs`)
Parses the Compose file safely (preserving `${VAR}` / `${VAR:-default}` expressions) and emits `.leandock/compose.override.yml`:
- **Dynamic Memory Budgeting:**
  - Safe OS headroom: `os_reserve_mb = max(384, total_ram_mb * 0.15)` (on Windows/macOS, budgets against container VM RAM if detectable, or conservative fraction of host RAM).
  - Classify services: `databases`, `caches`, `apps`, `frontends`, `workers`, skipping any service with `leandock.ignore=true`.
  - Compute `mem_limit` and `mem_reservation` ONLY for services without existing `mem_limit` or `deploy.resources.limits.memory`.
  - Inject container-level log rotation (`json-file`, `max-size: 10m`, `max-file: 3`) as a universal safety net across all OSes and non-root setups.
- **Safe Database Flag Tuning** (ONLY for recognized images without `leandock.ignore=true` and without conflicting shell-string `command`):
  - **MongoDB (`mongo:*`):** Append `--wiredTigerCacheSizeGB <val>` (floor `0.25`, scaled ~50–60% of container RAM budget), preserving existing args like `--replSet`.
  - **PostgreSQL (`postgres:*`):** Append `-c shared_buffers=<val>MB -c effective_cache_size=<val>MB -c work_mem=<val>MB -c max_connections=<val>` (floor `shared_buffers=64MB`).
  - **Redis / Valkey (`redis:*`, `valkey/valkey:*`):** Append `--maxmemory <val>mb` (preserve persistence by default; apply `allkeys-lru` + `--save ""` ONLY if `leandock.redis.ephemeral=true` or `--ephemeral-cache`).
  - **MySQL / MariaDB (`mysql:*`, `mariadb:*`):** Append `--innodb-buffer-pool-size=<val>M` (+ `--performance-schema=OFF` when host RAM <= 2 GB).
  - **Qdrant (`qdrant/qdrant:*`):** Inject safe environment variable bounds (`QDRANT__STORAGE__PERFORMANCE__MAX_OPTIMIZATION_THREADS=1` on <= 2-core VPS) to prevent memory spikes during indexing.
- **Opt-In Static Frontend Offloading (`--extract-static` + label `leandock.static_dir=...`):**
  - Extracts static build output from labeled FE containers into `.leandock/static/<service>/`, disables the redundant FE container via override, and serves it via LeanDock's built-in Rust static server (gzip + SPA fallback via `leandock.spa=true`).
- **Opt-In Safe Image Minification (`--minify`):**
  - Requires `slim` (SlimToolkit) installed. Minifies non-DB app images, runs temporary container health/startup verification, and automatically reverts to the original image on any failure.

### Phase 2: Post-Run Optimizer (`src/postrun/`)

#### Module 2.1: Linux Kernel Memory Optimizer (`kernel_opt.rs`, `#[cfg(target_os = "linux")]`)
- **Automated `zram` (`zstd`):** Skips cleanly if `/dev/zram` already active or inside LXC/OpenVZ/WSL. Configures persistent zram device (50–100% of RAM, capped at 8 GB) + optimal `/etc/sysctl.d/99-leandock.conf` (`vm.swappiness=120`, `vm.page-cluster=0`, `vm.watermark_boost_factor=0`, `vm.watermark_scale_factor=125`).
- **Smart `KSM`:** Requires kernel >= 6.1, systemd >= 254, and writable `/sys/kernel/mm/ksm/run`. Sets conservative scan rates, writes persistent `tmpfiles.d`, and adds `MemoryKSM=yes` drop-in to the container service slice.

#### Module 2.2: Health Verification Guard & Auto-Rollback (`health_guard.rs`)
- Monitors all started containers for a configurable stability window (default 10s) checking Docker health status and restart counts.
- If any container enters crash-loop (`Restarting`) or exits non-zero, prints the last 30 lines of container logs and (when `--auto-rollback` is enabled, default true) automatically falls back to launching the unmodified original Compose file so the deployment never stays broken.

#### Module 2.3: Safe Post-Deploy Disk Cleaner (`cleaner.rs`)
- Runs ONLY after `health_guard` confirms all containers are healthy.
- Prunes dangling intermediate images and unused build cache while strictly protecting named volumes, bind mounts, and active images. Reports exact MB/GB reclaimed.

### Phase 3: Cross-Platform Wake-on-Request Reverse Proxy (`src/proxy/`)
- Activated via `leandock proxy` (or installed as a lightweight systemd user/system service on Linux).
- Discovers containers labeled with:
  - `leandock.idle.enable=true`
  - `leandock.idle.timeout=10m`
  - `leandock.idle.listen_port=8080`
  - `leandock.idle.target_port=3000`
  - `leandock.idle.wake_timeout=30s`
- **Async Tokio + Hyper v1 Implementation:**
  - Tracks last request timestamp per service; gracefully stops idle containers (`0 MB RAM`).
  - Incoming HTTP request to a sleeping container uses **per-service singleflight coordination** (`tokio::sync::Notify` / state machine) so N concurrent requests trigger exactly ONE container start.
  - Polls target TCP port readiness up to `wake_timeout`, then streams/forwards the held HTTP request seamlessly; returns clean `503 Service Unavailable` with `Retry-After` if wake fails or times out.
  - Also serves extracted static frontends when `--extract-static` is active.

### Phase 4: CLI UX, Dry-Run Analysis & Benchmarking (`src/cli/`, `src/report/`)
Build with `clap` (derive) + `comfy-table` + `owo-colors`:
- `leandock analyze [-f compose.yml]` — Zero-touch dry-run! Prints detected OS/hardware/virtualization tier, ignored services, exact proposed DB/memory flags, and estimated RAM/disk savings.
- `leandock up [-f compose.yml] [--engine docker|podman] [--no-kernel] [--no-cleanup] [--minify] [--extract-static] [--ephemeral-cache] [--auto-rollback=true] [--dry-run]`
- `leandock down [-f compose.yml]`
- `leandock proxy [--port-map ...]`
- `leandock quadlet [-f compose.yml]` (Linux Podman Quadlet exporter)
- `leandock status` — Live snapshot of container RAM/CPU, zram compression stats, KSM pages saved, and Wake-on-Request service states (Active / Sleeping).
- `leandock bench` — Captures reproducible Before vs. After resource metrics (RAM, Swap/zram, Disk, Process count, Startup time) outputting Table / Markdown / JSON.
- `leandock revert [--all]` — Restores backed-up host configs (`daemon.json`, `sysctl`, `zram`, `KSM`, `.wslconfig`) and cleans `.leandock/`.
- `leandock doctor` — Full environment diagnostic checking OS tier, virtualization quirks, permissions, socket health, and runtime versions.

---

## 6. Mandatory Edge Cases & Exception Handling Matrix

Explicitly implement AND write automated tests for ALL of the following:
1. **`leandock.ignore=true` Protection:** Works identically for both YAML map labels and string-array labels; guarantees zero mutations to ignored services.
2. **Non-Root Execution:** Never crashes without `sudo`; cleanly skips host/kernel tweaks with a single clear notice while still generating Compose overrides and deploying smoothly.
3. **Restricted Virtualization (OpenVZ / LXC / WSL2):** Detects virtualization reliably; skips `zram`/`KSM` without aborting `leandock up`.
4. **Windows & macOS Compilation + Runtime Safety:** Zero panics or Linux-path assumptions when running `analyze`, `up`, `doctor`, or `proxy` on Windows or macOS.
5. **Custom DB Commands & Entrypoints:** Appends flags cleanly to array commands (e.g., `["mongod", "--replSet", "rs0"]`), avoids duplicating flags the user already set, and safely skips ambiguous shell strings (`sh -c "..."`) with a warning.
6. **Compose YAML Complexities:** Handles Compose v2 (`mem_limit`) vs v3 (`deploy.resources.limits.memory`), `build:`-only services without `image:`, and `${VAR:-default}` syntax without mangling interpolation.
7. **Ultra-Low RAM VPS (512 MB / 1 GB):** Enforces hard safe minimum floors (e.g., MongoDB `0.25` GB, Postgres `64MB`) so databases never receive invalid `0MB` flags, and warns when stack requirements exceed available RAM + zram.
8. **Concurrent Wake-on-Request Stampede:** 50 simultaneous requests hitting a sleeping service trigger exactly ONE container `start` call; all 50 wait and receive valid responses once ready.
9. **Crash-Loop Auto-Rollback:** Detects failing containers post-deploy, dumps recent logs, and automatically restores raw Compose deployment.
10. **Atomic Writes & Signal Handling:** Temp-file + atomic rename for all config edits; graceful `SIGINT`/`SIGTERM` shutdown in CLI and proxy.

---

## 7. Technical Stack & Cross-Platform Crate Guidelines

Keep dependencies minimal, idiomatic, and cross-platform safe:
- **CLI & Output:** `clap` (derive), `owo-colors`, `comfy-table`, `indicatif`
- **Async & HTTP Proxy:** `tokio` (rt-multi-thread, net, process, sync, time, signal, macros), `hyper` (v1), `hyper-util`, `http-body-util`, `bytes`
- **Serialization & Parsing:** `serde`, `serde_json`, `serde_yaml_ng`, `humantime`
- **System Info:** `sysinfo` (cross-platform RAM/CPU/Disk) + lightweight direct `/proc` & `/sys` parsers under `#[cfg(target_os = "linux")]`
- **Unix-Only Crates:** Place `nix` strictly under `[target.'cfg(unix)'.dependencies]`
- **Logging & Errors:** `tracing`, `tracing-subscriber`, `anyhow`, `thiserror`
- **Testing:** `tempfile`, `assert_cmd`, `predicates`

---

## 8. Complete GitHub Repository Structure & Documentation Quality Bar

Generate the complete, publication-ready repository:

    leandock/
    ├── .github/
    │   └── workflows/
    │       ├── ci.yml                 # Matrix: ubuntu-latest, macos-latest, windows-latest (fmt, clippy -D warnings, test)
    │       └── release.yml            # Cross-compile binaries: linux-x86_64/arm64 (musl), macos-x86_64/arm64, windows-x86_64
    ├── Cargo.toml
    ├── LICENSE                        # MIT License
    ├── README.md                      # Compelling, honest open-source README (English)
    ├── CHANGELOG.md
    ├── CONTRIBUTING.md
    ├── scripts/
    │   ├── install.sh                 # Linux & macOS installer (arch detection + SHA256 verify)
    │   └── install.ps1                # Windows PowerShell installer
    ├── docs/
    │   ├── ARCHITECTURE.md            # Deep technical architecture, Mermaid diagrams, cross-platform traits
    │   ├── OPTIMIZATIONS.md           # Exact math/formulas for DB tuning, zram, KSM, WSL2, Podman, Proxy
    │   ├── EDGE_CASES.md              # LXC/OpenVZ, Windows/macOS tiers, custom DB commands, rollback guarantees
    │   └── BENCHMARKS.md              # Reproducible benchmark methodology using `leandock bench`
    ├── examples/
    │   ├── fullstack-mongo-api/       # Sample docker-compose.yml (Mongo + Redis + FastAPI + Nginx FE + ignored service)
    │   └── wake-on-request-demo/      # Sample docker-compose.yml with leandock.idle.* labels
    ├── src/
    │   ├── main.rs
    │   ├── lib.rs
    │   ├── error.rs
    │   ├── cli/mod.rs
    │   ├── profiler/
    │   │   ├── mod.rs
    │   │   ├── hardware.rs
    │   │   ├── platform.rs            # Tiered capability detection (Linux KVM/LXC, Windows/WSL2, macOS, ARM64)
    │   │   └── runtime.rs
    │   ├── prerun/
    │   │   ├── mod.rs
    │   │   ├── compose_parser.rs      # Parses YAML + handles `leandock.ignore=true`
    │   │   ├── db_tuner.rs            # Hardware-aware DB tuning & memory budgeting
    │   │   ├── host_tuner/
    │   │   │   ├── mod.rs
    │   │   │   ├── linux_docker.rs    # #[cfg(target_os = "linux")]
    │   │   │   ├── podman_quadlet.rs  # #[cfg(target_os = "linux")]
    │   │   │   ├── windows_wsl.rs     # #[cfg(target_os = "windows")]
    │   │   │   └── macos_runtime.rs   # #[cfg(target_os = "macos")]
    │   │   ├── static_offload.rs
    │   │   └── minifier.rs
    │   ├── postrun/
    │   │   ├── mod.rs
    │   │   ├── kernel_opt.rs          # #[cfg(target_os = "linux")] zram + KSM (kernel >= 6.1)
    │   │   ├── health_guard.rs        # Health verification & auto-rollback
    │   │   └── cleaner.rs
    │   ├── proxy/
    │   │   ├── mod.rs
    │   │   ├── engine_client.rs       # Cross-platform ContainerEngine trait (Unix Socket on Unix, CLI/HTTP on Windows)
    │   │   ├── wake_manager.rs        # Singleflight container wake & idle reaper
    │   │   └── server.rs              # Async Hyper v1 reverse proxy + static server
    │   ├── backup/mod.rs              # Atomic writes, snapshots, `leandock revert`
    │   └── report/mod.rs              # `analyze`, `status`, `bench`, `doctor` formatters
    └── tests/
        ├── db_tuner_tests.rs          # Custom commands, low RAM floors, `leandock.ignore=true`, Redis safety
        ├── compose_override_tests.rs  # Non-destructive merge & env interpolation preservation
        ├── platform_tier_tests.rs     # KVM vs LXC/OpenVZ vs Windows/macOS capability matrix
        └── proxy_singleflight_tests.rs# Concurrent wake-up stampede & timeout handling

### `README.md` Quality Bar (Crucial for GitHub Adoption)
Make `README.md` punchy, authoritative, and developer-trusted:
1. **Hero & Badges:** Clear one-line value proposition, Build/Release/License/Rust badges.
2. **Why LeanDock Exists:** Tell the real story concisely (the gap between `docker compose up` convenience and real-world VPS/laptop resource waste) + the 3 Core Oaths.
3. **What LeanDock Does — and What It Doesn't Do:** Honest engineering transparency (containers on Linux are already native processes; no fake magic, only real measurable wins).
4. **Supported Platforms & Capability Matrix:** Clear table covering Linux KVM, Linux LXC/OpenVZ, Windows (WSL2), macOS, and ARM64.
5. **When to Use (and When NOT to Use) LeanDock:** Concrete bullet lists + showing `leandock.ignore=true`.
6. **Architecture Flowchart (Mermaid):** Visual flow from `compose.yml` -> Profiler -> Pre-Run -> Deploy -> Health Guard -> Post-Run -> Wake Proxy.
7. **30-Second Quickstart:** Install (`install.sh` / `install.ps1`), `leandock analyze`, `leandock up`, `leandock bench`, `leandock revert`.
8. **Full CLI & Compose Label Reference Table.**

---

## 9. Execution Instructions for AI Studio

1. **Validate Cross-Platform Design First:** Briefly confirm exact crate versions in `Cargo.toml` and `#[cfg(...)]` boundaries so `cargo test` and `cargo clippy -- -D warnings` pass cleanly on Linux, macOS, and Windows.
2. **Write Complete, Compilable Rust Code:** Every `.rs` file must include full imports, idiomatic error handling (`Result<T, LeanDockError>`), and zero `todo!()` or omitted logic.
3. **Deliver All Docs, Scripts, Examples, CI Workflows, and Tests** listed in Section 8. If output approaches token limits, stop cleanly after completing the current file and ask the user to reply `"continue"` so nothing is ever truncated or summarized.