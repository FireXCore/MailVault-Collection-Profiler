#!/usr/bin/env bash
set -euo pipefail

node scripts/check-release-config.cjs
node scripts/check-docs.cjs
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --exclude mailvault-profiler-desktop --locked -- -D warnings
cargo test --workspace --all-features --exclude mailvault-profiler-desktop --locked
npm ci
npm run check:rust-syntax
npm run type-check
npm run build
