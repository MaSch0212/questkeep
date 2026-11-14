# AGENTS.md

Guide for humans and coding agents working on Questkeep.

## Commands

| Task | Command |
|---|---|
| Install JS deps | `pnpm install` |
| Run the app (dev) | `pnpm dev` (= `tauri dev`) |
| Build the app | `pnpm build` (= `tauri build`; needs the updater signing key, see `docs/RELEASING.md`) |
| **All quality gates** | `pnpm check` |
| Auto-format, then check | `pnpm fix` |
| Update the Rust lockfile (2-day min age) | `pnpm deps:rust` |
| Add a changeset (every user-visible PR) | `pnpm changeset add <major\|minor\|patch> "summary"` |
| Show version, pending changesets, next version | `pnpm changeset status` |
| List installed Steam games (dev tool) | `cargo run -p questkeep-store-steam --example scan_steam` |
| Build the example store plugin (needs `rustup target add wasm32-wasip2`) | `examples/plugins/example-store/build.ps1` |
| Check executable icon extraction on real games (dev tool) | `cargo run -p questkeep-core --example exe_icons -- <out dir> <exe>...` |
| Regenerate app icons from `assets/logo.svg` | `node scripts/make-icon.mjs && pnpm tauri icon assets/icon.svg -o src-tauri/icons` (tray glyphs: run `pnpm tauri icon assets/tray-dark.svg -o target/tray-dark`, same for `tray-light`, and copy `32x32.png`/`64x64.png` to `src-tauri/icons/tray-*.png`/`tray-*@2x.png`) |

`pnpm check` runs, in order: rustfmt check, clippy (`-D warnings`), `cargo test --workspace`,
oxfmt check, oxlint (`--deny-warnings`), `tsc --noEmit`, vitest. In CI (`CI` env set) it also
fails if `ui/src/bindings` differs from what `cargo test` generated. Run it before every commit. It also runs the release-script tests (`node --test scripts/release/*.test.mjs`).
CI (`.github/workflows/ci.yml`) runs the same steps on Windows and Ubuntu, then builds the app.

UI-only commands run from `ui/`: `pnpm lint`, `pnpm format`, `pnpm format:check`, `pnpm test`,
`pnpm typecheck`, `pnpm build`.

### Windows notes

- Run cargo from PowerShell or cmd, not Git Bash: Git Bash's `link` shadows MSVC's `link.exe`.
- Requires Visual Studio Build Tools with the C++ workload.
- Never invoke `python` in scripts: on stock Windows it's a Microsoft Store stub that hangs.

### Debugging and testing the running app

Set `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` before `pnpm dev`; the
page is then reachable through the Chrome DevTools protocol at `http://127.0.0.1:9222/json`
(screenshots, console, `window.__TAURI_INTERNALS__.invoke(...)`). Put an empty `portable.txt` next
to `target/debug/questkeep.exe` to use a throwaway data dir under `target/debug/data`.

**Agents testing on someone's workstation must not show windows or take focus** (the user may be
playing a fullscreen game). Use:

```powershell
scripts/dev-hidden.ps1 -Portable   # hidden window, DevTools on :9222, low priority, throwaway data
```

It sets `QUESTKEEP_HIDDEN=1` (debug builds only: no tray icon, the window is created invisible, unfocused and
off the taskbar), disables WebView2 background throttling so the page keeps rendering, and runs
cargo/node/the app at below-normal priority with a third of the cores as build jobs. Screenshots
via `Page.captureScreenshot` still work. Also run other heavy commands (clippy, tests, release
builds) with `(Get-Process -Id $PID).PriorityClass = "BelowNormal"; $env:CARGO_BUILD_JOBS = "6"`
first; child processes inherit the priority. Never call `SetForegroundWindow` or screen-capture
APIs (`CopyFromScreen`): they steal focus or capture the user's screen.

## Module map

