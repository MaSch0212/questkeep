# Roadmap

Ideas not built yet, roughly in priority order. Not a commitment.

## Done in v0.1 (first public release)

Steam, Epic, GOG and Xbox installed games; Steam owned games, Install action, imported Steam
playtime; store metadata and backgrounds (Steam, GOG); list view. Linux store support:
Heroic/Legendary (Epic, GOG) and Lutris installed games. Account sign-in for Epic and GOG (tokens
in the OS keyring): owned games, Epic playtime, covers, backgrounds and metadata. Tray icon with a
native popup, start with the system, optional GOG launch through Galaxy, signed auto-updates and
the release workflow.

## Done in v0.2

Xbox account sign-in (owned PC games, playtime); Xbox covers, backgrounds and descriptions from the
Microsoft Store catalog; Epic covers without sign-in (public catalog); Uninstall action per store;
custom title bar. Follow-ups: Legendary per-game uninstall; GOG `Uninstall\<id>_is1` installs and
uninstallers; per-game low-quality covers; Epic catalog cache on disk; GOG gamesdb covers; option
to skip Game Pass titles; Heroic Amazon (Nile) and sideloaded games; owned games
from Heroic/Lutris libraries; IGDB metadata for games without store data.

Also: GOG playtime from GOG Galaxy's local database; IGDB fills missing genres (Epic genres).

## Done in v0.3

Categories (synced from Steam collections, GOG Galaxy tags and Lutris); saved views in sidebar
sections; the title bar with Questkeep menu, Add game, Open launcher, Sync and the Ctrl+K command
bar; update check at start; combined games for titles in several stores; redesigned game details
with the Edit dialog. New stores: Amazon Games, Ubisoft Connect, Battle.net, EA app and Prism
Launcher; sign-in for Steam, Amazon, Battle.net and EA. One "Stores and accounts" list in Settings.

## Done in v0.4

Multi-select with Change all, bulk combine and the combine screen; docked, resizable details
panel; games started outside Questkeep are tracked; a richer game page (links, scores, time to
beat, achievements, news, players now); richer metadata with source priority, and/or filter
groups, more sort and group fields, completion statuses and Explore; Edit game for every field
(user values survive refreshes), launch profiles and the image picker; Home page; table columns;
first-run setup; apps vs games; backups; translations (de, fr, it, es, pt).

## Done in v0.5

Groups as a list beside the games; the Statistics page; store order; background sync every 15
minutes; installs watched until done; portable exe and rpm package; store websites and price
comparison sites in the + menu.

## Done in v0.6

Plugins: sandboxed WASM stores, JavaScript extensions and themes; appearance settings with light
and dark mode, new themes and theme parameters; the theme editor; auto categories; text filters,
"Not" and drag and drop for filters; back and forward navigation; the user and developer wiki.

Also: Amazon, Ubisoft, Battle.net and EA verified live (owned and installed games, install,
uninstall, launch, Open launcher); group combining (e.g. "Shooters" for FPS and TPS, per
grouping field); duplicates found across editions ("GOTY", "Definitive Edition") and numeral
styles ("VII" / "7"); sorting names read roman numerals and number words like Playnite; Amazon
uninstalls through the game's own remover, EA's uninstaller gets the UAC prompt it needs; Epic
installs found through the launcher's newer install records; the Battle.net sign-out notice;
uninstall and remove from the game's right-click menu work again. Add game is the Edit dialog
with a Setup section: installed programs to pick from (Apps list and Start menu) and a metadata
match from IGDB, the Steam store or the GOG catalog that later refreshes keep using; the game page
scrolls sideways instead of squeezing in narrow windows. Every change in the Edit dialog, images
and launch profiles included, waits for Save, and Add game creates nothing before it; Combine
groups… lists the grouping's combined groups to edit or split up, and Settings lists all of them and
combines groups of any grouping.

Launch automation: game scripts (pre, post and action scripts; global, filtered or per game; inline
scripts for the installed PowerShell, Bash, Command Prompt or Python with highlighting and
autocomplete, game details as variables and placeholders); closing store launchers after playing
(never, always, or only when the game started them); minimizing or hiding Questkeep while a game
runs; the notification bell with system notifications for errors in the background; the Edit
dialog asks before Escape or a click outside discard changes.

## Done in the next release

New store: Meta Horizon Link (the Meta Quest PC app): installed PC VR apps from its library
folders, owned ones from its local cache without a sign-in, store art, details and PC playtime.

