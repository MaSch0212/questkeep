# Richer game metadata, filters and sorting: design

Status: ready for building. Step 1 of the game page work; the page redesign (step 2,
`2026-10-31-game-page-design.md`), editing all metadata, images and launch profiles (step 3,
`2026-10-31-edit-game-design.md`) and a first-run setup (step 4, `2026-10-31-first-run-design.md`)
follow in their own documents.

## Why

Questkeep stores five metadata fields per game (description, release date, developers,
publishers, genres) and views can only filter by store, category and a few flags, sort by four
fields and group by category or store. Playnite, the reference, knows much more about each game
(tags, features, series, scores, links, install size, ...) and lets the library be filtered,
sorted and grouped by all of it. The game page redesign (step 2) needs this data too, so it comes
first: richer data, stored so that views can use it.

## Goals

- New per-game data, filled automatically from the stores and IGDB:
  - **Lists:** tags, features (single player, co-op, multiplayer, controller support, VR, cloud
    saves, ...), series, age ratings, platforms, game modes, themes, player perspectives.
  - **Single values:** critic score, community score (0-100), install size (bytes), time to beat
    (three durations: main story, main + extras, completionist).
  - **Links:** name + URL (store pages, official site, wikis, PCGamingWiki, community hub, news,
    Reddit, Discord, socials, ...).
- New per-game data only the user sets: **completion status**, **user score** (0-100) and
  **notes**. This step only stores them and makes them filterable; editing them in the UI belongs
  to step 3, except completion status, which gets a simple picker on the game page now so it is
  usable.
