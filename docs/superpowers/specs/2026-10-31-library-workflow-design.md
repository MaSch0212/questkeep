# Library workflow: panel, selection, groups, columns, apps, statuses, backups: design

Status: ready for building. Steps 5–9 of this session's work, after the game page work (steps
1–4, see `docs/superpowers/queue.md`). Approved mockup: `docs/mockups/library-layout.html`
(all six screens: docked panel, floating panel, grid groups, apps launcher, multi-select,
columns).

## Why

Daily use of the library is jumpy and repetitive: the details panel appears and disappears and
reflows the list, nothing is remembered between restarts (selection, collapsed groups), group
headers are hard to see, there is no right-click menu and no way to act on several games. The
table shows fixed columns although the library now knows far more per game. Steam tools and other
software clutter the game list. Users also want their own completion statuses, control over
sort prefixes, and automatic backups.

## Order of work (each its own commit/changeset, user tests between)

5. **Panel and memory:** docked/floating details panel, splitter, remember selection, splitter
   width and collapsed groups per view.
6. **Groups and menus:** group header visuals, jump to group, game context menu, multi-select
   (panel + menu).
7. **Table columns.**
8. **Apps:** game/app type, Type filter, apps launcher, "Show all apps" (Explore itself comes in step 1).
9. **Settings items:** sort prefixes, automatic backups.

## Names used here

- "Table view" means the code's **List** layout. The **Details** layout (narrow list with the
  game page beside it) stays. In Details: group headers, jump to group, context menu,
  multi-select (the page side shows the multi-select summary) and a splitter between list and
  page (width remembered per view) apply; docked/floating does not (the page is always there);
  column choice does not (its list is narrow). (My call.)
- "Grid views" in the game page spec (step 2) means wherever the side panel shows: grid and
  table (List) views.

## 5. Details panel and per-view memory

- In grid and table views the details panel is **always shown** ("docked") while the view is
  open, also with nothing selected (empty state: "Select a game to see its details here."), so
  selecting a game never changes the list's width.
- **📌 Unpin** makes it **floating**: it appears over the list (right side, shadow) only while a
  game is selected and closes with Esc or ✕; the list keeps its full width. 📌 docks it again.
- **Splitter** between list and docked panel: drag (and keyboard: focus + arrow keys) to resize,
  width shown while dragging; min 320px, max half the content width. (My call.) The floating
  panel uses the same width.
