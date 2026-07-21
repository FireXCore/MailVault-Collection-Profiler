#!/usr/bin/env node

const fs = require("node:fs");
const path = require("node:path");

const root = path.resolve(__dirname, "..");

function readJson(relativePath) {
  return JSON.parse(fs.readFileSync(path.join(root, relativePath), "utf8"));
}

function fail(message) {
  console.error(`release configuration error: ${message}`);
  process.exit(1);
}

function parseCargoWorkspaceVersion() {
  const cargoToml = fs.readFileSync(path.join(root, "Cargo.toml"), "utf8");
  const workspaceSection = cargoToml.match(
    /\[workspace\.package\]([\s\S]*?)(?=\n\[[^\]]+\]|$)/,
  );

  if (!workspaceSection) {
    fail("Cargo.toml is missing [workspace.package]");
  }

  const version = workspaceSection[1].match(/^version\s*=\s*"([^"]+)"\s*$/m);
  if (!version) {
    fail("Cargo.toml [workspace.package] is missing version");
  }

  return version[1];
}

function parseSemver(version) {
  const match = version.match(
    /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/,
  );

  if (!match) {
    fail(`application version is not valid SemVer: ${version}`);
  }

  return {
    major: Number(match[1]),
    minor: Number(match[2]),
    patch: Number(match[3]),
    prerelease: match[4]?.split(".") ?? [],
  };
}

function expectedWixVersion(appVersion) {
  const parsed = parseSemver(appVersion);

  if (parsed.major > 255 || parsed.minor > 255 || parsed.patch > 65535) {
    fail(
      `application version cannot be represented by MSI limits: ${appVersion}`,
    );
  }

  if (parsed.prerelease.length === 0) {
    return `${parsed.major}.${parsed.minor}.${parsed.patch}`;
  }

  const numericIdentifiers = parsed.prerelease.filter((value) => /^\d+$/.test(value));
  if (numericIdentifiers.length === 0) {
    fail(
      `pre-release version requires a numeric build identifier for MSI: ${appVersion}`,
    );
  }

  const build = Number(numericIdentifiers.at(-1));
  if (!Number.isSafeInteger(build) || build > 65535) {
    fail(`MSI build identifier must be between 0 and 65535: ${build}`);
  }

  return `${parsed.major}.${parsed.minor}.${parsed.patch}.${build}`;
}

const rootPackage = readJson("package.json");
const desktopPackage = readJson("apps/desktop/package.json");
const tauriConfig = readJson("apps/desktop/src-tauri/tauri.conf.json");
const cargoVersion = parseCargoWorkspaceVersion();

const sourceVersions = new Map([
  ["root package.json", rootPackage.version],
  ["desktop package.json", desktopPackage.version],
  ["Cargo.toml workspace", cargoVersion],
  ["Tauri app version", tauriConfig.version],
]);

for (const [location, version] of sourceVersions) {
  if (version !== rootPackage.version) {
    fail(
      `${location} version ${version} does not match source release ${rootPackage.version}`,
    );
  }
}

const targets = tauriConfig.bundle?.targets ?? "all";
const buildsMsi =
  targets === "all" ||
  targets === "msi" ||
  (Array.isArray(targets) && targets.some((target) => target.toLowerCase() === "msi"));

if (buildsMsi) {
  const configuredWixVersion = tauriConfig.bundle?.windows?.wix?.version;
  if (typeof configuredWixVersion !== "string") {
    fail("bundle.windows.wix.version must be set when MSI is a bundle target");
  }

  const expected = expectedWixVersion(rootPackage.version);
  if (configuredWixVersion !== expected) {
    fail(
      `bundle.windows.wix.version is ${configuredWixVersion}; expected ${expected} for ${rootPackage.version}`,
    );
  }
}

console.log(
  `release configuration ok: app=${rootPackage.version}, msi=${tauriConfig.bundle?.windows?.wix?.version ?? "not targeted"}`,
);
