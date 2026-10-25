# Roadmap

Ideas not built yet, roughly in priority order. Not a commitment.

## Done in v0.1 (first public release)

Steam, Epic, GOG and Xbox installed games; Steam owned games, Install action, imported Steam
playtime; store metadata and backgrounds (Steam, GOG); list view. Linux store support:
Heroic/Legendary (Epic, GOG) and Lutris installed games. Account sign-in for Epic and GOG (tokens
in the OS keyring): owned games, Epic playtime, covers, backgrounds and metadata. Tray icon with a
native popup, start with the system, optional GOG launch through Galaxy, signed auto-updates and
the release workflow.

## Done for v0.2

Xbox account sign-in (owned PC games, playtime); Xbox covers, backgrounds and descriptions from the
Microsoft Store catalog; Epic covers without sign-in (public catalog); Uninstall action per store;
custom title bar. Follow-ups: Legendary per-game uninstall; GOG `Uninstall\<id>_is1` installs and
uninstallers; per-game low-quality covers; Epic catalog cache on disk; GOG gamesdb covers; option
to skip Game Pass titles; Heroic Amazon (Nile) and sideloaded games; owned games
from Heroic/Lutris libraries; IGDB metadata for games without store data.

Also: GOG playtime from GOG Galaxy's local database; IGDB fills missing genres (Epic genres).

## Next

- **Verify on Linux**: Heroic Amazon (Nile) and sideloaded games, owned games from Heroic/Lutris.
- **GOG playtime without Galaxy** (Linux/Heroic): no web endpoint accepts the Galaxy token.
- **Ubisoft and Amazon follow-ups**: try launch/install/uninstall live; tray entries for both
  launchers. Verify the Amazon and Steam sign-ins with real accounts (built from Playnite's flows,
  not yet signed in live); Ubisoft owned games would need an account API Ubisoft doesn't offer.
- **Battle.net and EA sign-ins**: verify with real accounts (built from Playnite's and Lutris'
  flows, not signed in live). Check the product codes of rarer Battle.net titles (only
  `WTCG`/`Fen`/`VIPR` are mapped specially) and EA installs through `origin2://`.
- **EA app**: try the `origin2://` / `link2ea://` launch path for games the plain executable does
  not start.

- **Launchers**: verify "Open launcher" on real installs for EA (registry command), Ubisoft
  (InstallDir), Battle.net and the Xbox app.
- **More view filters**: genres, tags, platforms, release year, playtime ranges; saved per view.
- **Categories follow-ups**: Heroic categories; assign categories to several games at once; more
  group-by fields (genre, release year); map a single store category onto an existing category
  without merging whole categories; check `sharedconfig.vdf` tags on an older Steam install.
- **Combined games follow-ups**: smarter duplicate matching (editions, "GOTY", subtitles);
  edit launch options per copy; a bulk "combine selected" once multi-select exists.
- **Command bar commands**: run app actions (sync a store, open settings, switch views) from
  Ctrl+K, not only games.

## Blocked

- **Per-game Epic launcher uninstall**: no URI exists; revisit if Epic documents one.
- **Game Pass by entitlement**: the entitlement API is for publishers only (see DECISIONS).

## Later

- Plugin host: WASM providers (wasmtime + WIT) and CSS/JS UI extensions; theme loader on top of
  `theme.css` variables.
- Fullscreen / controller mode (the grid keyboard navigation is the groundwork).
- Multiple play actions per game, emulator profiles, pre/post scripts.
- Roman numeral support in sorting names (Playnite's converter handles many edge cases).
- Minimize while playing.
- Cloud sync of the library database.
