# Start page and new game card: design

Status: approved mockups, design for planning. Mockups (open in a browser; data is made up):

- [`docs/mockups/start-page.html`](../../mockups/start-page.html): the start page and the grid
  with the new card. Its "Grid view" button shows the grid. The "Title strip" toggle is a
  rejected variant; "Title on hover" is the chosen one.
- [`docs/mockups/start-page-settings.html`](../../mockups/start-page-settings.html): the settings.
  Variant **B (one dropdown)** was chosen; variant A is rejected.

Where this document and the mockups disagree, this document wins.

## Why

Questkeep opens straight into a list of games. There is no place that answers "what was I
playing?", "what did I just get?" or "how much did I play this week?". A start page gives that
overview at a glance and lets you jump back into a game with one click. The grid tile gets a
redesign at the same time, because the start page needs a card and the two should look the same.

## Goals

- A **Home** entry at the top of the sidebar, above all views, opens the start page.
- The start page shows two big shelves, **Recently played** and **Recently added**, and a band of
  stats at the bottom: **playtime of the last 7 days** (hours per day) and **installed games per
  store**.
- Games can be played or installed straight from the start page.
- One new **game card** component, used by the start page shelves and by the grid view.
- Settings: the start page can be turned off, and the page shown when Questkeep opens is
  configurable: the start page, the page open last, or a chosen view.

## Non-goals

- A "Show all" link on the shelves, or temporary/unsaved views. Dropped for now.
- A "continue playing" hero banner.
- A search box on the start page. The title bar's command bar (Ctrl+K) stays the only search.
- Customising the start page (choosing, ordering or resizing shelves and stats).
- More stats (playtime per game, per month, achievements and so on). Ideas for ROADMAP.
- Changing the list and details layouts. Only the grid gets the new card.

## Anti-goals

- A start page that looks stretched on wide screens. The content must stay centered with a
  maximum width.
- Cropped covers. Covers show uncropped at 2:3, like a store cover.
- Two search boxes on screen.
- Text labels taking the space where an icon is clear (for example "Not installed").
- Any action that only works with the mouse.

## Constraints

From AGENTS.md:

- Logic (queries, aggregation, date bucketing) lives in `questkeep-core` with tests. Commands stay
  thin, and `ui/src/lib/api.ts` is the only module that calls `invoke()`.
- Schema changes are appended steps in `db/migrations.rs` (currently up to step 8).
- Changing Rust command types regenerates `ui/src/bindings` via `cargo test`; commit both.
- No literal colors or sizes outside `theme.css`. Every visual value of the new UI becomes a
  `--qk-*` variable.
- Every action has a keyboard path, and buttons are labelled.
- Pure UI logic goes into `ui/src/lib/` with vitest tests.
- UI work is checked in the hidden running app, not only in jsdom tests.
- A changeset (`minor`) for the feature.

## The design

### Sidebar and navigation

- A **Home** entry sits above all view sections in the sidebar, styled like a view entry, with a
  house icon (lucide `house`). It is not a view: it can't be renamed, moved, deleted or edited,
  and the sidebar editor ignores it.
- Picking Home shows the start page in the main area. The library toolbar (filters, sort, layout,
  save view) is not shown on Home, and neither is the details side panel.
- The UI's `Page` type gains `"home"` next to `"library"` and `"settings"`.
- Keyboard: Home is reachable like any sidebar entry. Cards on the start page can be selected
  with the keyboard.
- **Opening a game** from the start page (Enter, double-click, or whatever opens it in the grid)
  works like picking it in the command bar today (`store.openGamePage`): Questkeep switches to
  the first view that contains the game and opens it there. Closing the game leaves you in that
  view, not on Home. (Decided by the user: no game page on Home and no "back to Home".)
- Play/Install on a card starts the action without leaving Home.
- When the start page is turned off in settings, the Home entry disappears. If Home was open, the
  first view opens.

### Start page layout

Top to bottom, filling the window height without a page scrollbar on normal window sizes:

1. **Recently played** shelf.
2. **Recently added** shelf.
3. **Stats band**: two panels side by side, max width about 1180 px, centered.

Each shelf:

- A heading in plain horizontal text ("Recently played", "Recently added"). No rotated labels and
  no "Show all".
- Shows the **15 most recent games**. Recently played: games with a `last_played`, newest first.
  Recently added: by `date_added`, newest first.
- "Added" means **added to Questkeep** (the import time); stores don't provide purchase dates.
  For a game combined from several stores, the **newest copy's** `date_added` counts, so a game
  you just got in a second store shows up again. Ties (for example right after the first scan,
  when everything was added "Today") are ordered by sorting name. (Decided by the user.)
- Scrolls **horizontally** when the cards don't fit. The mouse wheel scrolls it sideways. When the
  cards fit, the row is **centered**.
- **Cover size follows the window height**: the cover width is computed so both shelves and the
  stats band fit the window, clamped between about 90 and 240 px wide (2:3 covers).
