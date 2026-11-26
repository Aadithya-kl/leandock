# Maintainer Release & Multi-Channel Publishing Guide

This guide explains how maintainers configure accounts, tokens, and repositories so that pushing a single Git tag (`git tag v0.2.0 && git push --tags`) automatically builds and publishes **LeanDock** across every distribution channel:

| Channel | User Command | Registry / Repo Required | GitHub Secret / Variable |
|---|---|---|---|
| **GitHub Releases** (`curl` / PowerShell / `cargo-binstall`) | `curl -fsSL .../install.sh \| bash` | Your GitHub repo | Built-in `GITHUB_TOKEN` (zero setup) |
| **npm** (`npx` / `npm`) | `npx leandock analyze` | [npmjs.com](https://www.npmjs.com/) + `@leandock` org/scope | Secret `NPM_TOKEN` |
| **PyPI** (`uvx` / `pipx`) | `uvx leandock analyze` | [pypi.org](https://pypi.org/) (Trusted Publishing OIDC) | Repo variable `ENABLE_PYPI_PUBLISH=true` |
| **crates.io** (`cargo`) | `cargo binstall leandock` / `cargo install leandock` | [crates.io](https://crates.io/) | Secret `CARGO_REGISTRY_TOKEN` |
| **Homebrew Tap** (`brew`) *(optional, disabled)* | `brew install <owner>/tap/leandock` | GitHub repo `<owner>/homebrew-tap` | Secret `TAP_PUSH_TOKEN` |
| **Scoop Bucket** (`scoop`) *(optional, disabled)* | `scoop bucket add <owner> ... && scoop install leandock` | GitHub repo `<owner>/scoop-bucket` | Secret `TAP_PUSH_TOKEN` |

> Every optional channel in `.github/workflows/release.yml` checks whether its secret/variable is configured and **skips cleanly** if unset. You can enable channels one by one at your own pace.

---

## One-Time Setup Checklist

### 1. GitHub Repository (Required — Powers `curl`, PowerShell & `cargo-binstall`)
1. Create a public repository on GitHub (for example `https://github.com/<your-org>/leandock`).
2. Repository URLs are already pinned to `https://github.com/pvbang/leandock`. If you fork or rename the repository, update them consistently in:
   - `Cargo.toml` (`repository`, `homepage`, `documentation`)
   - `pyproject.toml` (`[project.urls]`)
   - `packaging/npm/leandock/package.json` & `packaging/npm/prepare-packages.mjs`
   - `packaging/homebrew/leandock.rb.tmpl` & `packaging/scoop/leandock.json.tmpl`
   - `scripts/install.sh` & `scripts/install.ps1`
   - `README.md`
3. No extra secret is needed for GitHub Releases — `GITHUB_TOKEN` has `contents: write` permission in `.github/workflows/release.yml`.

---

### 2. npm Registry Setup (`npx leandock` & `npm install -g leandock`)
LeanDock uses the zero-postinstall `optionalDependencies` architecture (identical to `esbuild`, `biome`, and `turbo`), publishing 5 small platform binary packages plus the main `leandock` wrapper:
- `@leandock/cli-linux-x64`
- `@leandock/cli-linux-arm64`
- `@leandock/cli-darwin-x64`
- `@leandock/cli-darwin-arm64`
- `@leandock/cli-win32-x64`
- `leandock`

**Steps:**
1. Create an account on [npmjs.com](https://www.npmjs.com/) and enable 2FA.
2. Create a free public npm Organization named `leandock` (or change `@leandock/` in `packaging/npm/` to your own npm scope).
3. Go to **Access Tokens -> Generate New Token -> Granular Access Token** with **Read and write** permissions for `leandock` and `@leandock/*`.
   - **Tick “Bypass two-factor authentication (2FA)”** — without it npm rejects publishes from CI with `403 ... Two-factor authentication or granular access token with bypass 2fa enabled is required`.
   - Set **Packages** to **All packages** (or explicitly select both the unscoped `leandock` package and the `@leandock` scope); a token limited to `@leandock/*` fails the unscoped wrapper with `403 ... You may not perform that action with these credentials`. Alternatively use a classic **Automation** token.
4. Add the token in GitHub: **Settings -> Secrets and variables -> Actions -> New repository secret**:
   - Name: `NPM_TOKEN`
   - Value: `<your npm granular token>`

---

### 3. PyPI Setup (`uvx leandock` & `pipx install leandock`)
LeanDock uses `maturin` (`bindings = "bin"`) in `pyproject.toml` to compile native platform wheels so Python/AI users run `uvx leandock` without needing Rust installed. The release workflow builds wheels for macOS (x64/arm64), Windows (x64) and Linux (x64/arm64) in both `manylinux_2_17` (glibc: Debian/Ubuntu/Fedora/RHEL) and `musllinux_1_2` (musl: Alpine/distroless) flavours.

**Steps (Passwordless OIDC Trusted Publishing — Recommended):**
1. Create an account on [pypi.org](https://pypi.org/) and enable 2FA.
2. Go to **Account Settings -> Publishing -> Add a new pending publisher**:
   - **PyPI Project Name:** `leandock`
   - **Owner:** `<your GitHub username or org>`
   - **Repository name:** `leandock`
   - **Workflow name:** `release.yml`
   - **Environment name:** `pypi`
3. In your GitHub repository:
   - Go to **Settings -> Environments -> New environment** and create an environment named `pypi`.
   - Go to **Settings -> Secrets and variables -> Actions -> Variables -> New repository variable**:
     - Name: `ENABLE_PYPI_PUBLISH`
     - Value: `true`

---

### 4. crates.io Setup (`cargo install leandock` & `cargo binstall leandock`)
1. Sign in to [crates.io](https://crates.io/) with your GitHub account.
2. Go to **Account Settings -> API Tokens -> New Token** with `publish-new` and `publish-update` scopes.
3. Add it as a GitHub Actions secret:
   - Name: `CARGO_REGISTRY_TOKEN`
   - Value: `<your crates.io token>`

> `Cargo.lock` **must stay committed** — the workflow publishes with `cargo publish --locked`.

---

### 5. Homebrew Tap & Scoop Bucket (`brew` & `scoop`) — optional, not enabled
This channel is intentionally **not configured** for the current release. The `update-package-managers` job in `release.yml` skips cleanly while `TAP_PUSH_TOKEN` is unset. To enable it later:
1. Create two empty public GitHub repositories under the same GitHub owner:
   - `homebrew-tap` (will contain `Formula/leandock.rb`)
   - `scoop-bucket` (will contain `bucket/leandock.json`)
2. Create a GitHub Fine-Grained Personal Access Token (PAT) with **Contents: Read and write** scoped ONLY to `homebrew-tap` and `scoop-bucket`.
3. Add it as a GitHub Actions secret in the main `leandock` repo:
   - Name: `TAP_PUSH_TOKEN`
   - Value: `<your PAT>`

---

## Cutting a New Release

1. Bump the version consistently across:
   - `Cargo.toml` (`version = "0.x.y"`)
   - `pyproject.toml` (`version = "0.x.y"`)
   - `packaging/npm/leandock/package.json` (`"version"` and `optionalDependencies`)
   - `CHANGELOG.md`
2. Run local verification:
   ```bash
   cargo fmt --all -- --check
   cargo clippy --all-targets -- -D warnings
   cargo test --all-targets
   ```
3. Commit, tag, and push:
   ```bash
   git add -A
   git commit -m "chore(release): v0.2.0"
   git tag v0.2.0
   git push origin main --tags
   ```
4. Monitor the **Release** workflow in GitHub Actions — within ~10 minutes, every configured channel (`curl`, PowerShell, `npx`, `uvx`, and `cargo binstall`) will serve `v0.2.0` automatically.
