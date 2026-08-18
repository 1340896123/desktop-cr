# AGENTS.md

WinUI-style remote desktop client: **Tauri v2 + React 18 + Fluent UI React v9**. POC stage — all Rust backend modules are in-memory mocks; real RustDesk integration is future work. Comments and UI copy are Simplified Chinese; keep that convention.

## Commands

- `npm run dev` — Vite dev server, **port 1420 strict** (`vite.config.ts` sets `root: 'src'`, output to `../dist`, ignores `src-tauri/**` in watch)
- `npm run build` — `tsc && vite build`; this is the only typecheck/lint gate. **No eslint, no test framework, no CI.** tsconfig is strict with `noUnusedLocals`/`noUnusedParameters` — unused code fails the build
- `npm run tauri dev` / `npm run tauri build` — run/build the desktop app
- `cargo check` / `cargo build` in `src-tauri/` — Rust side (Windows deps only under `target.'cfg(windows)'`)
- Single-file Rust commands are `cargo run` in `src-tauri/` only for the whole app; there are no Rust unit tests

## Architecture

- `src/services/*.ts` wrap Tauri `invoke`/`listen`. Every function guards on `isTauri()` (`'__TAURI_INTERNALS__' in window`) and falls back to mock data/console warnings — **so `npm run dev` in a plain browser works without Tauri**. Don't remove the guard when touching services.
- Rust commands (`src-tauri/src/`) are snake_case and registered in `main.rs`: `hbb_client` (mock device list + Mutex-held connection state), `virtual_display`, `input_injector`, `capture`. All are `(mock)` stubs with TODO markers for future phases; Windows-only logic is behind `#[cfg(target_os = "windows")]` so it compiles on Linux.
- Frontend subscribes to the `connection-state` event (`onConnectionStateChange` in `src/services/connection.ts`).
- Design tokens: `src/theme/tokens.ts` (custom palette/spacing/radius, NOT Fluent tokens) — design system spec in `docs/design-system.md`. Components use `makeStyles` from `@fluentui/react-components`.
- README §2.1 lists RustDesk git deps (`hbb_common`, `scrap`) — these are **deliberately commented out** in `Cargo.toml` (slow/fragile builds). Don't re-enable casually; the README is an aspirational blueprint, not current state.

## Gotchas

- `src-tauri/resources/idd_driver/` is a placeholder (empty); bundled via `tauri.conf.json` `bundle.resources`. No driver files exist yet.
- `tauri.conf.json` CSP restricts `connect-src ipc: http://ipc.localhost` — adding real network calls (HBBS/HBBR server) requires updating the CSP.
- Frontend dev server `allowedHosts` includes `.monkeycode-ai.online` for the monkeycode preview domain — keep it if the preview workflow is still used.
- Windows installer is WiX with `zh-CN` language.
- Git history: 2 commits, Chinese commit messages, version 0.1.0.