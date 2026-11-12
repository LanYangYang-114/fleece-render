# Agent Instructions

## Read first

- `README.md`
- `docs/ARCHITECTURE.md`
- `docs/API.md`
- `docs/HANDOFF.md`

## Project Rules

1. This is a pure-Rust server renderer. Do not add a browser, Node.js, Playwright, WebGL, GUI, or mandatory GPU dependency.
2. Keep `fleece-render-core` independent from HTTP/config/profile-provider code.
3. Keep the CPU backend fully functional; a GPU backend must always be optional and feature-gated.
4. Preserve published HTTP routes and camelCase query fields while evolving `/api/v1`.
5. Preserve deterministic image output for identical inputs and settings.
6. URL, data URL/base64, and raw image bytes must render identically for identical textures/options.
7. Use nearest-neighbor texture sampling and the existing box UV / triangle winding conventions.
8. Every render must remain under the five-second target on CPU at the default size.
9. Add timeouts and byte limits to any new network input.
10. Update `docs/API.md` when the HTTP contract changes and `docs/HANDOFF.md` when architecture changes.

## Build / Test

Windows:

```powershell
.\scripts\test.ps1
.\scripts\build.ps1
powershell -ExecutionPolicy Bypass -File .\scripts\acceptance.ps1
```

Linux:

```bash
./scripts/test.sh
./scripts/build.sh
```

Before finalizing code changes, run:

```text
cargo fmt --all -- --check
cargo test --locked --workspace
cargo check --locked --workspace
```

## Notes

- Windows scripts use `scripts/dev-env.ps1` to activate a workspace-local toolchain when one is present.
- Generated images/logs belong in `output/` and must not be committed.
- Workspace-local toolchains and temporary directories are not tracked.
