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

## Done in the next release

Plugins: sandboxed WASM stores, JavaScript extensions and themes; appearance settings with light
and dark mode, new themes and theme parameters; the theme editor; auto categories; text filters,
"Not" and drag and drop for filters; back and forward navigation; the user and developer wiki.

Also: Amazon, Ubisoft, Battle.net and EA verified live (owned and installed games, install,
uninstall, launch, Open launcher); group combining (e.g. "Shooters" for FPS and TPS, per
grouping field); duplicates found across editions ("GOTY", "Definitive Edition") and numeral
styles ("VII" / "7"); sorting names read roman numerals and number words like Playnite; Amazon
uninstalls through the game's own remover, EA's uninstaller gets the UAC prompt it needs; Epic
installs found through the launcher's newer install records; the Battle.net sign-out notice;
uninstall and remove from the game's right-click menu work again.

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
- **Command bar commands**: run app actions (sync a store, open settings, switch views) from
  Ctrl+K, not only games.

## Blocked

- **Per-game Epic launcher uninstall**: no URI exists; revisit if Epic documents one.
- **Game Pass by entitlement**: the entitlement API is for publishers only (see DECISIONS).

## Later

- Plugins (stores, extensions and themes exist, see `docs/PLUGINS.md` and `docs/THEMES.md`),
  still open:
  - a plugin gallery (browse and install from within the app) and signature checks;
  - account sign-in and credentials for plugin stores, registry access for WASM stores, guest
    output (stdout/stderr) in the log;
  - extensions: more places to add to (game page sections, filters, columns), translations per
    plugin;
  - themes: theming the native tray popup and the window's background color at startup (both
    are drawn outside the page), `@import` of relative files in theme CSS, translated theme
    texts for plugin themes.
- Fullscreen / controller mode (the grid keyboard navigation is the groundwork).
- Multiple play actions per game, emulator profiles, pre/post scripts.
- Minimize while playing.
- Cloud sync of the library database.
