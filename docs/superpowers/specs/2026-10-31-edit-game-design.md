# Edit game: all metadata, images and launch profiles: design

Status: ready for building. Step 3 of the game page work. Builds on step 1
(`2026-10-31-metadata-foundation-design.md`: per-source storage, user fields) and step 2 (the
page's Play ▾ menu). Approved mockup: `docs/mockups/edit-game.html` (all sections, the image
picker and its web search tab).

## Why

Store and IGDB data is sometimes wrong or missing, and the user wants to fix it by hand and keep
the fix. The user also wants several ways to start a game (different command line arguments)
with an editable default, and to change the cover, background and icon, picking from the images
the sources have, like Playnite does.

## Goals

The Edit game dialog becomes a larger dialog with sections listed on the left:

- **General:** name, sorting name (placeholder shows the automatic one), type (Game / App, from
  step 8 on), release date,
  description; then "You": completion status, your score (0–100 slider, "Clear"), categories,
  notes.
- **Details:** developers, publishers, genres, series, tags, features, platforms, age rating,
  game modes, perspective, themes (chip inputs with suggestions from existing values); critic
  score, community score, time to beat (three durations), install size.
- **Links:** all links with their source; hide/show a link; add, edit and remove your own.
- **Launch:** launch profiles (below).
- **Playtime tracking:** unchanged from today.
- **Images:** cover, background and icon (below).
- Footer: "Refresh metadata" (as on the page today), Cancel, Save.
- The section entry shows a count of edited fields ("2 edited").

### Edited values

- Every metadata field shows its source ("from IGDB", "from Steam", "automatic", "measured").
- Changing a field stores the value as the **"user" source**, which step 1 shaped as the top
  priority source; it survives every metadata refresh. The field then shows "edited by you ·
  Use IGDB value" (the source that would win without the edit); that link drops the user value.
- Clearing a list field to empty is an edit too ("no tags"), not a reset. (My call.)
- Hidden links and user links are stored per library entry; links that a later refresh no
  longer returns simply disappear, hidden flags for them are kept harmlessly. (My call.)

### Images

- Each of cover, background and icon is one row with the same layout: a fixed preview frame,
  title, one-line hint (where it is used and where the current one comes from), and the same
  buttons: **Choose file…**, **Pick image…**, **Remove** (icon: **Reset**, back to the
  executable's icon).
- Files: .png, .jpg, .webp; the icon also .ico, .exe and .dll (first icon in it, reusing the
  existing executable icon extraction).
- **Pick image…** opens the image picker: a grid of thumbnails with source filters ("All",
  each source with a count, "⌕ Web"). The current image is first with a check mark; selecting
  shows its size and source in the footer; "Use this image" applies it.
  - Sources: Steam (library cover, header/hero, screenshots), IGDB (cover, artworks,
    screenshots), SteamGridDB (grids, heroes, icons; only with a key). Sources answer in
    parallel and fill in as they arrive; sources without a key are left out.
  - **Web search** (user's request, like Playnite): a search box prefilled with "<name> cover",
    "<name> wallpaper" or "<name> icon", a size switch (any / large; icons ask for transparent
    images), results with size and site. Runs only when the tab is opened or Search is pressed.
    Uses **DuckDuckGo image search** through a hidden webview (the way the store-session window
    works today), reading its `i.js` JSON response; Google only as a fallback. (My call: Google's
    page changes and consent screen break scraping more often.)
- A picked or chosen image is copied into the image cache as the user's image and survives
  metadata refreshes like edited fields; Remove/Reset drops it.

### Launch profiles

- Each game has a list of **profiles**. Every store copy contributes one automatically (named
  after the store, e.g. "Steam", "Xbox app"); manual games have one from their launch options.
- **Default** (user's choice): until the user picks a default, Play behaves exactly like today
  (the first installed copy in the entry's play order, else the first copy) and the list shows
  "Automatic" as the default. Once the user clicks "Make default" on a profile (badge
  "DEFAULT"), Play always starts that one; if its copy is not installed, Play offers Install
  for it. "Use automatic" returns to today's behaviour. Stored as a nullable chosen default, so
  the migration changes nothing for existing games.
- **Add profile:** name; "Based on": a store copy (its launch plus **extra arguments**) or
  **Executable** (path, arguments, working directory, like today's manual launch options).
- Store profiles can be renamed and get extra arguments, not deleted; own profiles can be
  renamed, duplicated, deleted. Drag (and keyboard move up/down) to reorder.
- Extra arguments on a store launch go through a new optional store API method that returns the
  launch with arguments, or "not supported". Steam supports it (`steam://run/{id}//{args}/`);
  for stores that don't, the arguments field is disabled with the hint "This store can't pass
  arguments; base the profile on the executable instead." (My call.)
- **Play ▾ menu** (page header and command bar): lists all profiles (default marked), then
  "Edit profiles…" which opens this section. The **tray** keeps one click = play the default
  (user's choice); no profile submenu in the tray popup.
- Playtime tracking works the same for every profile.

## Non-goals

- Emulator profiles, pre/post scripts (ROADMAP "Later").
- Per-field source choice ("use Steam for this field"); only "your value" vs "automatic".
- Bulk editing of several games.
- Uploading images anywhere.

## Anti-goals

- Losing a user's edit or image on refresh, combine or app update.
- A refresh silently overwriting a field the user changed.
- A dialog that does network work before the user asks for it (the picker only loads when
  opened).

## Constraints

- Profiles need a schema change (append a migration step); existing games migrate to one
  profile per copy with today's default, so nothing changes for the user until they edit.
- Combining games keeps all profiles; separating a copy takes its store profile with it, own
  profiles stay with the entry. (My call.)
- The web search webview never shows a window and never takes focus; it has no access to the
  user's sign-in cookies of the store windows (separate data dir/profile). (My call.)
- From AGENTS.md: logic in core with tests; UI verified in the hidden test app; keyboard paths
  for every action; no literal colors/sizes outside `theme.css`.

## Left to the builder

- Table layout for profiles and user values (consistent with step 1's per-source storage).
- Chip input component details and value suggestions.
- Web search result count and thumbnail sizes.
