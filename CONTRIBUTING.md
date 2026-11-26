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

## Where to Start

You do not need a Linux VPS to contribute to most of LeanDock:

- **Documentation and examples:** the four docs in `docs/` and the two stacks in `examples/` are always
  open to clarity fixes and measured results.
- **Tests:** `tests/` contains the platform-tier, DB tuner, Compose override, and proxy singleflight
  suites. New edge cases are the most valuable contribution you can make.
- **Small, well-scoped work:** browse issues labelled
  [`good first issue`](https://github.com/pvbang/leandock/issues?q=is%3Aissue+is%3Aopen+label%3A%22good+first+issue%22)
  or [`help wanted`](https://github.com/pvbang/leandock/issues?q=is%3Aissue+is%3Aopen+label%3A%22help+wanted%22).
- **Real-world reports:** if LeanDock did something unexpected on your host, a complete bug report is a
  contribution. Use the issue templates — they ask for `leandock doctor` output for a reason.

Before opening a pull request, read the [Known Limitations](docs/LIMITATIONS.md) and the
[Roadmap](docs/ROADMAP.md), and note which platforms you actually tested. Security issues must go through
[SECURITY.md](SECURITY.md), not a public issue. All participation is covered by the
[Code of Conduct](CODE_OF_CONDUCT.md).

## Cutting a Release (Maintainers)

See [docs/PUBLISHING.md](docs/PUBLISHING.md) for the complete guide to configuring GitHub Releases, npm, PyPI, crates.io, Homebrew Tap, and Scoop Bucket.