- **Completion statuses are a user-editable list** (user's request, like Playnite):
  - Defaults: **Not Played**, **Played**, **Beaten**. The user can add, rename, reorder and
    remove statuses in settings (removing one in use asks which status those games get).
  - Settings: the status **new games** get (default: Not Played) and the status set when a game
    is **started for the first time** (default: Played; only applied while the game still has
    the new-game status). Either can be "none".
  - Existing games get Not Played if never played, else Played, by the migration. (My call.)
  - Sorting by status follows the list order; grouping shows one group per status.
- **Source priority** in settings: an ordered list of metadata sources the user can reorder.
  Default order: **IGDB, Steam, then all other stores**. For each field the first source in that
  order that has a value wins. Fields can have their own order (features and scores default to
  Steam first; see Decisions).
- A **"Refresh metadata for all games"** button in settings. Existing games do not get the new
  data on their own; new games still get metadata when they are added, as today.
- **Filters, sorting and grouping** over as much of the data as possible (see below), saved with
  views like the existing filters.

## Non-goals

- The game page redesign, player counts, news, achievements and weekly activity (step 2).
- Editing metadata by hand and launch profiles (step 3). The data model must not block them: a
  user-edited value must later be able to survive a metadata refresh.
- A Questkeep-run proxy for IGDB. IGDB stays optional with the user's own Twitch keys.
- Playnite's Version, Region, ROMs and scripts.

## Anti-goals

- Refreshing every game in the background after the update without being asked.
- Failing or showing errors when IGDB keys are missing. Without keys, the IGDB-only fields
  (website links, time to beat, game modes, themes, perspectives) simply stay empty.
- Duplicate values that only differ in spelling ("Co-op" / "Co-Op", "Single-player" /
  "Singleplayer") in the filter lists.
- A filter panel that becomes unusable with this many fields.

## Sources

| Data | Steam (no key) | GOG | Other stores | IGDB (user keys) |
|---|---|---|---|---|
| Tags | public store tags (tag ids + `IStoreService/GetTagList` names) | gameTags | - | keywords (not used as tags) |
| Features | store `categories` (Playnite's renames: "Steam Cloud" -> "Cloud Saves", "Steam " prefix dropped) | features | Xbox/Epic if their catalog has them | game modes (+ VR from perspectives) |
| Series | franchise association | - | - | collections / franchises |
| Age rating | - | - | - | age ratings (ESRB / PEGI) |
| Platforms | store `platforms` (win/mac/linux) | - | - | platforms |
| Game modes, themes, perspectives | - | - | - | game_modes, themes, player_perspectives |
| Critic score | Metacritic | - | - | aggregated_rating |
| Community score | review summary (% positive) | - | - | rating |
| Install size | - | store size | measured on disk for installed games | - |
| Time to beat | - | - | - (HowLongToBeat as fallback, see the game page spec) | game_time_to_beats |
| Links | store page, community hub, discussions, guides, news, PCGamingWiki, achievements and workshop when the game has them | store page, forum, PCGamingWiki | store page where a URL pattern is known | websites (named by website type) |

Link templates (from Playnite's Steam plugin): `store.steampowered.com/app/{id}`,
`steamcommunity.com/app/{id}`, `.../discussions/`, `.../guides/`, `.../workshop/`,
`store.steampowered.com/news/?appids={id}`, `steamcommunity.com/stats/{id}/achievements`,
`pcgamingwiki.com/api/appid.php?appid={id}`.

IGDB is matched exactly where possible: through `external_games` by the game's Steam app id (or
another store id IGDB knows), and by name only as a fallback, as today.

## Decisions

- **Priority per field, first source with a value wins** (user's choice: a configurable order,
  IGDB first by default). A list field takes the whole list from one source rather than merging
  lists across sources, so tags from Steam and keywords from IGDB never mix. **Links are the
  exception: merged from all sources**, de-duplicated by URL, because each source knows
  different ones. (My call.)
- **One global priority order plus per-field orders** (user's choice): the settings show the
  global list and, per field, an optional own order ("Use global order" / custom). Defaults:
  global IGDB, Steam, other stores; **features, critic score and community score default to
  Steam first, then IGDB, then the rest** (Steam has controller support, cloud saves, Metacritic
  and review percentages that IGDB lacks). Labels on the page and in the edit dialog name the
  real winning source (e.g. "Metacritic" via Steam).
- **Combined games** (one entry, several stores): every copy's store is a source; the priority
  order picks among them like among any other sources.
- **Normalised values**: list values are stored once and shared (like categories), compared
  case- and punctuation-insensitively, with a small alias table for the known variants
  (co-op, single-player, ...). (My call.)
- **Stored per source**, with the winning value computed from the priority order. Changing the
  priority then needs no re-fetch, and step 3 can add user overrides as one more, top-priority
  source. (My call; the builder may choose another layout if it gives the same two properties.)
- **The priority list has one entry per source**: IGDB and every registered store, each movable
  on its own. Default: IGDB, Steam, then the other stores in alphabetical order. New stores added
  later go to the end. (My call, reading "IGDB, then Steam, then all the others".)
- **Existing data keeps showing**: description, release date, developers, publishers and genres
  move into the per-source storage. Values already in the database become a "previous data"
  source at the very end of the priority list, so nothing goes blank before a refresh; a refresh
  replaces them with real per-source values. Developers, publishers and genres become normalised
  list fields like the new ones. (My call.)
- **"Has a value"** means a non-empty list or a non-null single value. A score of 0 is a value.
  Per source and game the database records when it was last asked and whether it answered, so
  "never asked" (for example IGDB without keys) is told apart from "asked, had nothing". (My call.)
- **User-only fields belong to the library entry**, not to a store copy. Combining games keeps
  the entry's values; separating a copy gives the separated game empty ones. The per-source
  storage is shaped so that step 3's hand edits are one more, top-priority source ("user") with
  no further migration. (My call.)
- **One vocabulary per list field across sources**: platforms are mapped to one canonical set
  (Windows, macOS, Linux, plus consoles named as IGDB names them); age ratings are stored as
  "PEGI 16" / "ESRB Teen" style values; features and tags go through the alias table. (My call.)
- **Steam needs no key**: tag ids come from `IStoreBrowseService/GetItems` (public) and tag names
  from `IStoreService/GetTagList` (public), both checked live on 2026-10-31.
- **Fixed ends of the priority list:** "user" (step 3's edits) is always first and "previous
  data" always last; neither is movable. Install size measured on disk ranks above store-reported
  sizes. Name and sorting name stay outside the source priority (the existing "renamed by user"
  handling stays). (My call.)
- **Missing keys:** the per-game "Refresh metadata" of a game with no other source keeps today's
  explanation ("Add IGDB credentials in Settings → Metadata…"); that is an explanation the user
  asked for, not an error from a background refresh. (My call.)
- **Refresh all** runs through the existing background metadata worker and its rate limits, with
  progress shown the way background downloads are today. Per-game "Refresh metadata" keeps
  working and fetches the new fields too.

## Filters, sorting, grouping

As many as possible, saved with views:

- **Filters, list fields** (match any / all of the chosen values, plus "none"): store, category,
  genre, tag, feature, series, platform, age rating, developer, publisher, game mode, theme,
  perspective, completion status.
- **Filters, ranges**: release year, playtime, play count, last played, date added, critic
  score, community score, user score, time to beat (main story), install size.
- **Existing filters stay**: favorite, hidden, installed, played.
- **Sort by**: name, release date, date added, last played, playtime, play count, critic score,
  community score, user score, time to beat, install size, completion status, developer,
  publisher, series.
- **Group by**: none, store, category, genre, tag, feature, series, platform, age rating,
  developer, publisher, game mode, theme, perspective, completion status, release year,
  playtime range, last played (time buckets), install size range, score range.
- Games with no value sort last and group under "None".
- **And/or groups** (user's request): the added filters combine with "all" (and) or "any" (or),
  and groups can be nested with their own all/any, so "A and (B or C)" works. The quick filters
  (store, installed, favorite, played, hidden) always combine with "and" with the rest. (My call.)
  A list filter itself is "any of / all of / none of" its chosen values.
- **Filter panel: approved mockup** `docs/mockups/filters.html`, variant **A** ("add filters"):
  - the quick filters stay at the top as today;
  - below them "Games must match all/any of these", then the added filter rows and groups;
    "＋ Add filter" opens a searchable field list grouped "About the game", "Scores & length",
    "You"; "＋ Add group" adds a framed group (its "⋯" menu: ungroup, duplicate; "×" removes);
  - a list filter row: any/all/none switch and value chips; "+ Add …" opens a searchable value
    list with game counts per value and a "No …" entry (games without a value);
  - a range filter row: from/to inputs, a slider and a few presets (for playtime: never,
    under 1 h, 10 h and more);
  - active filters also show as removable pills under the top bar ("Clear all" at the end);
    groups show in brackets with "or" between members.
  - Variant B in the file was rejected.
- **Sort and group menus: approved** (same file): the common fields first (sort: name, last
  played, playtime, date added, release date; group: none, store, category, genre, completion
  status), the rest under "More" / grouped submenus; ascending/descending stays in the sort menu.
- Order of work: data, fetching, SQL filtering/sorting/grouping and the settings first, then the
  panel and menus.

## Constraints

From AGENTS.md:

- Logic lives in `questkeep-core` with tests; store-specific fetching lives in the store crates
  behind `LibraryProvider` (the store API grows a richer metadata type; the core never matches on
  store ids). IGDB stays in the core.
- Schema changes are appended steps in `db/migrations.rs` (currently up to step 9).
- Bindings regenerate through `cargo test`; commit both.
- No literal colors or sizes outside `theme.css`; every action has a keyboard path.
- Library queries must stay fast with 5,000 games (the seed generator): filtering and sorting in
  SQL, not in the UI.

## Left to the builder

- Table layout, alias table contents, how completion status is shown on the game page for now.
- Exact range presets for playtime, size and score groups.
- Whether install size of installed games is measured on scan or lazily.

## Added after the library workflow spec (2026-10-31)

- **Explore** is built in this step together with the new filter panel: see
  `2026-10-31-library-workflow-design.md`, section 8 ("Explore"). The game page's clickable
  values (step 2) open it.
- The completion status list gets its **settings section** in this step (add, rename, reorder,
  remove; status for new games; status on first start).
