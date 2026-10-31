# Session work queue

Work through this list in order without asking, unless an item says otherwise. After each
feature: test in the hidden test app, ask the user to test, commit and push on approval, with a
changeset where it is user-visible. Specs and mockups are the source of truth for details.

## Game page work (builder-check questions answered on 2026-10-31)

1. Metadata foundation, filters, sorting, grouping:
   `specs/2026-10-31-metadata-foundation-design.md`, mockup `docs/mockups/filters.html` (variant A).
2. Game page redesign: `specs/2026-10-31-game-page-design.md`, mockup `docs/mockups/game-page.html` (variant A).
3. Edit game, images, launch profiles: `specs/2026-10-31-edit-game-design.md`, mockup `docs/mockups/edit-game.html`.
4. First-run setup: `specs/2026-10-31-first-run-design.md`, mockup `docs/mockups/first-run.html`.

## Library workflow (steps 5–9; combine screen mockup `docs/mockups/combine.html` approved)

5–9. `specs/2026-10-31-library-workflow-design.md`, mockup `docs/mockups/library-layout.html`.

## More items from the user (2026-10-31, verbatim; specced in the library workflow spec)

- Add game context menu
- Remember last selected game in each view (also between application restarts)
- Remember the expanded/collapsed state of groups per view, also between restarts (added later;
  today it is kept per view only in memory). Stored per view and grouping field (so "Store" and
  "Category" groupings keep separate states). Explore does not remember it (it starts fresh).
- Always show game sidebar with option to unpin it (which then floats) to avoid jumping around
  when selecting a game
- Add splitter for game sidebar/drawer (remember splitter width per view)
- Improve visuals of group headers - currently hard to see in table and list view
- Add jump-to-group button when grouping is enabled
- Add completion status (similar to playnite) that can be selected on the game page - add default
  ones but allow to add and remove states
  - Default status configurable for newly added games
  - Status configurable that is set when starting a game for the first time
  - Default statuses: Not Played (default), Played (first time), Beaten
  (This replaces the fixed completion set in step 1's spec.)
- Add sorting prefix-removal (e.g. ignore "The" or "A" from the start while sorting) -
  configurable in settings
- Add Automatic backup functionality with manual restore - backing up all data, settings and
  images (compressed to a configurable folder) - configurable frequency and rotation
- Differ between Apps and Games (Steam includes a couple of Apps, not sure how yet, but maybe
  exclude them from the game list and add an application launcher to the title bar)

## Decisions on the new items (2026-10-31)

- Mockup `docs/mockups/library-layout.html` (docked/floating panel, splitter, group headers,
  jump to group, context menu, apps launcher, multi-select, columns): approved.
- Apps: every view gets a Type filter (Games / Apps / Both, default Games). "Show all apps" in the
  title-bar apps launcher opens **Explore** filtered to Type: Apps (user's idea).
- **Explore** (user's request and name, compass icon; called "temporary view" at first): one entry at the very bottom of the sidebar; clicking it
  always starts fresh (table view, no filter, sort or grouping); "Save as view…" turns it into a
  normal view. Other features (like "Show all apps", clickable chips on the game page) may open
  it preset with filters.
- **Multi-select** (user's request): Ctrl+click toggles, Shift+click selects a range, plus
  Shift+arrows, Ctrl+A, Esc clears. A special multi-select details panel (cover stack, counts per
  store, totals, "Change all": completion status, categories, favorite, hidden, type, with
  "Mixed"; the cover stack shows at most 8 covers in selection order, fanned to fit the panel
  width (fewer when the panel is narrow), with a "+N" badge for the rest; actions: install all, refresh metadata, combine into one game, remove) and a
  multi-select context menu. Right-click on an unselected game selects only that game. Group
  headers show "N of M selected". Mockup: "Multi-select" in `docs/mockups/library-layout.html`.
- **Table columns** (user's request): right-click the table header to choose columns from every
  field (searchable, checkboxes, drag to reorder; Name always first); drag column edges to
  resize, drag headers to move, click a header to sort. Columns, order and widths are saved per
  view; Explore starts with the default columns. Mockup: "Columns" in
  `docs/mockups/library-layout.html`.
- Library layout mockup approved by the user (2026-10-31); the "Columns" screen too.

## How to work while the user is away (user's choice, 2026-10-31)

- Build in the order above. Each step goes on its own branch (`feat/<step>`), stacked on the
  previous step's branch; nothing is merged or pushed to main until the user has tested it.
- When back, the user tests the steps in order; on approval merge into main and push, with a
  changeset per user-visible step.
- Time to beat: IGDB first, HowLongToBeat (unofficial) as fallback (game page spec).
