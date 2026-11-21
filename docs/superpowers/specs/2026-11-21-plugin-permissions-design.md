# Plugin permission model rework: design

Status: draft for approval. Builds on `2026-11-14-plugins-design.md` and `docs/PLUGINS.md`.

## Intent and why

Store plugins run in a WASM sandbox that can reach only `/plugin`, `/data`, folders listed in
`permissions.read` (fixed host paths), folders the user chooses (`permissions.folders`) and
declared https hosts. That is too stiff for what a store plugin has to do: find where a launcher
is installed, read its config and databases, sign the user in. The built-in stores do all of this
with host-side tools plugins do not have (registry, uninstall entries, appx, foreign SQLite,
keyring, login windows, web sessions).

The goal: **a new store plugin can do everything the built-in stores can do** (the capability
bar is the set of built-ins: Steam, Epic, GOG, Xbox, Ubisoft, EA, Battle.net, Amazon,
Meta/Oculus, Prism, Lutris, Heroic), and a store plugin can find its install, launcher and config
paths on its own. **The built-in stores stay built-in.** Moving any of them out into plugins is
explicitly not part of this work; the survey below only defines which capabilities the plugin
system must offer.

The trade-off is chosen on purpose: plugins get broad read access, and safety comes from
**review in the plugin registry**, a visible permission summary at install time, and a warning
for plugins installed from a zip or folder.

## Goals

- Capability parity: a new plugin could implement any of the built-in stores' behavior (see
  "What each store needs" below). Built-ins are not migrated and not changed.
- A plugin discovers paths itself: registry, known folders, any file on the PC (read-only).
- Credentials are stored as securely as in the built-ins (OS keyring).
- Sign-in flows of all six account-based stores are describable as plugin data.
- Everything a plugin can reach that can carry data off the PC is **visible at install**: http
  hosts and sign-in domains.
- "What can a plugin do" is written down accurately for every plugin kind (a section in
  `docs/PLUGINS.md`, see Documentation).

## Non-goals

- A sandbox that stops a malicious plugin from reading user files. Not offered.
- Process enumeration for plugins (no store needs it).
- Changing the extension (JavaScript) trust model beyond documenting it (see Open points).
- Migrating or rewriting any built-in store as a plugin, or changing the built-in stores' code
  (the shared host-side code, such as `read_foreign` and the login windows, may be reused or
  generalized, but built-in behavior must not change).
