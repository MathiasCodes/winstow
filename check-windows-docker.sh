#!/usr/bin/env bash
# Build the Windows binary in Docker, then run the integration tests on Windows.
set -euo pipefail

repo_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)
skill_scripts="${RUST_DOCKER_SCRIPTS:-$HOME/.claude/skills/rust-docker/scripts}"

if [[ ! -f "$skill_scripts/rust-windows-build.sh" || ! -f "$skill_scripts/rust-windows-test.sh" ]]; then
    echo "Rust Docker scripts not found in $skill_scripts" >&2
    exit 1
fi
if ! command -v cygpath >/dev/null 2>&1; then
    echo "Run this script from Git Bash on Windows (cygpath is required)" >&2
    exit 1
fi

cd -- "$repo_dir"
"$skill_scripts/rust-windows-build.sh" --debug --bin winstow
export WINSTOW_TEST_BIN
WINSTOW_TEST_BIN=$(cygpath -w "$repo_dir/target/docker-windows/debug/winstow.exe")
"$skill_scripts/rust-windows-test.sh" "$@"
