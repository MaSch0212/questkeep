# Roadmap

Ideas not built yet, roughly in priority order. Not a commitment.

## Done in v0.2

Epic, GOG and Xbox installed games; Steam owned games, Install action, imported Steam playtime;
store metadata and backgrounds (Steam, GOG); list view. Linux store support: Heroic/Legendary
(Epic, GOG) and Lutris installed games.

## Done in v0.3

Account sign-in for Epic and GOG (tokens in the OS keyring): owned games, Epic playtime, covers,
backgrounds and metadata.

## Next

- **Xbox account sign-in** (Microsoft OAuth + Xbox Live token chain) for owned games, covers and
  metadata; reuse `auth/` (`TokenStore`, `AccountSession`).
- **Sign-in follow-ups**: persist the Epic catalog cache on disk, GOG playtime and covers from the
  signed-in library, Epic genres.
- **Linux stores, follow-ups**: Heroic Amazon (Nile) and sideloaded games, owned-but-not-installed
  games from Heroic/Lutris service libraries.
- **Real covers for Epic and Xbox** (store catalog APIs or SteamGridDB); Xbox currently falls back
  to square package logos.
- **GOG: optional launch through Galaxy** (setting; the provider supports it) and the
  `Uninstall\<id>_is1` registry fallback for installs missing from `GOG.com\Games`.
- **IGDB metadata** for manual games and stores without a public API.
- Uninstall action per provider.

## Later

- Plugin host: WASM providers (wasmtime + WIT) and CSS/JS UI extensions; theme loader on top of
  `theme.css` variables.
- Fullscreen / controller mode (the grid keyboard navigation is the groundwork).
- Multiple play actions per game, emulator profiles, pre/post scripts.
- Roman numeral support in sorting names (Playnite's converter handles many edge cases).
- Tray icon, minimize while playing.
- Auto-update and release automation (signed installers).
- Cloud sync of the library database.