- Window automation for plugins (Prism's Ctrl+N keystroke is dropped).
- Any write access outside the plugin's own `/data`.

## Anti-goals

- A plugin that can write outside `/data`, run programs by itself, or send data to a host not
  listed at install.
- Silent permission growth: an update that adds access must never apply unseen.
- A plugin API whose security is weaker than the built-ins' for the same job: a plugin store
  with an account must have the same keyring-backed token storage as the built-ins.

## Today (what plugins can and cannot do)

Store plugins (WASM): read-only `/plugin`; read-write `/data`; `permissions.read` and
`permissions.folders` mounts (read-only, fixed at load time); `http-request` to declared hosts
(https; `http://` only when declared; redirects off); `log`. No environment, registry, processes,
stdio, raw network. Returns data, including `PlayAction` (`file` or `uri`); Questkeep launches and
tracks.

Extension plugins (JS): trusted code in the app window. Every Tauri command (including starting
and removing games), the DOM, `fetch` to https hosts. No declaration, no sandbox, nothing listed
at install except the trust warning. Cannot add stores (that needs WASM).

Themes: plain CSS, no code, but can restyle or hide any part of the app.

Reference: Playnite loads extensions as full-trust .NET DLLs with no sandbox and no permissions;
its only protections are a warning text, a server-side blacklist checked at startup, and safe
mode (restart without extensions, aimed at crashes).

## What each store needs (survey of the built-in stores)

| Store | Registry | Files | Login / secrets |
|---|---|---|---|
| Steam | `…\Valve\Steam` | Steam dir, `libraryfolders.vdf` (other drives), `appmanifest_*.acf`, `loginusers.vdf` | web session, keyring |
| Epic | none | `%ProgramData%\Epic\…\Manifests`, `LauncherInstalled.dat`; Linux legendary/Heroic | OAuth window, keyring |
| GOG | Galaxy client, install records | `galaxy-2.0.db` | OAuth window, keyring |
| Xbox | none | `AppxManifest.xml`, `MicrosoftGame.config`; appx package list | login window, keyring |
| Ubisoft | `…\Ubisoft\Launcher`, uninstall entries | game YAML | none |
| EA | `origin2` shell command, uninstall entries, keys named in each game's `installerdata.xml` | `installerdata.xml` per game | web session |
| Battle.net | uninstall entries | `%ProgramData%\Battle.net\Agent` (`product.db`, `aggregate.json`) | web session |
| Amazon | uninstall entries | `%LOCALAPPDATA%\Amazon Games` SQLite; Nile config (Linux) | OAuth window, keyring |
| Meta/Oculus | `oculus` shell command, library paths | library `Manifests` (any drive), `data.sqlite` | none |
| Prism | uninstall entries | `%APPDATA%\PrismLauncher`, configurable instance dirs | none |
| Lutris / Heroic | none | `pga.db`, `~/.config/heroic/**` | none |

Findings: no store reads processes; no store needs whole-disk read as such, but several follow
chains (registry value, directory, file content, more directories) that a static list cannot
express. This survey was grep-level; the builder should confirm each capability against the
built-in code that uses it. Nothing is ported: the validation is the example plugin and tests
below.

## Approaches considered

For how plugins get access to discovered folders:

- **A. Declarative discovery only** (host resolves manifest rules, mounts the result). Safest;
  cannot express file-content-driven chains (Steam libraries, Meta, EA, Prism).
- **B. A plus plugin-requested mounts, confirmed once by the user.** Strict; prompts on every
  second drive.
- **C. A plus requested mounts auto-granted by a declared "shape".** No prompts; the shape is
  declared by the plugin itself, so it is a review aid, not enforcement.
- **D (chosen by the user). Full read-only filesystem and registry access for plugins that
  declare it, trust through registry review.** Simplest, makes all stores expressible with no
  chain logic in the host. Cost: no technical protection against a malicious plugin; the user
  accepted this, including that Questkeep's own data folder (library DB, other plugins' data, the
  web-session browser profile) is readable. A deny list through a custom filesystem layer and
  moving the web-session profile were considered and not chosen.

Decisions on the other points are in the next section.

## Design

### Trust model

1. **Registry plugins** are reviewed in a PR before they are listed and are hash-checked at
   install (existing). The review rules below apply.
2. **Plugins installed from a zip or folder** show a prominent security warning in the install
   dialog: only install plugins from authors and sources you trust. (Extensions already show a
   trust warning; this extends to every plugin that declares filesystem or registry access, and
   stays for extensions.)
3. **Updates never apply access unseen.** Any change to `http`, `login` domains, `filesystem`,
   `registry` or `launcher.processNames` keeps the update out of "Update all" and shows the
   change in the update dialog (extends the existing `newHttp` / `newRead` / `newFolders` rule;
   `PLUGIN_REGISTRY.md` lists the fields).