- The scroller leaves room above the cards so a raised hover card and its outline are not clipped
  (the mockup uses 6 px padding with a -4 px margin so the layout doesn't move).
- Each card shows a **date badge** in the bottom-left of the cover: the last-played date on
  Recently played, the added date on Recently added.
- Empty states: a shelf with no games shows one muted line ("Nothing played yet", "No games yet"),
  not an empty row.

Date badge text (relative to the local date): "Today", "Yesterday", "N days ago" (2 to 6),
"1 week ago" / "N weeks ago" (7 to 27 days), then a short date "18 Sep", plus the year when it
isn't the current year ("18 Sep 2024").

### Stats band

**Playtime, last 7 days**

- A bar chart, one bar per day: today and the 6 days before, in local time, oldest on the left,
  today on the right and highlighted. Day labels are short weekday names, with "Today" for today.
- Each bar shows its hours above it with one decimal, or "–" for zero. A bar has a minimum
  visible height so zero days still show a stub.
- The panel header shows the week's total and the daily average ("9.4 h · avg 1.3 h/day").
- The data comes from `play_sessions`. This includes sessions recorded for games started outside
  Questkeep.
- A session's **credited** time (`duration_seconds`, which already skips system sleep) is what
  gets counted, so the week matches the playtime shown elsewhere. A session that crosses local
  midnight spreads its credited time over the days in proportion to its wall-clock span. A
  session still running counts from `started_at` to `last_seen_at`. (My call: exact per-day
  credit would need a schema change, and sleep across midnight is rare.)

**Installed by store**

- One row per store the user has games in, in **two columns**: store logo, store name, a bar, and
  the numbers "installed / owned" (installed in bold).
- The bar shows the **share installed** (installed ÷ owned). The tooltip says "N % of your <Store>
  games are installed".
- Owned counts a store copy once per game. For a game combined from several stores, each store
  counts its own copy.
- The header shows the total number of installed games.

### The game card

One component, used by the start page shelves and by the grid view (it replaces the current
`GameTile` look; Play/Install keeps using the existing `primaryAction` logic from `QuickAction`).

Structure, top to bottom:

- **Top strip**, outside the cover, 36 px tall:
  - Left: the **primary store's logo and name**. The primary store is the copy that plays: the
    first installed copy in `copies` order, else the first copy. Logos are the app's real
    single-colour provider glyphs (`providers/icons/*.svg`), with **no background**.
  - When the game is in **more than one store**, a small accent-colored **dot** (no text) sits on
    the bottom-right corner of the primary logo. Hovering the logo shows a tooltip listing all
    stores.
  - On **hover or selection**, the store name and the dot are hidden and **all store logos** are
    shown side by side instead, primary first.
  - Right: state icons (lucide), each with a tooltip:
    - favorite: filled yellow star;
    - hidden: eye-off;
    - running: green play icon in a pulsing circle;
    - not installed: cloud-download icon in a grey circle.
- **Cover**, edge to edge: 2:3, uncropped, no padding. The card has radius 10, a thin outline
  and a shadow. On hover the card rises a few pixels; when selected it gets an accent outline.
  Not-installed covers are dimmed (desaturated and darker).
  - The **title** is shown only on hover or selection, as a two-line clamp over a bottom gradient
    on the cover. The date badge fades out meanwhile.
  - The **Play / Install** pill sits in the middle of the cover, visible on hover or selection.
    A running game shows no button.
  - Games without a cover keep the current placeholder that shows the title.
- No bottom strip and no always-visible title.

Keyboard: the card stays focusable and selectable as today's tile; Enter opens the game and the
existing play shortcut still works on the selection.

The date badge uses no `backdrop-filter`, but a near-opaque background instead. (A pulse
animation looked choppy in Firefox with many blurred badges on screen. This doesn't affect the
webview, but the blur adds nothing visible.)

### Settings

In **General**, a new group "Start page" (mockup variant B):

- Checkbox **"Show the start page"** (default on).
- **"When Questkeep opens, show"** followed by one dropdown:
  - "Start page" (default);
  - "The page I had open last";
  - a "Views" group listing every saved view by name.
- While the start page is off, the "Start page" entry is disabled. If it is the chosen one, a hint
  under the dropdown says Questkeep opens the first view instead.
- "The page I had open last" includes the start page: closing the app on Home reopens Home.
  Settings is never remembered as the last page; the page before it counts.
- If the chosen view has been deleted, Questkeep opens the first view.

Settings fields (`settings.rs`, camelCase in JSON, defaults so old files keep working):

- `showStartPage: bool` (default `true`).
- `startupPage`: one of `"start"`, `"last"`, or a view id. The exact Rust shape (an enum with a
  `View(String)` variant, or a string) is up to the builder, as long as the binding is typed.

The page open last is **not** stored in `settings.json`: the settings page saves the whole
settings object, which would overwrite it. It is stored in the database instead (see below).

### Data and commands

New core functions, each with tests:

- `recently_played(limit)` and `recently_added(limit)`: the games for the shelves. Hidden games
  are left out (my call).
- `playtime_by_day(days, now, local offset)`: credited seconds per local day for the last `days`
  days, spread over local midnight as described under the stats band.
- `installed_by_store()`: per store, installed and owned copy counts. Hidden games are left out,
  and so are manual games, which have no store (my call).
- Last page: a small key-value table (`app_state`, appended migration step) with get/set of the
  last page (`"home"` or a view id). The UI writes it when the page changes.

The UI loads the start page data when Home opens and refreshes it on the core events that already
refresh the library (game changed, scan finished, session started/ended), and once a minute while
a game is running so today's bar grows.

## Left to the builder

- Component and file names, and how the card is shared between the shelf and the grid
  virtualizer.
- The exact formula for the cover size and the minimum window height before the page scrolls.
- Whether the four stats queries are one command or several.
- How the store order in "Installed by store" is chosen (the mockup sorts by owned count,
  largest first).
- Theme variable names for the new values.
