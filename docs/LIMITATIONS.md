# Known Limitations

LeanDock prefers to skip an optimization and say why rather than risk a running stack. This page lists
what is intentionally unsupported, what depends on your host, and where the safety boundaries are.
If something here blocks you, open a
[feature request](https://github.com/pvbang/leandock/issues/new?assignees=&labels=enhancement&template=feature_request.yml)
or check the [roadmap](ROADMAP.md).

## Platforms

| Environment | What works | What does not |
|---|---|---|
| Linux KVM / bare metal (x86_64, aarch64) | All four tiers | Kernel tuning needs root and the prerequisites below |
| OpenVZ / unprivileged LXC | Compose/DB tuning, proxy | `zram`, `KSM`, and `daemon.json` writes are skipped (read-only `/sys`, shared kernel) |
| Windows 10/11 (WSL2) | Compose/DB tuning, `.wslconfig` tuning, proxy | Linux kernel tier is skipped; **native Windows containers** are not supported |
| macOS (Intel / Apple Silicon) | Compose/DB tuning, proxy | No host tuning — LeanDock reports VM advice only |
| FreeBSD, other targets | — | No official binaries |

## Linux kernel tier (`zram`, `KSM`)

Tier 4 requires **all** of the following; otherwise it is skipped with an explanation:

- Kernel **>= 6.1** and systemd **>= 254** (for the `MemoryKSM=yes` drop-in).
- Root privileges and a writable `/sys/kernel/mm/ksm/run`.
- A non-shared virtualization layer (KVM, bare metal, Xen, VMware). LXC/OpenVZ are excluded.
- A kernel that allows loading `zram` (`modprobe zram`).

`leandock doctor` prints the exact prerequisite that failed.

## Compose and database tuning

- Tuned engines today: **MongoDB, PostgreSQL, Redis/Valkey, MySQL/MariaDB, Qdrant**. Other services still
  receive memory limits, log rotation, and cleanup, but no engine-specific flags.
- Services whose `command:` is a shell wrapper (`sh -c "..."`, pipes, `&&`, `;`, `$`) are **skipped on
  purpose**. Convert the command to YAML array form to enable tuning.
- Redis/Valkey persistence is preserved by default (`noeviction`). Ephemeral mode
  (`--ephemeral-cache` or `leandock.redis.ephemeral=true`) disables RDB/AOF writes — only use it for pure
  caches.
- Original Compose files are never edited; all changes live in `.leandock/compose.override.yml`. If your
  workflow already passes its own override files, verify the merge order.

## Wake-on-Request proxy

- Only services with `leandock.idle.enable=true` and the required `leandock.idle.*` labels are managed.
- The proxy speaks **HTTP/1.1 only**. WebSocket upgrades, HTTP/2, gRPC, and raw TCP are not proxied.
- There is **no TLS termination**. Put a real reverse proxy (Caddy, Nginx, Traefik) in front if you need
  HTTPS.
- One public listen port per service; there is no path-based routing or load balancing across replicas.
- Idle stopping requires the proxy process to stay alive (`leandock proxy` or
  `leandock proxy --install-service` on Linux). If the proxy stops, containers stay in their last state.
- A wake that exceeds `leandock.idle.wake_timeout` returns `503` with `Retry-After: 5`; clients must
  tolerate that.
- Scale-to-zero trades cold-start latency for RAM. Do not use it for latency-critical services.

## Static offload and minification

- `--extract-static` only serves pre-built static assets from the directory declared in
  `leandock.static_dir`. It does not run server-side rendering or proxy API calls.
- SPA fallback to `index.html` is opt-in via `leandock.spa=true`.
- `--minify` requires [SlimToolkit](https://github.com/slimtoolkit/slim) (`slim`) on `PATH`. It is opt-in,
  each image is verified after startup, and LeanDock falls back to the original image on failure — but
  image minification is inherently risky for unusual runtimes.

## Cleanup and rollback guarantees

- Cleanup runs **only after** the health guard passes and prunes strictly dangling (`<none>`) images and
  unused build cache. Volumes, bind mounts, and tagged images are never removed.
- The health guard watches a stability window (default **10s**). Slow starters may need
  `--health-window 30`.
- `leandock revert` restores snapshotted host files and removes `.leandock/`. It cannot undo changes made
  by your own Compose file or by Docker itself.

## Benchmarks

`leandock bench` measures your machine and your stack. Numbers depend on images, caches, disk speed, and
settle time, so they are **not** a promise of savings on different hardware. Publish the JSON output if
you want others to reproduce a result.

## Packaging

- Windows **ARM64** binaries are not built yet.
- Homebrew and Scoop channels are not published; use the install script, npm, PyPI, crates.io, or the
  release archives.
- Install scripts support Linux and macOS on `x86_64`/`aarch64`, and Windows on `x86_64`.

## Not a fit

Kubernetes, Docker Swarm, Nomad, multi-node scheduling, CRIU checkpoint/restore, and deleting user data
are explicitly out of scope. See [When NOT to Use LeanDock](../README.md#when-not-to-use-leandock).
