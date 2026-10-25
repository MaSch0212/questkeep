# Library views and title bar rework: design

Status: approved mockup, design for planning. Mockup:
[`docs/mockups/menu-rework.html`](../../mockups/menu-rework.html) (open it in a browser; everything is
clickable, data is made up). Where this document and the mockup disagree, this document wins.

## Why

The sidebar mixes controls that behave differently:

- All games, Favorites and Hidden are views you switch between.
- "Installed only" is a toggle that stacks on top of them.
- The Sources list is a set of toggles.
- Sync, Add game and Settings sit in the same column.

People can't tell what a click will do. The rework makes the sidebar a list of **saved views**,
and only that. A view is a named set of filters, a sort order and a layout. Every other control
moves to a place that matches what it does. Games become easier to reach, too: a command bar
finds any game from anywhere.

## Goals

- One consistent model: picking a sidebar entry applies a saved view, and nothing else lives in
  the sidebar.
- Filters, sort and layout are set above the games. The current setup can be saved as a new
  view or written back to the open one.
- Users can rename, reorder, regroup and delete views and sections, and change their icons.
- The title bar holds the app-wide actions: Add game, Open launcher, Sync, Settings, and a
  command bar.
- List and details rows get smaller and show the game icon instead of the cover.

## Non-goals

- New filter fields beyond those listed below (genre, developer, year, playtime range and so on
  are ideas for later; see ROADMAP).
- Syncing views between machines, exporting them, or sharing them.
- Commands other than finding games in the command bar (for example "Open settings"). It is a
  game finder for now.
- Changing the grid view or the details panel itself, beyond the row changes below.

## Anti-goals

- Saving over a view by accident. A view changes only when the user picks "Update view".
- The keyword filter must never end up inside a saved view.
- Two always-visible search boxes.
- Hiding the actions behind gestures that nobody discovers. Every action that can be reached by
  right-clicking or in edit mode can also be found through the Questkeep menu.

## Constraints

From AGENTS.md:

- All logic goes into `questkeep-core`, with tests, and commands stay thin.
- Schema changes are appended steps in `db/migrations.rs`.
- No literal colors or sizes outside `theme.css`.
- Every action has a keyboard path, and buttons are labelled (`aria-label` or text).
- UI work is checked in the hidden running app, not only in jsdom tests.

## The design

### Title bar

From left to right:

- **Logo and "Questkeep" name.** This is a button that opens the **Questkeep menu**:
  - Edit views
  - Add game ›, a submenu with the same items as the Add game button
  - Open launcher ›
  - Sync ›, containing "Sync all" and then one entry per syncable store
  - Settings
  - Exit Questkeep
- **Add game.** An icon only (plus sign), opening the existing add-game choices as a menu:
  - Add a game manually…
  - Add from an executable…
  - one item per provider `add_game_option`, such as "New Minecraft instance"
- **Open launcher.** An icon only (rocket), opening a list of the store launchers installed on
  this PC. Picking one starts that launcher.
- **Sync.** An icon only, which spins while any sync runs and shows a small dot. Its popup:
  - shows when the last full sync ran
  - has a "Sync all" button
  - lists **every** store that can sync, always, with its status (when it last synced,
    "Syncing…", or the failure) and its own sync button
  - "Can sync" means every registered provider with `can_scan`. That is the set the sidebar's
    Sources list offers sync for today, and it includes stores without a sign-in, because they
    still import installed games. *My call.*
  - The last sync time of each store, and of the last full sync, survives restarts. Core records
    them when a scan finishes (a small DB table or a settings field) and exposes them through a
    command, so this is core work in phase 1. The last failure is kept in memory only, as today.
    *My call.*
- **Command bar,** centred in the title bar (see below).
- **Settings,** an icon on the right, next to the window buttons.
- The window buttons, unchanged.

Everything left of the command bar and between the buttons stays a window drag region.

### Command bar ("Find a game…", Ctrl+K)

- Focusing it opens a results panel under it. When the field is empty, the panel lists the
  5 most recently played games.
- Typing matches **all** games by name, whichever view is open; the match is case-insensitive.
  Hidden games are included but ranked last and marked "Hidden". The matched part of each name is
  highlighted. Up to 7 results are shown.
