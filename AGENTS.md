# AGENTS.md

Guide for humans and coding agents working on Questkeep.

## Commands

| Task | Command |
|---|---|
| Install JS deps | `pnpm install` |
| Run the app (dev) | `pnpm dev` (= `tauri dev`) |
| Build the app | `pnpm build` (= `tauri build`) |
| **All quality gates** | `pnpm check` |
| Auto-format, then check | `pnpm fix` |
| Update Rust lockfile (2-day min age) | `pnpm deps:rust` |

`pnpm check` runs: rustfmt check, clippy (`-D warnings`), `cargo test --workspace`, oxfmt check,
oxlint, `tsc --noEmit`, vitest. Run it before every commit.

On Windows, run cargo from PowerShell or cmd, not Git Bash: Git Bash's `link` shadows MSVC's
`link.exe` when the MSVC environment is not found.
