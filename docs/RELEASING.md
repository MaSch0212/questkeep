# Releasing Questkeep

Releases are cut from a manual GitHub Actions run. Versions come from git tags and from
**changesets**: small markdown files that each pull request adds to `.changeset/`.

## Changesets

```md
---
bump: minor   # major | minor | patch
---
Add Epic, GOG and Xbox providers.
```

```sh
pnpm changeset add <major|minor|patch> "<summary>"   # creates .changeset/<random>.md
pnpm changeset status                                 # current version, pending files, next version
```

- Add one per user-visible change. Docs/CI-only changes need none. CI posts a warning (never a
  failure) on pull requests that change code without adding one.
- **Current version** = the highest `vX.Y.Z` git tag (no tag means `0.0.0`).
- **Next version** = current bumped by the highest pending bump. Pre-1.0 is plain semver: `major`
  on 0.x gives 1.0.0, `minor` gives 0.(x+1).0.
- No pending changesets means nothing to release; the workflow ends successfully with a notice.
- Because the repository has no tag yet, the very first release computes from `0.0.0`
  (`minor` gives 0.1.0, `patch` gives 0.0.1). To start somewhere else, push a tag first
  (`git tag v0.1.0 && git push origin v0.1.0`).

## Cutting a release

1. Make sure `main` is green and contains the changesets you want to ship.
2. GitHub, Actions, **Release**, **Run workflow** on `main`. Tick **dry_run** to compute the
   version, render the notes (see the run summary) and build the installers without committing,
   tagging or publishing anything. A dry run may run from any branch; a real release only from `main`.
3. The workflow:
   1. `prepare` computes current/next version and the notes. No changesets: stop.
   2. `build` (Windows, Ubuntu 22.04) writes the version into `src-tauri/tauri.conf.json`,
      `Cargo.toml` (workspace version both crates inherit), `ui/package.json` and the two crate
      entries in `Cargo.lock`, then runs `pnpm tauri build`. Bundles: NSIS `.exe` and `.msi`
      (Windows), `.AppImage` and `.deb` (Linux), plus their `.sig` updater signatures.
   3. `release` (not on dry runs) fails if `main` moved during the build, then commits
      `chore(release): vX.Y.Z` (version files + deleted changesets) as `github-actions[bot]`,
      pushes it, pushes the annotated tag `vX.Y.Z`, generates `latest.json` from the `.sig` files
      and runs `gh release create` with the notes and every asset.
4. Only one release runs at a time (`concurrency: release`). If the `release` job fails after the
   tag was pushed, re-run the failed job: it detects the existing tag and only publishes the
   release (uploading with `--clobber` if the release already exists).

The release commit is pushed with `GITHUB_TOKEN`, so GitHub does not start CI on it (and nothing
loops). If `main` has branch protection, allow `github-actions[bot]` to push, or the `release` job
fails before anything is tagged or published.

## Auto-update signing setup (one time)

The app updates itself through `tauri-plugin-updater`: it reads
`https://github.com/MaSch0212/questkeep/releases/latest/download/latest.json` and verifies the
installer signature against the public key in `src-tauri/tauri.conf.json`. You generate the key:

1. Generate a key pair (pick a password or leave it empty):

   ```sh
   pnpm tauri signer generate -w ~/.tauri/questkeep.key
   ```

   This writes the private key to `~/.tauri/questkeep.key` and the public key to
   `~/.tauri/questkeep.key.pub`. **Never commit the private key; back it up.** If it is lost, installed
   apps can never verify a new key and users must reinstall manually.
2. In GitHub, Settings, Secrets and variables, Actions, add repository secrets:
   - `TAURI_SIGNING_PRIVATE_KEY`: the **content** of `~/.tauri/questkeep.key`
   - `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: the password (empty string if none)
3. Paste the content of `~/.tauri/questkeep.key.pub` as `plugins.updater.pubkey` in
   `src-tauri/tauri.conf.json` (it currently holds a `REPLACE_WITH_PUBLIC_KEY...` placeholder) and
   commit it.

Without the secrets the workflow still works: it warns, builds without updater artifacts and
publishes without `latest.json`, so that release does not auto-update installed apps. With the
secrets set but the placeholder public key still in place, the build fails on purpose.

## Builds without the key

`bundle.createUpdaterArtifacts` is `false` in `tauri.conf.json`, so `pnpm build` and CI work
without the private key. Only the signed release build adds `--config
src-tauri/tauri.updater.conf.json`, which turns updater artifacts on. Dev builds (`pnpm dev`) are
not affected. The update commands return "no update" in debug builds.

## Update commands (for the UI)

- `check_update() -> Option<UpdateInfo>` where
  `UpdateInfo { version: string, notes: string | null, date: string | null }` (`date` is RFC 3339).
  Rejects with a user-readable string if the endpoint or signature setup is broken.
- `install_update() -> ()` downloads, installs and restarts the app (on Windows the installer runs
  in passive mode). It returns normally only when there was no newer release.
