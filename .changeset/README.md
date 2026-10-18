# Changesets

Every user-visible change adds one small markdown file here. The release workflow turns the pending
files into the next version and the release notes, then deletes them. This README is ignored by the
tooling.

```md
---
bump: minor   # major | minor | patch
---
Add Epic, GOG and Xbox providers.
```

- `bump` is required. The release uses the highest bump of all pending files (major > minor > patch).
  Below 1.0 this is plain semver too: `major` on 0.x goes to 1.0.0.
- The body is the release-note text (one bullet in the notes). Write it for players, not developers.
- File names do not matter. Create one with:

  ```sh
  pnpm changeset add minor "Add Epic, GOG and Xbox providers."
  pnpm changeset status
  ```

Docs-only, CI-only and refactoring changes need no changeset. See `docs/RELEASING.md`.
