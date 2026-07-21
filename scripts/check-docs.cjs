#!/usr/bin/env node

const fs = require("node:fs");
const path = require("node:path");

const root = path.resolve(__dirname, "..");
const failures = [];
const checkedLinks = [];

const requiredFiles = [
  "README.md",
  "README_FA.md",
  "LICENSE",
  "NOTICE",
  "SECURITY.md",
  "SUPPORT.md",
  "CONTRIBUTING.md",
  "CODE_OF_CONDUCT.md",
  "CITATION.cff",
  "docs/GETTING_STARTED.md",
  "docs/INSTALLATION_WINDOWS.md",
  "docs/GUI_GUIDE.md",
  "docs/CLI_REFERENCE.md",
  "docs/EVIDENCE_OUTPUTS.md",
  "docs/WORKSPACE_FORMAT.md",
  "docs/FINDINGS_REVIEW.md",
  "docs/VALIDATION_0.1.0-alpha.3.md",
  "docs/releases/v0.1.0-alpha.3.md",
  "docs/SECURITY_MODEL.md",
  "docs/PRIVACY.md",
  "docs/TROUBLESHOOTING.md",
  "docs/RELEASE_PROCESS.md",
  "docs/GITHUB_PUBLISHING_GUIDE_FA.md",
  "docs/REPOSITORY_RELEASE_HANDOFF_FA.md",
  "docs/INDEX.md",
  "docs/assets/social-preview.png",
  "docs/assets/screenshots/01-collection-setup-preflight.png",
  "docs/assets/screenshots/02-profile-running.png",
  "docs/assets/screenshots/03-inventory-explorer.png",
  "docs/assets/screenshots/04-findings-explorer.png",
  "docs/assets/screenshots/05-cli-workflow.png",
  "docs/assets/screenshots/06-content-object-detail.png",
  "docs/assets/screenshots/alpha3/01-start.png",
  "docs/assets/screenshots/alpha3/03-runs.png",
  "docs/assets/screenshots/alpha3/06-findings.png",
  ".github/ISSUE_TEMPLATE/bug_report.yml",
  ".github/ISSUE_TEMPLATE/feature_request.yml",
  ".github/ISSUE_TEMPLATE/compatibility_report.yml",
  ".github/PULL_REQUEST_TEMPLATE.md",
  ".github/workflows/ci.yml",
  ".github/workflows/codeql.yml",
  ".github/workflows/dependency-review.yml",
  ".github/workflows/release.yml",
];

for (const relative of requiredFiles) {
  if (!fs.existsSync(path.join(root, relative))) {
    failures.push(`required file is missing: ${relative}`);
  }
}

const excludedDirectories = new Set([".git", "node_modules", "target", "dist", "runtime-evidence"]);
const textExtensions = new Set([
  ".md", ".txt", ".json", ".yml", ".yaml", ".toml", ".cjs", ".js", ".ts", ".tsx", ".rs", ".ps1", ".sh", ".cff",
]);

function walk(directory) {
  const result = [];
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    if (excludedDirectories.has(entry.name)) continue;
    const absolute = path.join(directory, entry.name);
    if (entry.isDirectory()) result.push(...walk(absolute));
    else result.push(absolute);
  }
  return result;
}

const files = walk(root);
const privatePatterns = [
  { label: "absolute Windows user path", regex: /C:\\Users\\[^\\\r\n]+/gi },
  { label: "private workspace path", regex: /C:\\MailVault-Profiler(?:\\|\b)/gi },
  { label: "private machine volume marker", regex: /PRIVATE_MACHINE_VOLUME/gi },
  { label: "raw screenshot filename", regex: /image\([34]\)\.png/gi },
];

for (const absolute of files) {
  const relative = path.relative(root, absolute).replaceAll(path.sep, "/");
  const extension = path.extname(absolute).toLowerCase();
  if (!textExtensions.has(extension) && path.basename(absolute) !== "LICENSE") continue;

  const content = fs.readFileSync(absolute, "utf8");
  if (relative === "scripts/check-docs.cjs") continue;
  for (const pattern of privatePatterns) {
    if (pattern.regex.test(content)) {
      failures.push(`${relative}: contains ${pattern.label}`);
    }
    pattern.regex.lastIndex = 0;
  }
}

for (const relative of ["package-lock.json", ".npmrc"]) {
  const absolute = path.join(root, relative);
  if (!fs.existsSync(absolute)) continue;
  const content = fs.readFileSync(absolute, "utf8");
  if (/applied-caas-gateway|internal\.api\.openai\.org/i.test(content)) {
    failures.push(`${relative}: contains a private package registry reference`);
  }
}

const forbiddenTrackedDirectories = ["node_modules", "target", "apps/desktop/dist", "runtime-evidence"];
if (fs.existsSync(path.join(root, ".git"))) {
  const { execFileSync } = require("node:child_process");
  const tracked = execFileSync("git", ["ls-files"], { cwd: root, encoding: "utf8" })
    .split(/\r?\n/)
    .filter(Boolean);
  for (const relative of forbiddenTrackedDirectories) {
    if (tracked.some((entry) => entry === relative || entry.startsWith(`${relative}/`))) {
      failures.push(`generated or private directory is tracked: ${relative}`);
    }
  }
}

const markdownFiles = files.filter((absolute) => path.extname(absolute).toLowerCase() === ".md");
const markdownLink = /!?\[[^\]]*\]\(([^)]+)\)/g;
for (const markdownFile of markdownFiles) {
  const content = fs.readFileSync(markdownFile, "utf8");
  let match;
  while ((match = markdownLink.exec(content)) !== null) {
    let target = match[1].trim();
    if (target.startsWith("<") && target.endsWith(">")) target = target.slice(1, -1);
    if (!target || target.startsWith("#") || /^(?:https?:|mailto:)/i.test(target)) continue;
    target = target.split("#", 1)[0].split("?", 1)[0];
    try {
      target = decodeURIComponent(target);
    } catch {
      failures.push(`${path.relative(root, markdownFile)}: malformed link encoding: ${match[1]}`);
      continue;
    }
    const resolved = path.resolve(path.dirname(markdownFile), target);
    checkedLinks.push(`${path.relative(root, markdownFile)} -> ${target}`);
    if (!resolved.startsWith(root + path.sep) && resolved !== root) {
      failures.push(`${path.relative(root, markdownFile)}: link escapes repository: ${target}`);
    } else if (!fs.existsSync(resolved)) {
      failures.push(`${path.relative(root, markdownFile)}: broken local link: ${target}`);
    }
  }
}

const screenshotDirectory = path.join(root, "docs/assets/screenshots");
if (fs.existsSync(screenshotDirectory)) {
  const screenshots = walk(screenshotDirectory).filter(
    (absolute) => path.extname(absolute).toLowerCase() === ".png",
  );
  for (const absolute of screenshots) {
    const filename = path.basename(absolute);
    const relative = path.relative(root, absolute).replaceAll(path.sep, "/");
    if (!/^\d{2}-[a-z0-9-]+\.png$/.test(filename)) {
      failures.push(`screenshot filename is not deterministic: ${relative}`);
    }
    const size = fs.statSync(absolute).size;
    if (size < 10_000) failures.push(`screenshot appears empty or invalid: ${relative}`);
  }
}

if (failures.length > 0) {
  console.error("Documentation/privacy gate failed:\n");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log(`documentation/privacy gate ok: ${markdownFiles.length} Markdown files, ${checkedLinks.length} local links, ${requiredFiles.length} required artifacts`);