- When the window is too narrow for list + docked panel (today's split breakpoint), the docked
  panel behaves like the floating one until the window is wide enough again. (My call.)
- **Remembered per view, across restarts:** docked/floating, panel width, last selected game,
  collapsed groups (per view *and* grouping field). Explore remembers none of these (it starts
  fresh each time it is opened from the sidebar). (User's request for selection and collapsed
  groups; panel state my call.)
- Storage: per-view UI state in the database next to the views (not in settings.json), written
  debounced. A remembered game that no longer exists or is filtered out is ignored.
- The selected game is scrolled into view when a view opens.

## 6. Group headers, jump to group, context menu, multi-select

- **Group header** (grid and table): full-width band with an accent bar, chevron, store icon when
  grouped by store, larger name, a count chip and a short summary (installed count, total
  playtime). Sticky while scrolling, as today. Visual values via `theme.css` variables.
- **Jump to group** button next to the group button, only while grouping is active: a menu with
  a type-to-find box, every group with its count, then "Collapse all" / "Expand all". Choosing a
  group scrolls it to the top (expanding it if collapsed) and focuses its first game. Keyboard
  shortcut: Ctrl+G. (My call.)
- **Context menu** on a game (right-click, Menu key, Shift+F10), single game: Play, Play with ▸
  (profiles, from step 3), Open game page, favorite, Completion status ▸, Categories ▸, Edit…
  (F2), Refresh metadata, Combine with another game…, Mark as app / Mark as game, Hide (Del),
  Install / Uninstall…, Open install folder, Remove from library…. Reuses the details panel's
  "⋯" menu entries where they exist.
- **Multi-select:** Ctrl+click toggles, Shift+click selects a range (in the view's visual
  order across groups), Shift+arrows extend, Ctrl+A selects all visible, Esc clears. Right-click
  on an unselected game selects only that game first. Group headers show "N of M selected".
  - **Multi-select panel:** cover stack (at most 8 covers in selection order, fanned to fit the
    panel width, fewer when narrow, "+N" badge for the rest), "N games selected" with counts per
    store, "Clear selection", totals (installed, playtime, install size), **Change all**
    (completion status, categories add/remove, favorite, hidden, type; "Mixed" when values
    differ), actions: Install all, Refresh metadata, Combine into one game, Remove from library….
  - **Multi-select context menu:** same actions; Edit is disabled.
  - Bulk actions that change data ask for confirmation only when destructive (remove), as today
    for single games. Bulk changes are one database transaction.
  - "Combine into one game" uses the existing combine logic (ROADMAP: "a bulk 'combine selected'
    once multi-select exists") with a **combine screen** (user's request; mockup
    `docs/mockups/combine.html`), also used by "Combine with another game…" for two games:
    - one column per game (the game headers stay visible while scrolling), one row per value
      that differs between them; rows where all agree
      or all are empty are left out;
    - single values (name, cover/background/icon, edited details, completion status, your
      score, favorite, default profile) are radio choices; values that can be combined
      (categories, notes, your links, own launch profiles) are checkboxes, all checked by
      default; notes are joined with a blank line;
    - suggested defaults: the game with the most playtime is the base (name, images, edited
      details, default profile); the most advanced completion status (latest in the status
      list); score from the most-played game that has one; favorite if any is; "Reset to
      suggested" restores them;
    - **added up:** playtime is summed, all play sessions are kept, last played is the latest,
      date added the earliest;
    - store copies of all games are kept, as today.

## 7. Table columns

- The table header stays visible while scrolling (as today), above the sticky group header.
- Right-click the table header: a searchable list of every column with checkboxes; shown columns
  listed first in their order and draggable; Name always first and always shown; "Reset to
  default columns".
- Columns: every field from step 1 that has a single display value or a short list (lists shown
  comma-separated, truncated with tooltip), plus today's columns.
- Drag a header edge to resize, drag a header to move, click a header to sort by that field
  (click again toggles direction); this sets the view's sort like the sort menu does.
- Columns, order and widths are part of the view (saved with it, "Unsaved" like other view
  changes); Explore starts with the default columns. Default columns are today's.

## 8. Apps, Explore and the apps launcher

- Every library entry has a **type**: Game or App.
  - Detection per store in the store crates (new optional field on imported games): Steam from
    the store's app type (software/tool; verified live via `IStoreBrowseService/GetItems` or
    appinfo; the builder records the source in DECISIONS), Xbox from the catalog's product kind,
    Epic from its catalog categories; everything else defaults to Game.
  - A user override ("Mark as app" / "Mark as game") wins and survives scans.
- Every view gets a **Type** filter (Games / Apps / Both), default **Games** for existing and new
  views, so apps disappear from game views without changing the user's views otherwise. It is a
  **quick filter** (always "and", next to installed/favorite). "Games" is the default value, so
  it shows no pill and does not mark migrated views "Unsaved". (My call.)
- **Apps elsewhere** (my calls): left out of the start page shelves and stats; shown in the tray's
  recent list (marked as app) and the command bar; included in duplicate detection; no IGDB
  lookups (store metadata only); no completion status (the status field is empty and hidden for
  apps, and the first-start rule does not apply).
- The type can also be changed in the Edit dialog (General section, "Type: Game / App").
- **Apps launcher** in the title bar (next to the store-launcher button): a popover with a find
  box and app icons (most used first, two rows max); click starts the app, right-click opens the
  context menu; "Show all apps" opens Explore filtered to Type: Apps. Hidden apps are left out.