```
crates/questkeep-store-api/    the store plugin interface: LibraryProvider (+ scan_categories, settings),
                               ImportedGame/PlayAction, auth/ (keyring TokenStore, AccountSession,
                               Http, AccountStatus, LoginWindow, WebSession + WebAccount for website
                               sign-ins), settings.rs (StoreSettings: options, credentials),
                               platform/ (registry, uninstall entries, appx), sqlite (foreign DBs)
crates/stores/<id>/            one crate per store (questkeep-store-<id>): steam, epic, gog, xbox,
                               ubisoft, amazon, battlenet, ea, prism, lutris, heroic (shared Heroic
                               helpers + sideloaded games); icon.svg, login/session scripts,
                               fixtures/ next to src/
crates/questkeep-stores/       the built-in store list (`builtin`); only the app depends on it
crates/questkeep-store-model/  what stores report (model.rs, settings.rs): plain serde data, also
                               compiled into WASM store plugins
crates/questkeep-plugin-sdk/   SDK for store plugins (wasm32-wasip2): Provider trait,
                               export_provider!, log/http helpers, wire.rs (the JSON call format,
                               shared with the host), wit/plugin.wit; examples/sandbox_probe.rs is
                               a test fixture
crates/questkeep-plugins/      plugin host: manifest.rs (plugin.json), install.rs (copy into
                               <data>/plugins/<id>, plugins.json), manager.rs (PluginManager:
                               load/unload/reload/watch, PluginInfo), wasm.rs (WasmProvider on
                               wasmtime: WASI mounts, host http/log, deadlines, memory cap)
examples/plugins/              example plugins, each its own cargo project (not in the workspace);
                               example-store is loaded by the plugin tests
crates/questkeep-imaging/      icon decoding and scaling (core and tray popup)
crates/questkeep-tray-popup/   Windows tray popup (Win32 + Direct2D: layout, render, window, a11y);
                               own input types, no core dependency
crates/questkeep-core/src/     all app logic; sees stores only through the store API
  library.rs                   facade used by the shell (manual add, update, scan, covers, seed)
  model/                       Game, PlayAction, queries, categories, library views (grouping),
                               sync times, sorting names (ts-rs exported)
  db/                          SQLite: migrations.rs (append-only), games.rs, sessions.rs, views.rs
                               (saved views, their sections, last sync times), categories.rs
                               (n:m game categories, store imports), combine.rs (one entry for a
                               game in several stores, duplicate detection)
  providers/                   ProviderRegistry (+ ProviderInfo for the UI) and the manual provider
  launcher/                    launch + tracker threads (mod.rs), process matching (process.rs)
  images/                      cover/background cache (mod.rs) + cover and metadata fetchers (fetcher.rs)
  events.rs                    EventBus + CoreEvent
  platform/                    executable icons, URI opening, processes, argument handling (cfg-gated)
  settings.rs, paths.rs, seed.rs, error.rs
src-tauri/src/                 thin shell: commands.rs (one wrapper per command), lib.rs (setup)
  signin.rs                    store login windows and the hidden store-session window, both
                               driven by the store's LoginWindow description
  tray.rs                      tray icon and native menu; hands the popup its data
  updates.rs                   update check after start, the found update, installing it
                               (debug builds: QUESTKEEP_FAKE_UPDATE=<version> pretends one exists)
ui/src/
  lib/api.ts                   the only module that calls invoke()
  lib/store.ts                 app state + core-event handling
  lib/extensions/              the extension host for plugins' JavaScript: host (load, reload,
                               unload), registry (what plugins added, undone per plugin), api
                               (the QuestkeepApi object), settings, declaredSettings, menus;
                               the API types are examples/plugins/questkeep-plugin.d.ts
  lib/themes.ts                theme registry (registerTheme), theme parameters as --qk-param-*
                               properties, applyAppearance (data-scheme/data-theme on <html>)
  lib/*.ts                     pure helpers (gridNav, grouping: groups + virtual rows + slot
                               navigation, copies: a game's store copies, format, names, query,
                               views: drafts/filters/arranging, commandBar) + tests
  components/                  one component per file; Popover + Menu for every popup menu;
                               DetailsPanel (game page + side panel), EditGameDialog, ConfirmDialog;
                               Extension* + PluginPage + settings/PluginSettings draw what
                               extensions add
  bindings/                    GENERATED by ts-rs during cargo test; never edit by hand
  theme.css                    every visual value as a --qk-* variable: the default theme's light
                               and dark palette, shades derived from the accent
  themes/                      built-in themes: <id>.css (rules on :root[data-theme="<id>"]) and
                               index.ts (definitions, parameters); literal colors allowed here
```

