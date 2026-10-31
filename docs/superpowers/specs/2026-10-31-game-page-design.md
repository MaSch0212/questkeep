# Game page redesign: design

Status: ready for building. Step 2 of the game page work. Builds on step 1
(`2026-10-31-metadata-foundation-design.md`), which supplies the richer metadata and links.
Approved mockup: `docs/mockups/game-page.html`, **variant A** (variant B, tabs, was rejected).

## Why

The game page has the immediately useful facts (play, last played, playtime, stores) but little
else; it feels empty. The user wants more details on it: check DLCs on the store page, open the
wiki, read news, see scores, time to beat, achievements and their own activity. Playnite's page is
the reference for *which* information to show, not for the layout.

## Goals

The page from the mockup, in both places it is shown: the full game page and the narrow side
panel of grid views (the mockup's "Side panel (380px)" switch shows the narrow layout).

- **Header:** as today (cover, name, Play with its ▾ menu, last played, playtime, stores, favorite,
  hide, edit, more), plus:
  - a **"playing now"** badge with the current player count (Steam games only);
  - a **completion status picker** (from step 1), shown next to the facts.
- **Links row:** one line of link chips (icon + name) from the merged links of step 1. Never
  wraps; when the chips do not fit, the last ones collapse into "+N more" (a menu with the rest).
  Opens links in the default browser.
- **Wide column:**
  - **About:** description, then feature chips and tag chips. Chips are clickable and open the
    library (**Explore**, built in step 1) filtered by that value (my call; Playnite does the same).
  - **News:** the latest 5 items: thumbnail when the item has an image, title, date and source,
    a two-line snippet; clicking opens the item in the browser. "Open all news ↗" links to the
    store's news page.
  - **Details:** released, developer, publisher, series, platforms, game modes, perspective,
    themes, age rating, install size, date added, play count. Values are clickable filters like
    the chips.
- **Side column:**
  - **Scores:** critic (with its source name, e.g. Metacritic) and community (e.g. Steam) as rings.
  - **Time to beat:** a segmented bar for main story / main + extras / completionist, with a
    "You · 146h" marker above the bar at the user's playtime (right-aligned label when the marker
    is near the end), and the source ("From IGDB" or "From HowLongToBeat").
    - **Sources** (user's choice): IGDB first; when IGDB has no time to beat (or no key is set),
      HowLongToBeat's unofficial search is tried (matched by name and release year, like
      Playnite's HowLongToBeat extension). HLTB is a "time to beat"-only source in step 1's
      per-source storage, after IGDB in that field's order. If HLTB changes and the request
      fails, the section is hidden and the log says why; no error is shown.
  - **Achievements:** "34 of 72", a progress bar, a grid of unlocked icons (most recent first)
    and the rarest unlocked one with its global percentage.
  - **Your activity:** hours played per week for the last 12 weeks as bars, with totals: last 12
    weeks, this week, number of sessions.
  - **Library:** categories, stores (each copy) and install path.
- **Narrow layout** (container width ≤ 700px, as in the mockup): cover beside title; Play
  stretches over the full width with icon-only tools; the facts on their own line (stores
  hidden); links icon-only (name as tooltip); one column in this order: About, Scores, Time to
  beat, Achievements, Your activity, News, Details, Library. The achievement grid shows one row.
- **Sections without data are hidden**, not shown empty (no news, no achievements, no scores,
  no time to beat, no activity in 12 weeks). The page must still look complete for a manually
  added game with no metadata at all.

## Non-goals

- News from stores other than Steam in this step: Steam has a public news API; for other stores
  only the "News" link (step 1 links) is shown. Other sources can be added later behind the same
  store API method.
- Achievements for stores other than Steam and Xbox.
- Editing anything except the completion status on the page (step 3 is the Edit dialog).
- Showing achievement details per achievement (names, descriptions) beyond tooltips on the icons.

## Anti-goals

- A slow page: everything live (player count, news, achievements) loads after the page shows,
  section by section, and never blocks the header or Play.
- Network calls for games nobody looks at: live data is fetched only when a game page or side
  panel opens, never for the whole library.
- Errors in the page for missing keys or offline: a section that cannot load is hidden; the log
  has the reason.

## Data

- **Player count:** Steam `ISteamUserStats/GetNumberOfCurrentPlayers/v1/?appid=…` (no key).
  Only for games with a Steam copy. Cached in memory for 10 minutes. (My call.)
- **News:** Steam `ISteamNews/GetNewsForApp/v2/?appid=…&count=5&maxlength=300` (no key). The
  snippet is the `contents` with BBCode/HTML stripped; the thumbnail is the first image in the
  item when there is one (Steam's `{STEAM_CLAN_IMAGE}` placeholder resolves to
  `https://clan.akamai.steamstatic.com/images/`). Cached in memory for 30 minutes. "Open all news"
  links to `store.steampowered.com/news/app/{id}`.
- **Achievements:**
  - Steam: the user's unlocked achievements need the account (`ISteamUserStats/GetPlayerAchievements`
    with the Steam API key from the Steam store settings, or the signed-in session's web token if
    Steam accepts it there); the schema with names and icons needs `GetSchemaForGame` (key);
    rarity comes from `GetGlobalAchievementPercentagesForApp` (no key). Without a working
    credential the section is hidden. The builder checks live which calls the session token
    works for and records it in DECISIONS.
  - Xbox: the signed-in Xbox account's achievements endpoint (Playnite's Xbox plugin uses
    `achievements.xboxlive.com`), only for games with an Xbox copy.
  - Combined games: the first copy (in source priority order) that returns achievements wins.
  - Cached on disk per game with the time fetched; refreshed when the page opens and the cache is
    older than 1 hour, and after a play session of that game ends. (My call.)
- **Activity:** from `play_sessions`: seconds per ISO week (Monday start, local time) for the last
  12 weeks, a session spanning midnight split by day. Computed in SQL/core, not in the UI.
- **Live data lives in the store crates** behind new optional `LibraryProvider` methods (e.g.
  `player_count`, `news`, `achievements`), each returning "not supported" by default, so the core
  never matches on store ids. The core picks which copy to ask. One command per section so they
  load independently.

## Decisions

- Live sections load independently and show a quiet skeleton while loading; they disappear if
  the result is empty. (My call.)
- The page does not poll; reopening the page or switching games refreshes from the caches above.
- The completion picker uses the step 1 field and writes it immediately (no Save).
- Keyboard: the links row, the chips, the news items and the "+N more" menu are reachable by Tab
  and open with Enter.

## Constraints

From AGENTS.md: no literal colors/sizes outside `theme.css`; labeled buttons; keyboard paths for
every action; logic in core with tests (activity buckets, news text cleaning, link collapsing as a
pure UI helper in `lib/` with vitest); UI verified in the hidden test app, both widths.

## Left to the builder

- Ring and bar styling details within the mockup's look; skeleton style.
- Which Steam/Xbox endpoints are used exactly, as long as the anti-goals hold.
- How "+N more" measures (ResizeObserver or similar).
