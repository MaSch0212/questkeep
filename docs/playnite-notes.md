# Playnite reference notes

Factual notes from reading Playnite (`E:\other\Playnite`) and PlayniteExtensions
(`E:\other\PlayniteExtensions`), both MIT licensed (Josef Nemec). Paths below are relative to
`E:\other`. These notes inform Questkeep's design; no C# is copied.

## Game model

- `PlayniteSDK/Models/DatabaseObject.cs`: `Id` is a `Guid` (new per game); equality by `Id`.
- `PlayniteSDK/Models/Game.cs`:
  - `PluginId` (Guid, `Guid.Empty` = custom game) + `GameId` (string, store id: Steam app id,
    Epic `AppName`). Together they identify a store game across re-imports.
  - `Playtime` and `PlayCount` are `ulong`; playtime is in **seconds**. `LastActivity`, `Added`,
    `Modified` are `DateTime?`.
  - `Favorite`, `Hidden`, `InstallDirectory`, `SortingName` (optional override), `IsInstalled`.
  - Image fields (`Icon`, `CoverImage`, `BackgroundImage`) hold a DB file id (relative path under
    `files/<gameId>/`), a local path, or an HTTP URL.
  - Runtime state (`IsRunning`, `IsLaunching`, …) is reset on load.
- `PlayniteSDK/Models/GameAction.cs`: a game has a list of actions.
  - `Type`: `File`, `URL`, `Emulator`, `Script`. Fields `Path`, `Arguments`, `WorkingDir`.
  - `TrackingMode`: `Default`, `Process` (process + children), `OriginalProcess` (only the
    started process), `Directory` (any process from a dir), `ProcessName`. `TrackingPath`.
  - `InitialTrackingDelay` (ms, default 0), `TrackingFrequency` (ms, default 2000).

**Questkeep takeaway:** UUID id + `(source, source_game_id)` unique key; playtime in seconds; one
`PlayAction` JSON object with the same tracking modes (minus emulator/script), extensible to a list.

## Storage

- LiteDB 4, one `.db` file per collection (`games.db`, `tags.db`, …) under
  `%AppData%\Playnite\library` (or `<exe>\library` in portable mode). Whole collections are loaded
  into memory at startup (`Playnite/Database/Collections/ItemCollection.cs`).
- Attached files (covers, icons) are copied into `library/files/<gameGuid>/<newGuid>.<ext>`
  (`Playnite/Database/GameDatabase.cs`, `AddFile`).

**Takeaway:** we use a single SQLite file with migrations and keep images in a cache folder with
paths stored relative to the data dir.

## Steam detection (`PlayniteExtensions/source/Libraries/SteamLibrary`)

- Install path: registry `HKCU\Software\Valve\Steam`, value `SteamPath` (forward slashes →
  backslashes) (`Steam.cs`). Playnite reads only HKCU. No Linux handling exists.
- Library folders (`Services/SteamLocalService.cs`): `<SteamPath>\steamapps\libraryfolders.vdf`.
  The Steam dir itself is always the first library. Two formats:
  - old: numeric key → path string (`"1" "D:\\Games\\Steam"`),
  - new: numeric key → object with a `path` key (case-insensitive).
  Only existing directories are kept.
- Manifests: every `appmanifest*` file in `<library>\steamapps` (skip names ending in `tmp`).
  Fields: `appid`/`appID`, `name` (fallback `UserConfig.name`), `installdir`, `StateFlags`.
  Keys are matched case-insensitively.
- Skip rules (complete list in Playnite):
  - `StateFlags` must include `FullyInstalled` (bit value 4).
  - App id `228980` (Steamworks Common Redistributables).
  - Install dir resolved as `<steamapps>\common\<installdir>`, falling back to
    `<steamapps>\music\<installdir>`; skipped if empty or under `steamapps\music` (soundtracks).
  - GoldSrc first-party mod folders: `bshift`, `cstrike`, `czero`, `dmc`, `dod`, `gearbox`,
    `ricochet`, `tfc`, `valve` (handled separately as mods).
  - No type-based tool/demo filtering for local manifests (that needs app info from Steam).
- Launch: `steam.exe -silent "steam://rungameid/<appid>"`.
- Images (`SteamShared/MetadataProvider.cs`): cover
  `https://steamcdn-a.akamaihd.net/steam/apps/<id>/library_600x900_2x.jpg`, landscape fallback
  `.../header.jpg`, hero `.../library_hero.jpg`.
- Last played locally: `userdata/<accountid>/config/localconfig.vdf` →
  `Software/Valve/Steam/apps/<appid>/LastPlayed`.

**Questkeep additions:** HKLM `WOW6432Node\Valve\Steam\InstallPath` fallback; Linux paths
(`~/.steam/steam`, `~/.local/share/Steam`, Flatpak). Since Steam tools (Proton, Steam Linux
Runtime, SteamVR redistributables) also have local manifests, we additionally skip a small,
documented set of known tool app ids.

## Epic detection (`PlayniteExtensions/source/Libraries/EpicLibrary`)

- Manifests: `%PROGRAMDATA%\Epic\EpicGamesLauncher\Data\Manifests\*.item` (JSON). Also
  `%PROGRAMDATA%\Epic\UnrealEngineLauncher\LauncherInstalled.dat` as install-location fallback.
- Fields: `AppName`, `DisplayName`, `InstallLocation`, `LaunchExecutable`, `CatalogNamespace`,
  `CatalogItemId`, `AppCategories`, `CompatibleApps`, `TechnicalType`.
- Filters: DLC (`AppCategories` contains `addons` without `addons/launchable`), UE plugins
  (`plugins`, `plugins/engine`, `CompatibleApps` starting with `UE_`), missing install dir.
- Launch: `com.epicgames.launcher://apps/{CatalogNamespace}%3A{CatalogItemId}%3A{AppName}?action=launch&silent=true`,
  tracked with `TrackingMode.Directory` on the install dir.

## Launch and playtime tracking

- `Playnite/Common/ProcessMonitor.cs`:
  - Process tree: each poll enumerates processes and adds any whose parent is in the tracked set;
    running while any tracked PID exists (children survive the root exiting).
  - Directory: any process whose main module path starts with the directory (with a trailing
    separator, so `C:\Fallout` does not match `C:\Fallout 2`), case-insensitive.
- `Playnite/Controllers/GenericGameController.cs` (`StartTracking`):
  - `Default` = directory tracking (install dir) for URL actions with a startup check, process
    tree for file actions.
  - Poll every `TrackingFrequency` (2 s). The startup check (waiting for the first process) has
    **no timeout**.
  - Session length accumulates loop iteration time; iterations longer than frequency + 30 s count
    as sleep/hibernate and are not added.
- `Playnite/GamesEditor.cs`: on start, `LastActivity = now` and `PlayCount += 1`; on stop,
  `Playtime += SessionLength`.
- App exit mid-session: no code path saves the running session; its time is lost.

**Questkeep improvements:** startup-check timeout for URI launches; persisted session rows with a
heartbeat so app exit or crash still records time; same sleep-gap rule.

## Sorting name (`Playnite/SortableNameConverter.cs`)

- Strip a leading article (`The`, `A`, `An`; case-insensitive).
- Pad standalone numbers to 2 digits (`4` → `04`).
- Convert Roman numerals to padded numbers with many exceptions (single `I` only at end or after
  chapter/season/episode, etc.).
- `one`/`two`/`three` at the end → `01`/`02`/`03`.

**Questkeep v0.1:** article stripping + number padding (Roman numerals deferred to the roadmap).
