# CLAUDE.md

Guidance for working on Winstow with Claude Code.

## Sources of truth

- [README.md](README.md): setup, CLI usage, configuration, and local testing.
- [docs/TECHNICAL_CONCEPT.md](docs/TECHNICAL_CONCEPT.md): intended behavior and edge cases.
- [.github/workflows/ci.yml](.github/workflows/ci.yml): required checks and release size limit.
- [docs/STATIC_LINKING.md](docs/STATIC_LINKING.md): Windows runtime linking.

## Working on code

- Start at `src/main.rs` for command flow. CLI and configuration live in `src/cli.rs` and `src/config.rs`; stow and unstow planning live in `src/stow.rs` and `src/unstow.rs`.
- Keep planning free of filesystem mutations. Represent changes as `planner::Action`s and execute them through `Plan::execute`. Conflict checks happen per package before its actions run; execution is sequential and has no rollback for I/O failures. Restow runs unstow and stow separately.
- Use `src/path_utils.rs` for path normalization, relative paths, and Windows-aware path comparisons; use `src/logger.rs` for user-facing output.
- When behavior changes, update the relevant parts of the technical concept, README, and CHANGELOG.

## Verification

- Add integration scenarios to the appropriate feature file in `tests/`; shared binary and sandbox helpers are in `tests/common/mod.rs`.
- Run the checks from the CI workflow. Symlink tests need a Windows host with Developer Mode or elevation. For the Docker-based workflow on Windows, see the README and `check-windows-docker.sh`.
