# Plugin registry

The plugin browser in Settings > Plugins lists plugins from one registry: the GitHub repository
[`questkeep/plugins`](https://github.com/questkeep/plugins). This page is the contract between
that repository, plugin repositories and the app.

- Registry index (what the app fetches): `https://questkeep.github.io/plugins/index.json`
- Plugin files are **not** stored in the registry. Each version points at a zip in the plugin
  author's own GitHub Release; the registry pins its `sha256`.

## Registry repository layout

```
plugins/<id>/meta.json       what the browser shows (reviewed on every PR)
plugins/<id>/icon.svg        or icon.png (square, SVG preferred, PNG 256x256 max, 200 KB max)
plugins/<id>/versions.json   every released version, newest last
scripts/validate.mjs         checks the above (CI on every PR)
scripts/build-index.mjs      writes site/index.json and copies icons to site/icons/
.github/workflows/           ci.yml (validate + build), pages.yml (deploy site/ on push to main)
```

`<id>` is the plugin's id from its `plugin.json`: 2 to 64 lowercase letters, digits and dashes.

### meta.json

```json
{
  "id": "bsmanager",
  "name": "BSManager",
  "summary": "One line, at most 120 characters.",
  "description": "A few sentences, plain text, at most 2000 characters.",
  "author": "Name",
  "repository": "https://github.com/questkeep/plugin-bsmanager",
  "homepage": "https://optional.example",
  "license": "MIT",
  "tags": ["store", "beat-saber"],
  "kinds": ["store"]
}
```

`kinds` is any of `store`, `extension`, `theme` (what the plugin brings; shown as badges).
`homepage` and `tags` are optional, everything else is required. `repository` must be an
`https://github.com/<owner>/<repo>` URL.

### versions.json

```json
[
  {
    "version": "0.1.0",
    "url": "https://github.com/questkeep/plugin-bsmanager/releases/download/v0.1.0/bsmanager-0.1.0.zip",
    "sha256": "<64 lowercase hex characters>",
    "size": 123456,
    "apiVersion": 1,
    "minApp": "0.7.0",
    "released": "2026-11-20",
    "notes": "Optional, short.",
    "yanked": false
  }
]
```

- `version`: semver, unique, listed in ascending order.
- `url`: `https://` only. GitHub release assets (`github.com/<owner>/<repo>/releases/download/...`)
  are what the validator accepts; the app follows GitHub's redirects.
- `sha256`: of the zip. The app refuses a file that does not match.
- `size`: bytes (informational; the app also caps the download at 100 MB).
- `apiVersion`: the plugin API version the plugin was built for (`API_VERSION` in
  `crates/questkeep-plugins/src/manifest.rs`, currently 1).
- `minApp`: the lowest Questkeep version that can run it.
- `yanked`: a withdrawn version. Hidden from the browser unless "show all versions" is on, never
  offered as an update, kept so people who installed it can see what they have.

The zip is a normal plugin zip (`plugin.json` at its top or in its only folder, see
`docs/PLUGINS.md`). Its `plugin.json` must have the same `id` and `version` as the entry; the
registry CI downloads each new version and checks that.

### index.json (generated)

```json
{
  "schema": 1,
  "generated": "2026-11-20T12:00:00Z",
  "plugins": [
    {
      "id": "bsmanager",
      "name": "BSManager",
      "summary": "...",
      "description": "...",
      "author": "...",
      "repository": "...",
      "homepage": null,
      "license": "MIT",
      "tags": ["store"],
      "kinds": ["store"],
      "icon": "icons/bsmanager.svg",
      "versions": [ { "version": "0.1.0", "url": "...", "sha256": "...", "size": 1, "apiVersion": 1,
                      "minApp": "0.7.0", "released": "2026-11-20", "notes": null, "yanked": false } ]
    }
  ]
}
```

`icon` is relative to the index URL. Plugins are sorted by name, versions ascending. `schema`
changes only for breaking changes; the app ignores an index with a higher `schema` than it knows
and says that the app needs an update.

## What the app does

1. **Fetch** the index (ureq, 15 s timeout, 5 MB cap, `https` only; debug builds also accept the
   `QUESTKEEP_REGISTRY_URL` environment variable, `http://localhost` allowed). The last good copy is
   cached in `<data>/plugins/registry-cache.json` and shown (marked as offline) when the fetch fails.
2. **Compatible** versions: `apiVersion == API_VERSION` and `minApp <= app version`, not yanked.
3. **Install**: download the zip (https, 100 MB cap, redirects followed), check `sha256`, keep the zip in
   `<data>/plugin-cache/<id>/<version>.zip`, show the normal install preview (permissions, the
   extension trust warning), then install through the existing staging path
   (`questkeep_plugins::install`). The plugin's `plugin.json` must match the entry's `id` and
   `version`, or the install fails.
4. **State** (`plugins.json`): the entry remembers `registry: { version, pinned }`. `source` is the
   cached zip, so Reload reinstalls from it. `pinned` is set when the user installs a version that
   is not the newest compatible one (a downgrade or an older pick); pinned plugins are not offered
   updates.
5. **Update / downgrade**: the same install with another version. Before the swap the plugin's own
   data folder is copied to `<data>/plugin-data-backup/<id>/<old version>/` (last 3 kept), because a
   downgrade can meet data a newer version migrated.
6. **Check for updates**: installed registry plugins whose newest compatible, non-yanked version is
   higher than the installed one (and are not pinned) show "Update available" (see
   [Update check](#update-check)).
7. Plugins installed from a folder or file are never touched by the registry, even when the id
   matches a registry plugin (they show as "installed from a folder" in the browser).

## Update check

- **When:** about 20 seconds after start (in a background thread, never blocking startup) and then
  every 12 hours while the app runs. Settings > Plugins has the switch "Check for plugin updates"
  (`checkPluginUpdates`, default on) and a "Check now" button. With the switch off no automatic
  request is made; the button still asks.
- **Only with a reason:** the index is requested only when at least one installed plugin has
  `registry` state (installed from the registry, pinned or not). Without one there is no network
  request at all, also for "Check now" (which then says so).
- **What it does:** it downloads the index like the browser does (so `QUESTKEEP_REGISTRY_URL`
  works in debug builds) and keeps it as the cache that `plugin_updates` reads. A registry that
  cannot be reached is silent for the automatic check (a debug log line); "Check now" reports it.
  The core event `pluginUpdatesChanged` is sent when the set of updates changed, and the UI reads
  `plugin_updates` again.
- **The notification** is a to-do for the updates that are pending now: one entry in the
  notification bell ("2 plugin updates available: BSManager 0.1.0 → 0.2.0, …", information level,
  button "View updates", no system notification). It exists while at least one update is pending
  and the user has not dismissed it, and its text always lists all pending updates. When the set
  changes the entry is updated in place; it turns unread only when a (plugin, version) pair
  appears that its text did not list. It goes by itself when the last update is installed, or the
  plugin is removed or pinned. Opening the bell, "View updates" and closing the dialog never
  remove it; installing the updates or the entry's own dismiss button (or "Clear all") do.
  It is saved with the other notifications, so it survives a restart, and it is created again
  right after start from the cached index (no network, not after the 20 s delay) when updates
  are pending. `<data>/plugins/plugin-update-notices.json` remembers which entry is in the bell
  and which pairs the user dismissed (found by the entry missing from the bell): those pairs
  stay quiet, a new pair brings the entry back unread. The automatic check and "Check now" use
  the same logic (with the switch off, the cache alone creates no entry). A file in the first
  format (a plain list of pairs) loads as an empty memory.
- **Nothing is installed on its own.** The entry and Settings > Plugins > Updates open the Plugin
  updates dialog: one row per plugin (icon, name, `0.1.0 → 0.2.0`, the version's `notes`) with an
  Update button (the normal download, preview and install dialog) and "Update all".
- **Update all** downloads and checks every update one after the other (`preview_registry_plugin`)
  and then asks once. An update whose preview has `changes.needsReview` is **not** part of it: it
  stays in the list ("Needs your review: asks for …") with its own Update button, which opens the
  usual install dialog showing the permissions. The others are installed one after the other
  (`install_registry_plugin` without `folders`, so earlier folder choices are kept); one failure does
  not stop the rest. The dialog lists what happened per plugin, and the updated plugins' stores are
  scanned like after a single update.
- **`changes`** (in `RegistryPluginPreview`) compares the version with the *installed* manifest:
  `newHttp` (hosts), `newRead` (folders), `newFolders` (`permissions.folders` keys),
  `addsFilesystem` and `addsRegistry` (the broad read permissions), `newLogin` (sign-in domains),
  `newLauncher` (programs Questkeep may close), `addsExtension`, `addsStore` (more stores than
  installed) and `needsReview` (any of them). Hosts, domains, names and paths compare ignoring
  case; removing a permission is not a change. A plugin that is not installed has no changes.

## Trust

Stores run sandboxed, but a store that declares `permissions.filesystem` or `permissions.registry`
can read everything the user can; extensions run as trusted code. What stands between a plugin and
the user's data is the review of every plugin in a PR to the registry before it is listed, and
every file is pinned by hash. A compromised plugin repository cannot change what a listed version
contains. Signing `index.json` is a later step (ROADMAP).

Review rules for a plugin that declares `filesystem` or `registry` (what it reads can only leave
the PC through the web hosts it lists, so those get the most attention):

- No wildcard hosts and no shared-hosting domains (`*.github.io`, `*.pages.dev`, storage buckets
  and similar) in `permissions.http` or `login.domains`: anyone can put content on them.
- `http://localhost` needs a stated reason.
- Hosts are the store's own (API, CDN, sign-in); every host is explained in the PR.
- Tokens go to `secrets`, never to `/data`.
- An update that adds `filesystem`, `registry`, a host, a sign-in domain or a launcher program is
  reviewed again like a new plugin (the app does not apply it in "Update all" either).

Plugins installed from a folder or zip skip the review; the install dialog warns about that.