Also: Edit game has a Time played field (a store only raises it when its total is larger, so 0 plus a sync takes the store's time back) and a Name for
metadata field (metadata and artwork searches; a trailing "(2010)" becomes the year); EA imports
play time and last played from its account, and duplicate editions are imported once; Settings
has a Tray group (tray icon choice) and the tray's Quick launch holds store launchers and picked
apps in your order, dragged into place; the popup's quick launch has 1 to 3 rows and puts entries that do not fit into a menu behind its last tile (...); Explore shows games and apps by default. Apps say Start instead of Play. The window keeps its height when it is closed and reopened.

Settings are restructured: pages grouped under App, Library and System, one page at a time (General, Appearance, Window and tray, Stores and accounts, Organizing, Artwork and metadata, Price comparison, Scripts, Plugins, Backups, Updates and about), settings in cards with switches for the ones that save at once, the content width capped, and Quick launch edited in a dialog.

Plugin browser: install and update community plugins from the registry. Questkeep checks for plugin
updates at start and every 12 hours (switch and "Check now" in Settings > Plugins), keeps the
pending updates in one notification bell entry, and the Plugin updates dialog updates one plugin or all of them
("Update all" leaves out updates that ask for new permissions).

Plugins can have an optional icon (`icon` in `plugin.json`), shown in Settings > Plugins and the install dialog.

Plugins can ask for folders you choose (`permissions.folders`, Settings > Plugins > Folders), mounted
read-only for their stores, and paths their stores return below such a folder become real paths.

## Next

- **Verify on Linux**: Heroic Amazon (Nile) and sideloaded games, owned games from Heroic/Lutris.
- **GOG playtime without Galaxy** (Linux/Heroic): no web endpoint accepts the Galaxy token.
- **Verify live**: the Steam sign-in with a real account; Amazon uninstall through
  `Amazon Game Remover.exe`; EA uninstall with the UAC prompt; Epic games known only from the
  install helper's `*.egi` records. Check the product codes of rarer Battle.net titles (only
  `WTCG`/`Fen`/`VIPR` are mapped specially).
- **Categories follow-ups**: Heroic categories; map a single store category onto an existing
  category without merging whole categories; check `sharedconfig.vdf` tags on an older Steam
  install.
- **Small gaps**: an import exclusion list (a removed store game comes back on the next sync
  today), a random game picker that respects the current filter, region, version and manual
  (path or URL) fields.
- **Command bar commands**: run app actions (sync a store, open settings, switch views) from
  Ctrl+K, not only games; also the home for the random picker and plugin commands.
- **URI protocol, CLI start and desktop shortcuts**: `questkeep://` start/show game/search,
  `--start <id>`; enables Stream Deck, Steam shortcuts and launcher integrations.
- **Multiple play actions per game**: file, URL, script and emulator actions, play and non-play
  (e.g. a config tool); the groundwork for emulation.
- **Logos and trailers**: a logo image kind (Steam, SteamGridDB) and trailers on the game page;
  the media Playnite themes rely on most (Extra Metadata Loader), later a launch splash screen.
- **Discord Rich Presence and per-game HDR, display and audio**; HDR state is already read in
  `questkeep-system`.
- **Plugin gallery**: browse, install and update plugins from within the app, with signature
  checks.
- **Metadata provider plugins**, fitted into source priority; then PCGamingWiki and ProtonDB as
  the first providers.
- **New stores**: itch.io, Humble, Rockstar, PlayStation (as store crates or WASM plugins).
- **Controller navigation in desktop mode** (Gamepad API on top of the grid navigation), then
  **fullscreen / controller mode** (controller-first shell, on-screen keyboard, game menu and
  settings, music, power menu, fullscreen themes).
- **Emulation**: emulator definitions and custom profiles, ROM scanner (archives,
  multi-disc), DAT matching by CRC or serial, platform list. Playnite's emulator and platform
  YAML is MIT-licensed and could be reused with attribution.

## Blocked

- **Per-game Epic launcher uninstall**: no URI exists; revisit if Epic documents one.
- **Game Pass by entitlement**: the entitlement API is for publishers only (see DECISIONS).

## Later

- Plugins (stores, extensions and themes exist, see `docs/PLUGINS.md` and `docs/THEMES.md`),
  still open:
  - account sign-in and credentials for plugin stores, registry access for WASM stores, guest
    output (stdout/stderr) in the log;
  - extensions: more places to add to (game page sections, filters, columns), translations per
    plugin;
  - themes: theming the native tray popup and the window's background color at startup (both
    are drawn outside the page), `@import` of relative files in theme CSS, translated theme
    texts for plugin themes.
- Cloud sync of the library database.

---

The items from "Small gaps" to "Emulation" under Next come from a comparison with Playnite and
its add-ons ([Questkeep vs Playnite](https://claude.ai/artifact/ETqu1V4TVLWaZ5hmbNUnok), private
report, 2026-11-15), in its suggested order: value per day of work, with dependencies respected.
The report also lists every compared feature and add-on with rough effort estimates.