- **Explore is built in step 1** (with the new filter panel), not here, so the game page's
  clickable chips (step 2) can open it from the start; step 8 only adds "Show all apps". (My
  call, resolving the step order.)
- **Explore** (compass icon), the last entry at the very bottom of the sidebar: an unsaved view.
  Opening it from the sidebar always starts fresh: table view, default columns, no filter, sort
  or grouping. "Save as view…" turns its current state into a normal view. Other features open it
  preset: "Show all apps" (Type: Apps), and clicking a chip or detail value on the game page
  (step 2) opens Explore filtered by that value. (User's request and name.)
- Apps play, track playtime and appear in the command bar like games (marked "App").

## 9. Settings items

- **Completion statuses** are built completely in step 1, including their settings section; not
  part of this step. (My call.)
- **Sort prefixes:** today `model/sorting.rs` always strips "the", "a", "an". This becomes a
  setting: a list of words ignored at the start of names when sorting (default: The, A, An),
  editable, plus an on/off switch. Changing it recomputes all sorting names (a quick background
  job); names the user typed as their own sorting name (step 3) are kept.
- **Automatic backups:**
  - Contents: the library database (consistent copy via SQLite's backup API), `settings.json`
    and the `images` folder (covers, backgrounds, icons, user images), as one compressed archive
    (zip) named with date and time. Not included: the cache, logs, webview data and keyring
    secrets (keys and tokens); the restore screen says that keys must be entered again.
  - Settings: on/off (**on by default** for new and existing users), folder (default
    `<data dir>/backups`), frequency (daily, **weekly** (default), on every start), how many
    backups to keep (**5** by default; older ones are deleted), "Back up now", and the last
    backup time and size. (User's choice.)
  - Runs in the background at low priority, once the app is idle after start when due; failure
    shows a toast and is retried at the next start.
  - **While a backup runs** (user's request): a status shows it (in the title bar's background
    activity area, like syncing, with progress), and quitting the app (close with tray off,
    tray "Quit", installing an update) asks "A backup is running. Quit anyway?" with "Wait and
    quit" (quits when it finishes) and "Quit now" (cancels it and deletes the partial file).
  - **Restore:** a list of backups in the chosen folder (date, size, app version) with
    "Restore…". Restoring asks for confirmation, makes a backup of the current state first,
    restarts the app, and replaces the database, settings and images on start before anything
    opens them. The backup settings themselves (on/off, folder, frequency, count) are kept from
    the current install, not taken from the backup. (My call.) A backup from a newer app version is refused with a clear message; an older one
    is migrated as usual.
  - The existing pre-migration backups (`db/backup.rs`) stay as they are.

## Non-goals

- Multi-select in the home page shelves or the command bar.
- Column selection for the grid view (it has no columns).
- Cloud backup or backup to anything but a folder; encrypting backups.
- Automatic app detection for stores that don't expose a type.

## Anti-goals

- The list jumping when a game is selected (the reason for the docked panel).
- Losing a user's view settings when the defaults change; existing views must look the same
  after the update except that apps move out.
- A restore that can leave a half-replaced data dir: restore works on a copy and swaps it in
  only when complete.
- Backups that block the UI or run while a game is being launched.

## Constraints

From AGENTS.md: schema changes as appended migration steps; logic in core with tests (range
selection, group summaries, column definitions, prefix stripping, backup rotation and restore);
pure UI logic in `lib/` with vitest; keyboard paths for every action; no literal colors/sizes
outside `theme.css`; UI verified in the hidden test app (both docked and floating, grid and
table). Tests of backups and restore run against the throwaway portable data dir only.

## Left to the builder

- Exact group summary text, empty-state art, animation of the floating panel.
- Debounce timing for UI state writes.
- Column default widths.