## Conventions

- **Commits:** Conventional Commits (`feat(core): ...`, `fix(ui): ...`, `docs: ...`), small,
  straight to `main` while bootstrapping. Keep `main` green.
- **Rust:** no `unwrap()`/`expect()` outside tests (clippy enforces it); errors are `CoreError`
  with user-readable messages. Log with `tracing`. Core stays sync (threads, not async).
- **Ported logic:** re-implement, never copy C#. Mark files with
  `// Logic derived from Playnite (MIT), see THIRD_PARTY_NOTICES.md`. Reference sources
  (read-only): `E:\other\Playnite`, `E:\other\PlayniteExtensions`.
- **Schema changes:** append a step to `db/migrations.rs`; never edit a released step.
- **New store:** add a crate under `crates/stores/<id>` that implements `LibraryProvider` from
  `questkeep-store-api` (+ `cover_candidates` if possible; an account with its `LoginWindow`;
  `settings` for options or credentials) and list it in `questkeep_stores::builtin`. No core, shell,
  UI or DB changes needed: everything store-specific stays in its crate.
- **Command contract:** change Rust types, run `cargo test` to regenerate `ui/src/bindings`, commit
  both. Commands stay thin; logic goes into core with tests.
- **UI:** no literal colors/sizes outside `theme.css` and the built-in themes in `ui/src/themes/`
  (the one other sanctioned place); labeled buttons; keyboard paths for every
  action. Pure logic goes into `lib/` with vitest tests. Tooltips: `data-tooltip="…"` on the
  element (shown by `TooltipLayer`), never the native `title` attribute.
- **Dependencies:** pnpm installs only versions at least 2 days old (`minimumReleaseAge`); Cargo
  has `min-publish-age = "2 days"`. Prefer major-only ranges in `package.json`.
- **Releases:** changesets in `.changeset/` + the manual Release workflow; see `docs/RELEASING.md`.
  Docs/CI-only changes need no changeset.
- Record non-obvious choices in `docs/DECISIONS.md` (one or two lines of rationale) and ideas in
  `docs/ROADMAP.md`.

## Sub-agent policy

The orchestrating agent owns architecture, the data model and migrations, concurrency (launcher,
fetcher), anything data-loss or security sensitive, cross-cutting refactors, and the final review
of all delegated work. Delegate well-specified building work with a self-contained brief: context,
exact files to touch, interfaces to respect, acceptance criteria, and what not to change. Never let
two agents edit the same files at once, and never commit delegated output without reviewing the
diff and running `pnpm check`.

### Delegation notes (what worked in v0.1)

- **Haiku: reading and summarizing reference code works well.** It produced accurate, cited notes
  on Playnite's model, Steam/Epic detection and process tracking (spot-checked; one claim about a
  missing class was correctly flagged as "not found" instead of guessed).
- **Sonnet: the Steam provider + VDF parser from a spec was right first time** (27 tests,
  realistic fixtures, sensible extra guards). Giving it the exact trait, file boundaries and the
  skip rules list was enough.
- **Sonnet: the full SolidJS UI from one detailed brief worked**, with two lessons:
  1. It could only run jsdom tests, so it never saw the real webview. The virtualized grid passed
     every test but did not scroll in the app (adapter/core version mismatch). **Always verify UI
     work in the running app** (DevTools protocol, see above) before accepting it.
  2. Tell UI agents to stop dev servers they start; a leftover Vite process confused later runs.
- **v0.2: five parallel Sonnet agents in git worktrees** (Epic, GOG, Xbox, Steam extras, UI),
  each committing to its own branch with a shared `CARGO_TARGET_DIR`, merged by hand. Only the
  `providers/mod.rs` registration lines conflicted. Again the UI passed all jsdom tests but had a
  layout bug (flex items shrinking in the details panel) that only the hidden-app screenshot
  showed.
- **Keep the orchestrator off `cargo fmt --all` while a Rust sub-agent edits files**; it rewrites
  their files under them. Format only after they finish.
- Scripted multi-line edits through `sed`/`node -e` broke twice (escaping); prefer the Edit tool
  for anything with backslashes or quotes.
