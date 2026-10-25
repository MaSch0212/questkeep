# Game details page redesign

## Why

The game details (`ui/src/components/DetailsPanel.tsx`, shown as the side panel/flyout and as
the full page of the "details" view) look rough:

- On wide windows the content is one narrow column; most of the space is empty.
- The structure is not intuitive: some things are edited in place (name pencil, cover buttons,
  launch options form, categories, toggles), others are read-only, and buttons for rare actions
  (refresh metadata, refetch artwork, combine, uninstall, remove) are scattered down the page.

References the user looked at: Steam, Playnite, Xbox app. The user picked the Playnite-like
direction ("B").

## Goals

- Fill the width with a clear structure: header (art, cover, title, actions), then info cards.
- One clear editing model (the user's decision):
  - **Edited directly on the page:** favorite, hidden, categories.
  - **Everything else is in an Edit popup** opened from an Edit button.
- One layout that works from the narrow side panel up to a wide details page.

## Non-goals

- No hand-editing of metadata (description, genres, release date, developer, publisher) now. The
  user wants that later; add it to `docs/ROADMAP.md`. The popup should have room to grow (fields
  below Name).
- No core, command or database changes. Everything the page needs already exists.
- No new features beyond "Open install folder" (see below).

## Anti-goals

- Editable and read-only things mixed again (e.g. inline rename, cover buttons on the page).
- A layout that only looks right at one width.
- Losing any action the page has today, or its keyboard path.

## Layout (option B)

Mockups: `.superpowers/brainstorm/…/content/layout-b.html` and `edit-dialog.html` (throwaway;
the description here is authoritative).

### Header

- Background art (`store.backgroundUrl`) across the full width, fading into the page background.
  Without art, a plain header band (as today when the hero fails).
- The cover overlaps the art on the left (`Cover` component).
- Next to the cover: the title (plain text, no pencil), a muted line "Developer · release year"
  (each part only when known), then the **action row**:
  - **Play** split button exactly as today (Install when not installed, "Running…" while
    running, ▾ "Choose a store" menu for combined games).
  - Stats: Last played, Playtime, Stores (store icons + names). Labels small and muted, values
    below them.
  - Right-aligned: **Favorite** and **Hidden** toggle buttons (one click, `aria-pressed`, as
    today), an **Edit** button, and a **⋯ More** button.
- The ⋯ menu (`Menu`/`Popover`), in this order:
  - Refresh metadata
  - Refetch artwork
  - Combine with another game… (opens the existing `CombinePicker`)
  - Open install folder (new; only when an install dir is known; see "Calls settled" below)
  - separator
  - Uninstall… (only when it can be uninstalled today; keeps its confirmation)
  - Remove from library… (keeps its confirmation)
  - Uninstall and remove are styled as dangerous.

### Cards

Below the header, a grid of cards that fills the width:

1. **About** (widest): the description with Show more/Show less, the genre chips under it.
   Hidden when there is neither a description nor genres.
2. **Details** (read-only list): Released, Developer(s), Publisher(s), Date added, Play count.
   Rows without a value are left out.
3. **Library**:
   - **Categories**: the existing `CategoryEditor` (chips with remove, add).
   - **Stores**: one row per copy with store icon, name and state ("Installed · preferred",
     "Not installed"). For combined games each row has a small ⋯ menu with "Play this store
     first" and "Separate". This replaces today's `section.copies` buttons.
   - The install directory, muted, below the stores (or "Not installed").

### Narrow widths (side panel, small windows)

Same component, same order. As space runs out: the cover gets smaller, the action row wraps
under the title, and the cards stack in one column. Exact breakpoints are the builder's call;
use container queries or the existing `details--split`/`details--page` classes, whichever fits.
The panel keeps its close button.

## Edit popup

A `Modal` titled "Edit game", opened by the Edit button.

- Left: cover preview with **Change…** and **Remove** (Remove disabled without a cover).
- Right: **Name**, then a **Launch options** section (target path, arguments, working directory
  with the existing browse buttons) only for manually added games, as today.
- Footer: **Cancel** and **Save**.
- **Nothing applies until Save** (my call), including a picked or removed cover. Cancel, Esc and
  a backdrop click discard. Save with an empty name is blocked with the existing error message.
  Errors from saving show in the popup and keep it open.
  - Today `changeCover` (`lib/actions.ts`) calls `pickImage()` and then `api.setCover(id, path)`.
    The popup calls `pickImage()` on Change… and `api.setCover(id, path | null)` on Save. The
    preview of a picked, not yet saved file: show it if the webview can load the local file
    (asset protocol), otherwise show the file name under the old cover (builder's call).
- `LaunchOptionsForm` is reused or folded in; it should no longer have its own save button
  inside the popup.

## Keyboard

- Every action reachable by Tab; menus as the existing `Menu` does them.
- **F2** (today: inline rename) opens the Edit popup with the Name field focused (my call).
- Focus returns to the Edit/⋯ button when the popup/menu closes.

## Implementation notes

- UI only: `DetailsPanel.tsx` (split into smaller components where it helps, e.g. an
  `EditGameDialog.tsx`), `LaunchOptionsForm.tsx`, `app.css`, `theme.css` (all new sizes and
  colors as `--qk-*` tokens). Tests: update `DetailsPanel.test.tsx`, `DetailsView.test.tsx`,
  `Combined.test.tsx`, `Categories.test.tsx` for the moved controls; add tests for the popup
  (save applies, cancel discards, F2).
- Verify in the hidden app (`scripts/dev-hidden.ps1 -Portable`) at the side-panel width and a
  wide details page, with and without background art, for a manual game and a combined game.
- Changeset: minor ("redesigned game details page").
- `docs/ROADMAP.md`: hand-editing metadata, with user values surviving a metadata refresh.

## Calls settled after the builder check (mine)

- **Save** sends only what changed, in order name → launch options → cover, and stops at the
  first failure: earlier changes stay saved, the error shows in the popup, the popup stays open
  (a retry is harmless). Nothing changed: Save just closes.
- **Open install folder** goes through a new thin command `open_install_folder(id)` that opens
  the entry's own `install_dir` from Rust, like `open_app_folder`, so the webview needs no
  open-path permission for arbitrary paths. (A small exception to "no core changes".)
- **Uninstall/Remove confirmations** become a small confirm `Modal` ("Uninstall “X” from GOG?
  The store will ask to confirm." / "Remove “X” from your library?") with the action and Cancel;
  Cancel is focused first; focus goes back to the ⋯ button.
- **Errors** of page actions (play, install, toggles, menu actions, store rows) show in one
  `role="alert"` line under the action row, as today; refetch artwork keeps its toasts.
- **Popup lifecycle:** the draft is taken from the game once, when the popup opens. The popup
  closes when the shown game changes (another id). F2 opens it unless it is already open.
- **Launch options** show when the entry's own source is manual (`game.source`), as today. Name
  and cover apply to the entry, as today.
- **Stores card:** combined games keep the per-copy playtime ("Installed · 12h"); "preferred"
  marks the first copy. A single-store game shows its one row without a ⋯ menu.
- **Details card** does not repeat playtime/last played/stores (they are in the header); the
  Library card shows for every game.

## Left to the builder

Exact sizes, spacing, card column ratios and breakpoints; icons (lucide); how the stats look
when a value is missing; component split.
