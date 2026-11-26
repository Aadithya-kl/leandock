## What this changes and why

<!-- One or two paragraphs. Link the issue if there is one: "Closes #123". -->

## Safety checklist (the three oaths)

- [ ] Original Compose files and Dockerfiles are never modified in place.
- [ ] Every host file write goes through `BackupSession::backup_and_write` so `leandock revert` stays complete.
- [ ] Unsupported environments (non-root, LXC/OpenVZ, older kernels, Windows, macOS) skip cleanly — no panic, no abort.
- [ ] New behaviour is covered by a test in `tests/`, or I explained why a test is not practical.
- [ ] User-facing flags, labels, or behaviour are documented in `README.md` and/or `docs/`.

## Platforms tested

- [ ] Linux (KVM / bare metal) — distro and kernel:
- [ ] Linux (OpenVZ / LXC) — detail:
- [ ] Windows (WSL2) — Docker Desktop / Rancher:
- [ ] macOS — Docker Desktop / OrbStack / Colima / Lima:

## Verification

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
```

<!-- Paste the relevant output, or state which checks could not run and why. -->

## Notes for reviewers

<!-- Trade-offs, follow-ups, or anything you want a second opinion on. -->
