# Contributing to LeanDock

Thank you for your interest in contributing to **LeanDock**! LeanDock exists to make running Docker Compose lean, predictable, and safe across small VPS instances and developer laptops.

## Core Engineering Principles

Before opening a pull request, make sure your change honors all three LeanDock oaths:

1. **Zero Workflow Friction:** Never require users to modify their `Dockerfile` or replace `docker-compose.yml`.
2. **Non-Destructive & Reversible:** Any host file change must go through `BackupSession::backup_and_write` using atomic temp-file + rename so `leandock revert` always works cleanly.
3. **Graceful Degradation:** Never panic or abort `leandock up` when an optional host/kernel capability is unavailable (non-root, LXC/OpenVZ, older kernel, Windows, macOS).

## Cross-Platform Compilation Rules

CI runs `cargo clippy --all-targets -- -D warnings` and `cargo test --all-targets` on **Linux, macOS, and Windows**:

- Gate Linux-only code (`zram`, `KSM`, `systemd`, `/proc`, `/sys`, `daemon.json`) strictly with `#[cfg(target_os = "linux")]`.
- Gate Windows-only code (`.wslconfig`) strictly with `#[cfg(target_os = "windows")]`.
- Gate macOS-only code strictly with `#[cfg(target_os = "macos")]`.
- Keep Unix-only crates (`nix`) strictly under `[target.'cfg(unix)'.dependencies]` in `Cargo.toml`.

## Local Verification Checklist

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

## Pull Request Process

1. Fork the repository and create a focused branch (`feat/...`, `fix/...`, `docs/...`).
2. Add or update automated tests in `tests/` covering any new edge cases.
3. Document any new CLI flags or Compose labels in `README.md` and `docs/`.
4. Open a Pull Request describing the exact mechanism, safety invariants, and tested platforms.