4. **Review rules** (for the registry repo's contributing docs): no wildcard hosts or
   shared-hosting domains (`*.github.io`, `*.pages.dev`, S3 buckets and similar) in `http` or
   login `domains` for a plugin that declares `filesystem`; `http://localhost` needs a stated
   reason.

### Manifest

New and changed `permissions` fields (`apiVersion` stays 1; older Questkeep versions ignore
unknown fields, so a plugin that needs them sets `minApp` in the registry, as with
`permissions.folders`):

```json
"permissions": {
  "filesystem": "read",
  "registry": "read",
  "http": ["api.gog.com"],
  "launcher": { "processNames": ["GalaxyClient.exe"] },
  "login": {
    "title": "Sign in to GOG",
    "url": "https://auth.gog.com/auth?...",
    "domains": ["auth.gog.com", "embed.gog.com"],
    "capture": { "param": "code", "urlPrefix": "https://embed.gog.com/on_login_success" },
    "userAgent": null,
    "script": "login.js",
    "session": { "url": "...", "script": "session.js", "renewUrl": null }
  }
}
```

Existing `read` and `folders` stay (stores that only need a fixed or user-chosen folder keep the
narrow sandbox and need no `filesystem`). A plugin without `filesystem` and `registry` behaves
exactly as today.

### Filesystem (`permissions.filesystem: "read"`)

- The whole file system is mounted read-only into the plugin (on Windows each fixed drive,
  e.g. as `/host/c/…`; on Linux/macOS the root), next to `/plugin` and `/data`.
- Paths a plugin returns (install dirs, play action targets, working dirs, tracking paths, image
  candidates, launcher actions) are translated back to real paths by extending the existing
  mount translation. `/plugin` and `/data` still never translate to launchable paths.
- Locked files are not readable through WASI. SQLite goes through `sqlite-query` (below).

### Registry (`permissions.registry: "read"`)

Host functions (WASM cannot reach the registry): `registry-read(path) → option<string>` (value
or default value, `[HKEY…\Key\Value]` style), `registry-enum(path) → list<string>` (subkeys),
and `uninstall-entries() → list<UninstallEntry>` and `appx-packages() → list<AppxPackage>` reusing
the store API's existing readers. Read-only. Not available (and returns empty) on non-Windows.
Names are provider-chosen, so EA's game-data-driven keys work.

### Host functions added to the WIT `host` interface

- `registry-*`, `uninstall-entries`, `appx-packages` (above).
- `platform() → windows | linux | macos` and `known-folder(name) → option<string>`
  (`home`, `local-app-data`, `app-data`, `program-data`, `config` (XDG-aware), `data`). WASM has
  no environment, so plugins cannot read `%LOCALAPPDATA%` or `~/.config` themselves. Replaces
  the idea of per-platform manifest sections.
- `sqlite-query(path, sql, params) → rows as JSON`, read-only, with today's `read_foreign`
  behavior: open read-only, fall back to a temporary copy including the `-wal` file. Plugins
  never ship SQLite.
- `secret-get(key)`, `secret-set(key, value)`, `secret-delete(key)`: see Secrets.
- Login and web-session calls: see Login.

### Secrets

Host functions over the OS keyring (the same mechanism as `TokenStore`). Questkeep prefixes
every key with the plugin id, so a plugin reads and writes only its own secrets through the API.
Plugins must not keep tokens in `/data`; the SDK docs and registry review rules say so. (A
plugin can still read other plugins' `/data` through `filesystem`; that is why this matters.)
Remove deletes the plugin's keyring entries. Backup/update behavior of `/data` is unchanged.

### Login windows and web sessions

A declarative `permissions.login` block (above), with scripts shipped in the plugin folder:

- The host owns the window and the profile; the plugin supplies URLs, scripts and the capture
  rule. Replaces `&'static str` scripts and `fn` pointers of today's `LoginWindow` and
  `WebSessionPage`.
- Navigation in these windows is restricted to the declared `domains`; anything else is blocked.
- A script's output goes only back to the calling plugin. The persistent web-session browser
  profile is separate per plugin id.
- The sign-in domains are listed in the install dialog next to the http hosts.
- Redirect capture (Epic, GOG, Xbox, Amazon) and web sessions (Steam, EA, Battle.net) both map
  onto this block. Exact script/bridge protocol and the plugin-facing calls
  (`login-start`, `session-read`) are left to the builder.

### Closing the launcher

`launcher_process()` keeps its shape but the **process names come from the manifest**
(`permissions.launcher.processNames`), listed in the install dialog. Questkeep only closes
processes whose names are declared and only when the user enabled "Close launcher after playing"
for that store (existing setting). The store still chooses the close method (clean command, close
windows, force-quit) among those.

### Prism "Add instance"

The keystroke injection (`start_in_front` + Ctrl+N) is dropped for the plugin version. The option
starts Prism with `--show-window` through a normal play action and the user presses Ctrl+N. No
window-automation capability is offered.

### Install dialog

For every plugin: name, version, source (registry / folder / zip), then a permission summary:
filesystem and registry read (named plainly: "can read every file and registry value on this
PC"), http hosts, sign-in domains, launcher process names, user-chosen folders (existing).
Non-registry sources show the trust warning. The existing `folders` UI stays.

### Extensions

Unchanged technically (see Open points). Documented as "trusted code, can call every command,
fetch every https host".

### Remote kill list (optional follow-up)

Playnite checks a blacklist at startup. The registry could publish a list of withdrawn plugin
ids/versions that Questkeep checks with its update check and shows as a warning. Not part of this
design; listed because the trust model now leans on the registry.

## Documentation

- Add a "What a plugin can and cannot do" section to `docs/PLUGINS.md` covering stores,
  extensions and themes (table), generated from this design.
- Update `docs/PLUGINS.md`, `docs/PLUGIN_REGISTRY.md` (new compare fields), `examples/plugins/README.md`,
  `docs/ARCHITECTURE.md` (host functions), and the wiki pages for plugin developers and for the
  install dialog. Record the trust-model decision in `docs/DECISIONS.md` (new "Plugin trust"
  entry at the end of the plugins section).
- Screenshots of the new install dialog: placeholders per AGENTS.md unless taken from the demo
  library.

## Details settled after review (author's calls, marked for the user to check)

1. **Consent is the existing confirmation.** The install and update dialogs already require a
   click on Install/Update after showing the permissions; no extra checkbox. An update that adds
   any of `filesystem`, `registry`, `http`, login `domains`, `launcher.processNames` is not
   applied by "Update all" and its dialog marks what is new (same flow as `newHttp`).
2. **Secret keys.** The plugin passes any opaque key string; the host stores it under
   `questkeep-plugin/<plugin id>/<key>`, so keys cannot collide across plugins. Built-in stores'
   keyring entries are untouched and not visible to plugins through the API (they can only be
   reached by a plugin that breaks the "no secrets in plain files" rule, which review checks).
3. **Closing the launcher.** The plugin's close method is data: a clean-exit command, close
   windows, or force-quit. A clean-exit command is run by the host only if its executable file
   name equals one of the declared `processNames`; otherwise it is ignored. No other
   plugin-supplied command line is executed by this flow.
4. **Platforms.** Windows and Linux (the CI targets). macOS is out of scope. Windows-only
   functions (registry, uninstall entries, appx, web-session windows as today) return empty or an
   error elsewhere. No generic "read locked file" function; only `sqlite-query` handles locked
   databases. Add one later if a store needs it.
5. **Login window parity.** The `login` schema must express all six flows (GOG, Epic, Xbox,
   Amazon as redirect capture; Steam, EA, Battle.net as web sessions). A plugin receives only the
   captured value or the script's returned output, never the cookie jar.
6. **Overlapping mounts.** Translation picks the longest matching guest prefix. A path that
   resolves into Questkeep's `plugins` or `plugin-data` folders is never translated for launch,
   install, uninstall, launcher or tracking actions, whether it came through `/plugin`, `/data` or
   the whole-filesystem mount. Image candidates may still read from there.
7. **`read` and `folders` stay** for plugins that do not declare `filesystem` (narrow sandbox),
   and are ignored in effect (but still validated) when `filesystem` is declared.

## Open points

- **Extension permissions.** Not discussed. Proposal (my call): unchanged and documented. If the
  user wants consistency with stores (declared http hosts, listed at install), that is a separate
  small design.
- **Mount layout.** `/host/c/…` for drives is a proposal; the builder may choose another stable
  naming.
- **Cookies and redirects** in `http-request` are untested against the Epic and GOG flows.

## Left to the builder

Host function signatures and WIT types; the login/session script bridge; the translation layer
for drive paths; how the plugin API reuses the host-side code the built-in stores already use
(without changing built-in behavior); tests, including at least one example plugin (extending
`examples/plugins/example-store` or a new one) that exercises each new capability (registry,
whole-filesystem read, `sqlite-query`, secrets, login window/web session, declared launcher
process names) so parity is demonstrated without porting a real store; `minApp` and registry
compare-field changes; SDK docs.
