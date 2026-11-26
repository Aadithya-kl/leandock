# Security Policy

LeanDock edits host-level Docker and kernel configuration, so security reports are taken seriously and
handled privately.

## Supported versions

| Version | Supported |
|---|---|
| 0.2.x | Yes — security fixes ship in the next patch release |
| < 0.2 | No |

## Reporting a vulnerability

**Do not open a public issue.** Use GitHub's private vulnerability reporting:

1. Open <https://github.com/pvbang/leandock/security/advisories/new>.
2. Describe the issue, affected version(s), platform, and a minimal reproduction.
3. If you cannot use that form, contact the maintainer through <https://github.com/pvbang>.

Please include, where relevant:

- The exact `leandock --version` and `leandock doctor` output (redact hostnames and paths).
- Whether the issue requires root, a local user in the `docker` group, or only network access.
- Any host files modified, and whether `leandock revert` restored them.
- Whether the released binary, wheel, or npm package is affected, and whether checksums matched.

You can expect an acknowledgement within **7 days**. Fixes are released as a patch version and credited
in the release notes unless you ask to stay anonymous.

## Scope

In scope:

- Integrity or verification bypass of release artifacts (checksums, npm/PyPI/crates.io packages).
- Path traversal or arbitrary file read/write in the static file server, proxy, or backup/revert logic.
- Command or argument injection through Compose files, labels, service names, or engine discovery.
- Privilege escalation or unsafe file permissions in `BackupSession` snapshots and `leandock revert`.
- Request smuggling, header injection, or unsafe handling of untrusted HTTP input in `leandock proxy`.
- Denial of service caused by a hostile Compose file or label value (panic, unbounded allocation).

Out of scope:

- Attacks that require an already compromised host, root shell, or a deliberately exposed Docker socket.
- Issues that only exist because the Docker daemon itself is misconfigured or publicly reachable.
- Resource exhaustion that is inherent to the stack the user chose to run.
- Vulnerabilities in third-party tools LeanDock invokes (`docker`, `podman`, `slim`, `podlet`).

## Safe harbour

Good-faith research on your own machines or disposable infrastructure is welcome. Do not test against
other people's hosts, and do not include real secrets in reports.
