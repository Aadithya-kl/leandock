# LeanDock Optimization Math & Mechanics

Every formula in LeanDock is deterministic, inspectable via `leandock analyze`, and bounded by hard safety floors so small VPS instances never receive invalid `0 MB` settings.

---

## 1. Dynamic Memory Budgeting (`src/prerun/db_tuner.rs`)

Given effective RAM $R_{\text{eff}}$ (on Linux: host RAM; on Windows/macOS: detected container VM RAM or conservative $\text{clamp}(R_{\text{host}}/2, 1024, 8192)$ MB):

1. **OS Headroom Reservation:**
   $$R_{\text{os}} = \max\left(384\text{ MB},\ \text{round}(0.15 \times R_{\text{eff}})\right)$$
2. **Allocatable Container Pool:**
   $$R_{\text{pool}} = \max\left(256\text{ MB},\ R_{\text{eff}} - R_{\text{os}}\right)$$
3. **Role Weights:**
   - MongoDB / MySQL / MariaDB: weight `5`
   - PostgreSQL / Qdrant: weight `4`
   - App / Worker: weight `3`
   - Cache (Redis / Valkey): weight `2`
   - Frontend (Nginx / Caddy): weight `1`
4. **Per-Service Budget:**
   $$B_i = \text{clamp}\left(\left\lfloor R_{\text{pool}} \cdot \frac{w_i}{\sum w_j} \right\rfloor,\ \text{floor}_i,\ \text{ceiling}_i\right)$$
   - Reservation: `mem_reservation = max(32 MB, round(0.45 * B_i))`
   - Services that already define Compose v2 `mem_limit` or v3 `deploy.resources.limits.memory` keep their user-defined limits untouched.

---

## 2. Database Engine Auto-Tuning Formulas

| Engine | Injected Flags / Env | Formula & Hard Floor | Safety Guarantees |
|---|---|---|---|
| **MongoDB** (`mongo:*`) | `--wiredTigerCacheSizeGB <val>` | $\max(0.25,\ \text{round}_2(0.55 \cdot B_i / 1024))$ | Preserves existing array args (e.g., `--replSet rs0`); skips if already set or shell string. |
| **PostgreSQL** (`postgres:*`) | `-c shared_buffers=... -c effective_cache_size=... -c work_mem=... -c max_connections=...` | `shared_buffers = max(64MB, 25% B_i)`<br>`effective_cache_size = max(128MB, 60% B_i)`<br>`work_mem = clamp(B_i / 128, 2MB, 16MB)`<br>`max_connections = 30 (<=1GB) / 50 (<=2GB) / 100` | Checks each `-c key=` individually to prevent duplicate flags. |
| **Redis / Valkey** (`redis:*`, `valkey/valkey:*`) | `--maxmemory <val>mb --maxmemory-policy ...` | `maxmemory = max(32mb, 75% B_i)` | **Persistence-safe default:** uses `noeviction` and leaves RDB/AOF untouched. Injects `allkeys-lru` + `--save ""` ONLY if `--ephemeral-cache` or `leandock.redis.ephemeral=true`. |
| **MySQL / MariaDB** (`mysql:*`, `mariadb:*`) | `--innodb-buffer-pool-size=<val>M` (`--performance-schema=OFF` when $R_{\text{eff}} \le 2048$ MB) | `pool = max(64M, 50% B_i)` | Matches both dash and underscore flag forms before appending. |
| **Qdrant** (`qdrant/qdrant:*`) | `QDRANT__STORAGE__PERFORMANCE__MAX_OPTIMIZATION_THREADS=1` | Applied on hosts with $\le 2$ logical cores | Injected via environment variable only; respects user-set env vars. |

---

## 3. Linux Kernel Memory Mechanics (`src/postrun/kernel_opt.rs`)

### Automated `zram` (`zstd`)
- **Size:** `100%` of RAM on hosts $\le 2\text{ GB}$, `50%` of RAM on larger hosts, clamped to `[512 MB, 8192 MB]`.
- **Compression:** `zstd` (typically 2.5x–3.5x compression on container memory pages).
- **Sysctl Profile (`/etc/sysctl.d/99-leandock.conf`):**
  - `vm.swappiness = 120` (prefer compressing cold anonymous pages in RAM over thrashing clean file cache)
  - `vm.page-cluster = 0` (eliminate swap read-ahead overhead since zram has zero seek latency)
  - `vm.watermark_boost_factor = 0` & `vm.watermark_scale_factor = 125` (smooth background reclamation without OOM stalls)

### Real `KSM` (Kernel Samepage Merging)
Historically, enabling `/sys/kernel/mm/ksm/run = 1` did almost nothing for Docker containers because Linux only merged pages marked with `MADV_MERGEABLE`. Since **Linux kernel 6.1** (`PR_SET_MEMORY_MERGE`) and **systemd 254** (`MemoryKSM=yes`), container runtimes can genuinely deduplicate identical anonymous pages across containers (e.g., multiple Python/Node/JVM runtimes).

LeanDock verifies **all four prerequisites** before enabling KSM:
1. Linux kernel $\ge 6.1$
2. `systemd` $\ge 254$
3. Non-restricted virtualization (KVM / bare-metal / VMware / Xen) and root privileges
4. Writable `/sys/kernel/mm/ksm/run`

When enabled, LeanDock sets conservative scan rates (`pages_to_scan = 200`, `sleep_millisecs = 200` to keep `ksmd` CPU usage near 0%), persists them via `/etc/tmpfiles.d/99-leandock-ksm.conf`, and installs `MemoryKSM=yes` drop-ins for `docker.service` and `containerd.service`.

---

## 4. Host Runtime Optimizations
- **Linux `daemon.json`:** Disables `userland-proxy` (eliminating per-port `docker-proxy` processes in favor of kernel iptables/nftables hairpin NAT), enables `live-restore: true`, and sets default `json-file` log rotation (`10m` x `3`).
- **Daemonless Podman (`leandock quadlet`):** Generates native systemd `.container` units (or delegates to `podlet`), eliminating the ~140–180 MB `dockerd` background daemon completely.
- **Windows `.wslconfig`:** Safely merges `memory=<~38% RAM, clamped 2-16GB>`, `swap=2GB`, `autoMemoryReclaim=gradual`, and `sparseVhd=true` without erasing existing user settings.
