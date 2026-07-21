#!/usr/bin/env node

const fs = require("node:fs");
const path = require("node:path");
const { Parser, Language } = require("web-tree-sitter");

const root = path.resolve(__dirname, "..");
const ignoredDirectories = new Set([".git", "node_modules", "target", "dist"]);

function walk(directory) {
  const files = [];
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    if (ignoredDirectories.has(entry.name)) continue;
    const absolute = path.join(directory, entry.name);
    if (entry.isDirectory()) files.push(...walk(absolute));
    else if (entry.isFile() && entry.name.endsWith(".rs")) files.push(absolute);
  }
  return files;
}

function resolveInputs(arguments_) {
  if (arguments_.length === 0) {
    return walk(root).sort((left, right) => left.localeCompare(right));
  }

  const files = [];
  for (const argument of arguments_) {
    const absolute = path.resolve(root, argument);
    if (!fs.existsSync(absolute)) {
      throw new Error(`Rust syntax input does not exist: ${argument}`);
    }
    const stat = fs.statSync(absolute);
    if (stat.isDirectory()) files.push(...walk(absolute));
    else if (stat.isFile() && absolute.endsWith(".rs")) files.push(absolute);
    else throw new Error(`Rust syntax input must be a .rs file or directory: ${argument}`);
  }

  return [...new Set(files)].sort((left, right) => left.localeCompare(right));
}

async function main() {
  const files = resolveInputs(process.argv.slice(2));
  if (files.length === 0) {
    throw new Error("No Rust source files were found for syntax validation.");
  }

  const runtimeWasmPath = require.resolve("web-tree-sitter/web-tree-sitter.wasm");
  await Parser.init({
    locateFile(filename) {
      return filename === "web-tree-sitter.wasm" ? runtimeWasmPath : filename;
    },
  });

  const rustWasmPath = path.join(
    root,
    "node_modules",
    "@vscode",
    "tree-sitter-wasm",
    "wasm",
    "tree-sitter-rust.wasm",
  );
  if (!fs.existsSync(rustWasmPath)) {
    throw new Error(`Rust Tree-sitter WASM grammar is missing: ${rustWasmPath}`);
  }

  const rust = await Language.load(rustWasmPath);
  const parser = new Parser();
  parser.setLanguage(rust);

  const failures = [];
  for (const sourcePath of files) {
    parser.reset();
    const source = fs.readFileSync(sourcePath, "utf8");
    const tree = parser.parse(source);
    if (!tree || tree.rootNode.hasError) {
      failures.push(path.relative(root, sourcePath).replaceAll(path.sep, "/"));
    }
    tree?.delete();
  }
  parser.delete();

  if (failures.length > 0) {
    console.error("Rust syntax validation failed:\n");
    for (const failure of failures) console.error(`- ${failure}`);
    process.exit(1);
  }

  console.log(`Rust syntax validation passed: ${files.length} source files`);
}

main().catch((error) => {
  console.error(error instanceof Error ? error.stack ?? error.message : error);
  process.exit(1);
});
