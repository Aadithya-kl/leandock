#!/usr/bin/env node
/**
 * Release helper invoked by .github/workflows/release.yml.
 * Packages prebuilt Rust binaries into platform-specific npm packages
 * (@leandock/cli-*) and updates the main `leandock` wrapper package version.
 *
 * Usage:
 *   node packaging/npm/prepare-packages.mjs <version> <binaries-dir> <out-dir>
 */

import fs from "fs";
import path from "path";

const [, , rawVersion, binariesDir, outDir] = process.argv;

if (!rawVersion || !binariesDir || !outDir) {
  console.error(
    "Usage: node packaging/npm/prepare-packages.mjs <version> <binaries-dir> <out-dir>"
  );
  process.exit(1);
}

const version = rawVersion.replace(/^v/, "");

const TARGETS = [
  {
    pkg: "@leandock/cli-linux-x64",
    dirName: "cli-linux-x64",
    os: "linux",
    cpu: "x64",
    rustTarget: "x86_64-unknown-linux-musl",
    bin: "leandock",
  },
  {
    pkg: "@leandock/cli-linux-arm64",
    dirName: "cli-linux-arm64",
    os: "linux",
    cpu: "arm64",
    rustTarget: "aarch64-unknown-linux-musl",
    bin: "leandock",
  },
  {
    pkg: "@leandock/cli-darwin-x64",
    dirName: "cli-darwin-x64",
    os: "darwin",
    cpu: "x64",
    rustTarget: "x86_64-apple-darwin",
    bin: "leandock",
  },
  {
    pkg: "@leandock/cli-darwin-arm64",
    dirName: "cli-darwin-arm64",
    os: "darwin",
    cpu: "arm64",
    rustTarget: "aarch64-apple-darwin",
    bin: "leandock",
  },
  {
    pkg: "@leandock/cli-win32-x64",
    dirName: "cli-win32-x64",
    os: "win32",
    cpu: "x64",
    rustTarget: "x86_64-pc-windows-msvc",
    bin: "leandock.exe",
  },
];

fs.mkdirSync(outDir, { recursive: true });

for (const target of TARGETS) {
  const srcBin = path.join(binariesDir, target.rustTarget, target.bin);
  if (!fs.existsSync(srcBin)) {
    throw new Error(`Missing built binary at ${srcBin}`);
  }

  const pkgDir = path.join(outDir, target.dirName);
  const binDir = path.join(pkgDir, "bin");
  fs.mkdirSync(binDir, { recursive: true });

  const destBin = path.join(binDir, target.bin);
  fs.copyFileSync(srcBin, destBin);
  if (target.os !== "win32") {
    fs.chmodSync(destBin, 0o755);
  }

  const pkgJson = {
    name: target.pkg,
    version,
    description: `Prebuilt LeanDock binary for ${target.os}-${target.cpu}`,
    license: "MIT",
    repository: {
      type: "git",
      url: "https://github.com/pvbang/leandock.git",
    },
    os: [target.os],
    cpu: [target.cpu],
    files: ["bin/"],
    publishConfig: {
      access: "public",
    },
  };

  fs.writeFileSync(
    path.join(pkgDir, "package.json"),
    `${JSON.stringify(pkgJson, null, 2)}\n`
  );
  console.log(`Prepared ${target.pkg}@${version}`);
}

// Copy and version-stamp the main `leandock` wrapper package
const mainSrc = path.resolve("packaging/npm/leandock");
const mainDest = path.join(outDir, "leandock");
fs.mkdirSync(path.join(mainDest, "bin"), { recursive: true });
fs.copyFileSync(
  path.join(mainSrc, "bin/leandock.js"),
  path.join(mainDest, "bin/leandock.js")
);
fs.chmodSync(path.join(mainDest, "bin/leandock.js"), 0o755);
fs.copyFileSync(path.resolve("README.md"), path.join(mainDest, "README.md"));
fs.copyFileSync(path.resolve("LICENSE"), path.join(mainDest, "LICENSE"));

const mainPkg = JSON.parse(
  fs.readFileSync(path.join(mainSrc, "package.json"), "utf8")
);
mainPkg.version = version;
for (const dep of Object.keys(mainPkg.optionalDependencies)) {
  mainPkg.optionalDependencies[dep] = version;
}
fs.writeFileSync(
  path.join(mainDest, "package.json"),
  `${JSON.stringify(mainPkg, null, 2)}\n`
);
console.log(`Prepared leandock@${version}`);