- Each result shows:
  - the game icon and name
  - the store icon
  - whether the game is installed
  - a **Play** button for installed games, or **Install** for the rest
- Keys:
  - ↑/↓ moves the selection
  - **Enter** opens the game's page (see below)
  - **Ctrl+Enter** plays or installs it
  - **Esc** closes the panel and clears the field

  Clicking a result opens its page; clicking its button plays or installs the game. A hint line
  at the bottom of the panel lists the keys.
- **Opening a game's page** means: switch to the library page if Settings is open, select the
  game, scroll it into view and open its details. If the current view doesn't contain the game,
  switch to the first view that does, or to "All games" (or the first view) as a fallback. The
  keyword filter is cleared first. *My call: the user asked for "jump to the page"; the details
  panel or details view is that page today.*
- If no view contains the game (a hidden game after the Hidden view was deleted, for example),
  open the first view and show the game's details anyway, without selecting it in the list. The
  store therefore needs a "focused game" that can sit outside `games()`. Whether a view contains a
  game is a pure function in `lib/` that matches a game against a view's filters. *My call.*

### Sidebar

- Sections of views. Each section has a heading. Each view has an icon, a name, and a count of
  the games it shows (the view's own filters, without the keyword).
- The active view is highlighted.
- **Unsaved changes are kept per view as a draft until the app closes** (decided by the user):
  - Changing the filters, sort or layout of a view and then switching away keeps those changes.
    Coming back shows them again.
  - Every view with a draft shows a small orange dot, not only the open one.
  - Reset or Update clears the draft. Drafts are kept in memory only.
  - The app always starts with the saved views and opens the first one.
  - Its game count in the sidebar follows the draft.

  The command bar switching views to reach a game leaves drafts alone.
- **Right-click on a view:** Rename, Change icon, Duplicate, Delete view, and then Edit views.
  Right-clicking empty sidebar space offers Edit views only. Rename switches to edit mode with
  that view's name selected.
- **Edit mode:** entered from the right-click menu or the Questkeep menu. A bar at the top reads
  "Editing views" and has a **Done** button. In edit mode:
  - every section and view gets a drag handle
  - names become text fields you can edit in place
  - each view has an icon button that opens the icon picker, and a delete button
  - each section has a delete button
  - a "New section" button sits at the end; it adds a section named "New section" and focuses
    its name

  Drag and drop moves a view within its section or into another one, and reorders sections. A
  line shows where the item will land. Keyboard path: Alt+↑/↓ on a focused row moves it, and
  moving a view past the edge of its section moves it into the next section. *My call.*
- Deleting a section that still has views asks for confirmation. The last remaining view can't be
  deleted. If the open view is deleted, the first view opens. *My call.*
- Changes made in edit mode are saved immediately. Done only leaves edit mode. *My call.*

### Toolbar above the games

From left to right:

- **Keyword filter.** A magnifier icon button. Clicking it, pressing Ctrl+F or `/`, or typing a
  printable character while the game list has focus opens it into a field with the placeholder
  "Filter this view…". The character you typed goes into the field.
  - While it narrows the list it stays open and tinted, and the count reads "**N** of M games".
  - It collapses when it loses focus while empty.
  - Esc, the × button, or switching views clears it and collapses it.
  - It filters by name only, it is never saved in a view, and it replaces today's `search` box.
  - It matches the way today's search does (`normalizeSearch`, case-insensitive).
- **Filters.** An icon button. When filters are active, the button widens and shows the count as
  a badge beside the icon. It opens a panel with:
  - **Store:** pick several stores; none picked means all
  - **Installed:** Any / Yes / No
  - **Favorite:** Any / Yes / No
  - **Played:** Any / Played / Never ("played" means the game has playtime or a play count)
  - **Hidden games:** Leave out / Include / Only

  The panel also has "Clear filters" and "Done".
- **Sort.** An icon button whose icon shows the direction. It opens one menu: Ascending and
  Descending with a check mark, a divider, then the fields (Name, Last played, Playtime, Date
  added) with a check mark. Picking a field keeps the current direction. *My call.*
- **View actions group.** It appears only while the view differs from its saved state, with
  space separating it from the controls before it. It is one bordered, orange-tinted group
  containing:
  - an orange dot and the label **Unsaved**
  - **Reset**, an undo icon that goes back to the saved state
  - **Save**, a bookmark-plus icon that opens a menu with **Update “<view>”** and **Save as new
    view…**
- Spacer, the game count, then the layout toggle (Grid / List / Details), all as today.

**Save as new view…** opens a dialog with:

- an icon picker button next to the **name** field
- a **section** picker
- a summary of what gets saved (filters, sort, layout)
- the note "The search box is not saved"

Saving creates the view and opens it.

### What a view holds

```
View      { id, sectionId, position, name, icon, filters, sort, descending, layout }
Section   { id, position, name }
filters   { stores: string[], installed: any|yes|no, favorite: any|yes|no,
            played: any|played|never, hidden: exclude|include|only }
layout    grid | list | details
```

Icons are names from a fixed set of about 24 Lucide icons. The mockup's set is a good start.

### Default views

These are created on first run and when upgrading:

- **Library:** All games (no filters, Name ascending, grid), Favorites (favorite = yes),
  Installed (installed = yes, list), Recently played (played, Last played descending, list)
- **Other:** Hidden (hidden = only, list)

For an existing user, the layout of All games is the one they use today (the `questkeep.viewMode`
local setting). The other defaults use the layouts above. *My call.*

"My views", "Backlog" and "Minecraft packs" in the mockup are only examples.

### List and details rows

- The list view and the details view's game list show the game's **icon** (`iconPath`) instead of
  the cover.
- Without an icon, they fall back to the store icon, or the initials placeholder used today.
- Row height goes from 52 to about 36px in the list, and from 44 to about 32px in the details
  list. The values become theme tokens.
- Grid tiles keep their covers.

## Decided by the user

- The sidebar holds only views, grouped in sections that the user can edit. The Sources section
  and the Installed-only toggle are removed.
- Filters, sort and layout are set in the toolbar and saved into views. The keyword filter is not
  saved, and switching views clears it.
- Add game, Open launcher and Sync go on the left of the title bar, as icons only. Settings goes
  on the right.
- Sync is one button. Its popup always lists every store that can sync.
- Edit mode instead of always-visible controls. Edit mode is reached by right-clicking or through
  the Questkeep menu, which holds every app-wide action.
- An icon picker for views, including in "Save as new view".
- The view actions (Unsaved, Reset, Save) form one group, set apart from the filter and sort
  controls. Save uses a bookmark-plus icon and opens Update / Save as new.
- The filter count sits next to the icon, not on top of it.
- A command bar in the title bar: arrows to select, Enter to open the page, plus play/install.
- A combined sort menu, as in Playnite.
- Unsaved changes stay with each view as a draft until the app closes, and the orange dot marks
  every view that has one.

## Left to the builder

- Where views live:
  - Recommended: two new SQLite tables (`view_sections`, `views`), added in a migration step that
    also inserts the defaults, with CRUD and reorder in core, exposed through thin commands.
  - Settings JSON would also work, but views are library data and need ordered edits.
- How `GameQuery` grows:
  - `installed_only` and `favorites_only` become tri-states.
  - `hidden` already exists.
  - Add a `played` filter.
  - `sources` already exists.
  - Keep `search` for the keyword filter.
- How the view counts in the sidebar are computed: a count query per view, or counting on the
  client from the full list, whichever is fast enough for about 10k games.
- How "Open launcher" works:
  - Add an optional provider method that reports whether the launcher is installed and starts it
    (a URI such as `steam://open/main`, or the executable).
  - Stores without a launcher (Manual, for example) don't appear in the list.
- The exact icon set, sizes, and the animation of the keyword field.
- Splitting the work into commits and phases. A sensible order:
  1. Core: views, migration, query.
  2. UI state and sidebar.
  3. Toolbar.
  4. Title bar and menus.
  5. Command bar.
  6. Rows.

## Docs to update when building

- DECISIONS: views stored in the DB; the keyword filter is never saved.
- ROADMAP: more filter fields; commands in the command bar.
- The AGENTS module map, if new modules appear.
- A changeset (minor).
