#!/usr/bin/env node

const fs = require("node:fs");
const path = require("node:path");

const root = path.resolve(__dirname, "..");
const tag = process.argv[2] || process.env.GITHUB_REF_NAME;
if (!tag) {
  console.error("usage: node scripts/check-tag-version.cjs v<semver>");
  process.exit(2);
}

const rootPackage = JSON.parse(fs.readFileSync(path.join(root, "package.json"), "utf8"));
const desktopPackage = JSON.parse(fs.readFileSync(path.join(root, "apps/desktop/package.json"), "utf8"));
const tauri = JSON.parse(fs.readFileSync(path.join(root, "apps/desktop/src-tauri/tauri.conf.json"), "utf8"));
const expected = `v${rootPackage.version}`;

const failures = [];
if (tag !== expected) failures.push(`tag ${tag} does not match package version ${expected}`);
if (desktopPackage.version !== rootPackage.version) failures.push("desktop package version differs from root package version");
if (tauri.version !== rootPackage.version) failures.push("Tauri version differs from root package version");

const releaseNotes = path.join(root, "docs/releases", `${tag}.md`);
if (!fs.existsSync(releaseNotes)) failures.push(`missing versioned release notes: docs/releases/${tag}.md`);

if (failures.length > 0) {
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log(`tag/version gate ok: ${tag}`);
