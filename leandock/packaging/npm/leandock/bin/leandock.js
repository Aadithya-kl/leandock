#!/usr/bin/env node
"use strict";

const { spawnSync } = require("child_process");
const path = require("path");

const PLATFORM_PACKAGES = {
  "linux-x64": "@leandock/cli-linux-x64",
  "linux-arm64": "@leandock/cli-linux-arm64",
  "darwin-x64": "@leandock/cli-darwin-x64",
  "darwin-arm64": "@leandock/cli-darwin-arm64",
  "win32-x64": "@leandock/cli-win32-x64",
};

function resolveBinaryPath() {
  const key = `${process.platform}-${process.arch}`;
  const pkgName = PLATFORM_PACKAGES[key];

  if (!pkgName) {
    console.error(
      `[leandock] Unsupported platform/architecture: ${key}.\n` +
        `Install from source instead: cargo install --git https://github.com/leandock/leandock`
    );
    process.exit(1);
  }

  const binName = process.platform === "win32" ? "leandock.exe" : "leandock";

  try {
    const pkgDir = path.dirname(require.resolve(`${pkgName}/package.json`));
    return path.join(pkgDir, "bin", binName);
  } catch {
    console.error(
      `[leandock] Could not locate platform binary package "${pkgName}".\n` +
        `If you installed with --no-optional or --omit=optional, reinstall without omitting optionalDependencies:\n` +
        `  npm install -g leandock\n` +
        `Or install the standalone binary directly:\n` +
        `  curl -fsSL https://raw.githubusercontent.com/leandock/leandock/main/scripts/install.sh | bash`
    );
    process.exit(1);
  }
}

const binPath = resolveBinaryPath();
const result = spawnSync(binPath, process.argv.slice(2), {
  stdio: "inherit",
});

if (result.error) {
  console.error(`[leandock] Failed to execute ${binPath}: ${result.error.message}`);
  process.exit(1);
}

if (result.signal) {
  process.kill(process.pid, result.signal);
} else {
  process.exit(result.status ?? 0);
}
