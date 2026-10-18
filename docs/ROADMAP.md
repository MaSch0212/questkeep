# Roadmap

Ideas not built yet, roughly in priority order. Not a commitment.

## Done in v0.2

Epic, GOG and Xbox installed games; Steam owned games, Install action, imported Steam playtime;
store metadata and backgrounds (Steam, GOG); list view. Linux store support: Heroic/Legendary
(Epic, GOG) and Lutris installed games. Account sign-in for Epic and GOG (tokens in the OS
keyring): owned games, Epic playtime, covers, backgrounds and metadata. Tray icon with a native
popup, start with the system, auto-update and the release workflow.

## Done for v0.3

Xbox account sign-in (owned PC games, playtime); Xbox covers, backgrounds and descriptions from the
Microsoft Store catalog; Epic covers without sign-in (public catalog); Uninstall action per store;
optional GOG launch through Galaxy; tray icons that follow cover changes; a notice when a second
start hands over to an already-running copy.

## Next

- **Signed updates**: v0.2.0 shipped without updater signatures; set up the signing key (see
  `docs/RELEASING.md`) before the next release.
- **Uninstall follow-ups**: per-game Epic uninstall (it only opens the launcher library).
- **GOG**: use the `Uninstall\<id>_is1` registry key both for installs missing from
  `GOG.com\Games` and as the uninstaller (today only `unins000.exe` in the install folder).
- **Covers**: per-game `covers_are_low_quality` (Xbox games without a Poster still get a logo
  before SteamGridDB).
- **Sign-in follow-ups**: persist the Epic catalog cache on disk, GOG playtime and covers from the
  signed-in library, Epic genres, an option to hide Game Pass titles from Xbox title history.
- **Linux stores, follow-ups**: Heroic Amazon (Nile) and sideloaded games, owned-but-not-installed
  games from Heroic/Lutris service libraries.
- **IGDB metadata** for manual games and stores without a public API.

## Later

- Plugin host: WASM providers (wasmtime + WIT) and CSS/JS UI extensions; theme loader on top of
  `theme.css` variables.
- Fullscreen / controller mode (the grid keyboard navigation is the groundwork).
- Multiple play actions per game, emulator profiles, pre/post scripts.
- Roman numeral support in sorting names (Playnite's converter handles many edge cases).
- Minimize while playing.
- Cloud sync of the library database.
