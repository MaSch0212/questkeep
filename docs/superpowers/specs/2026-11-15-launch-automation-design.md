# Launch automation: notifications, closing launchers, window on launch, game scripts

Status: implemented (2026-11-15). Four pieces, built in this order: 0, 1, 2, 3. Part 0 comes
first because the others report their failures into it.

## Intent

The user wants Questkeep to do more around a game launch than start the game:

1. Close a store's launcher after playing, so Steam, Epic & co. don't linger in the background.
2. Get Questkeep's own window out of the way while a game runs, and optionally back afterwards.
3. Run user scripts around launches and on demand, e.g. switch the EQ profile of an audio program
   before a game and back after it, or show the current game on an LED wall. Scripts must be very
   flexible: pass as much about the game as possible.

Much of this happens while Questkeep's window is hidden, so errors must not get lost: a
notification center (bell) plus system notifications (part 0).

Roadmap items this finishes: "pre/post scripts" and "Minimize while playing" (`docs/ROADMAP.md`,
Later).

## Goals, non-goals, anti-goals

- Goals: the four parts below, with safe defaults: nothing changes for a user who doesn't touch
  the new settings (except that errors now also land in the bell).
- Non-goals: scripts for games started outside Questkeep (the watcher's detected launches,
  including games started from a store launcher Questkeep opened); script sharing; scripts on app
  start/exit or sync; multiple play actions per game.
- Anti-goals: losing a launcher's cloud save by killing it mid-upload; a pre script that hangs and
  keeps the game from starting forever; post scripts not running after their pre scripts ran
  (pairs like "EQ on / EQ off" must stay balanced); Questkeep quitting (and losing playtime
  tracking) because its window was closed on launch; starting Windows' Microsoft Store
  `python.exe` stub (it hangs); notifications that pop over a fullscreen game.

## Constraints

- Store-specific knowledge stays in store crates (`AGENTS.md`, "New store"); WASM plugin stores
  get the same capability through the plugin SDK and the wire format.
- Core stays sync (threads). Scripts and launcher closing run on core threads, so they work with
  the window hidden.
- Schema changes: append migration steps only.
- UI: theme variables only, labeled buttons, keyboard paths, `data-tooltip`, pure logic in `lib/`
  with tests, all texts in the i18n tables.
- Windows and Linux (CI runs both).
- Wiki pages for launching, settings, the Edit dialog, the plugin SDK, and a new Scripts page get
  updated in the same work.

## 0. Notifications

- **Bell** in the title bar (top right) with a count badge of unread entries, colored when one is
  an error. Opens a popover: newest first, each with time, message, optional action button (e.g.
  "Open log", "Open store settings"), dismiss; "Clear all". Reachable by keyboard shortcut and the
  command bar. Opening it marks entries read.
- **What goes in:** errors and warnings, including today's error toasts and sync failures.
  Informational toasts ("Synced 12 games") stay transient and don't enter the bell. Toasts keep
  popping up as today; an error toast is also recorded in the bell.
- **Core can post notifications** (new `CoreEvent`, level + message + optional action), so tracker
  threads, script runs and launcher closing report even while the window is hidden. Existing
  UI-side error toasts go through the same store function.
- **Persistence:** the last 100 entries are saved (core, in the data folder) and survive restarts.
- **System notifications:** when Questkeep's window is minimized or hidden in the tray, errors and
  warnings also show as OS notifications (Windows toast / Linux notification). Clicking one opens
  Questkeep with the bell open. Setting in General: "Show system notifications while Questkeep is
  in the background", on by default. Windows' Focus Assist holds them back during fullscreen
  games; Questkeep does nothing extra about that.

## 1. Closing launchers after playing

**Setting** per store, in the store's row of Settings > Stores and accounts, only for stores that
declare a closable launcher: "Close launcher after playing": **Never** (default) / **Always** /
**Only if it wasn't running when the game started**. Stored as an app setting
`launcher_close: BTreeMap<store id, LauncherClosePolicy>` (not a `StoreOption`: that is a
checkbox and triggers a resync).

Two global values in General (advanced): **wait after the game closes** (default 60 s) and
**minimum session length** (default 2 min). Same as Playnite's defaults.

**Store API.** New provider method `launcher_process(&self) -> Option<LauncherProcess>`:

- `process_names`: executable names whose presence means "the launcher is running";
- `close`: how to close it, one of
  - `Command(PlayAction)`: the launcher's own clean exit (Steam `-shutdown`, GOG Galaxy
    `/command=shutdown`, Battle.net `--exec=shutdown`);
  - `CloseWindows`: ask the processes' windows to close (Windows `WM_CLOSE`, Linux SIGTERM), then
    force-quit if they are still running after 2 minutes;
  - `ForceQuit`: terminate right away (Epic, which has no clean exit; as in Playnite).

Default `None` (no setting shown): PrismLauncher, manual, Xbox. The rest get what Playnite uses
(Steam, GOG, Battle.net: Command; Ubisoft and Amazon: CloseWindows, Amazon on its UI process;
Epic: ForceQuit; EA, Heroic, Lutris: the builder checks what works). Stores with a `Command` are
never force-quit. The plugin SDK and `wire.rs` get the same call.

**Behavior.**

- At launch (launches from Questkeep only), record whether any of the store's `process_names` was
  running. The store is the launched copy's store (combined entries: the copy that was launched).
- When the game actually ran and its session ends (not on app shutdown): skip if the session was
  shorter than the minimum; skip if another game of the same store is running (any session
  Questkeep knows of: its own launches and watcher-detected ones); otherwise, if the policy says
  so, schedule closing after the wait. A game of that store starting during the wait cancels it.
- Post scripts (part 3) run before the wait starts.
- If the game never ran (startup timeout, only ignored programs, launch stopped by a pre script),
  the launcher is not closed: the user may still be signing in or updating.
- Failures post a warning notification; nothing blocks.

## 2. Window when a game starts

General settings, for launches from Questkeep only:

- "When a game starts": **Do nothing** (default) / **Minimize** / **Hide to tray**. Hide to tray
  never quits the app, whatever the close-to-tray settings say, so tracking continues.
- "Restore the window when the game closes": checkbox, off by default, disabled with "Do
  nothing". Restores without taking focus; with several games running, after the last one ends.
  Also restores when the game never ran (startup timeout, stopped launch).

The window changes at the moment the game is detected running (`GameStarted`), not on the Play
click: a store client may need input first (sign-in, update prompt), and hiding on click would
hide Questkeep over it for nothing. The core emits the events; the Tauri shell applies the window
changes, like the tray decisions in `tray.rs`.

## 3. Game scripts

### Concepts

A **script** has:

- name, kind (**pre**, **post**, **action**), enabled flag;
- scope: **global**, **filtered** (a `GameQuery`, edited like an auto category's filter), or
  **game** (belongs to one library entry);
- for pre scripts, **when**: **Before the game starts** (default) or **Once the game is running**.
  The second is for games with their own launcher (e.g. Cyberpunk's REDlauncher): the script runs
  when the launcher detects the game. Since directory tracking also counts the game's own
  launcher as "the game", the dialog hints that such games need their tracking set to the game's
  executable name (already possible in the Edit dialog);
- what to run, one of:
  - **Program**: executable path, arguments, working folder;
  - **Inline script**: a runtime (below) and the script text, stored as
    `<data>/scripts/<script id>.<ext>`; optional arguments;
  - **Linked file**: an external script file used where it is; runtime picked from its extension
    (overridable);
  - **Imported file**: an external file copied into `<data>/scripts/`, then edited like an inline
    script;
- advanced options: show the window (default hidden); wait for it to finish (default yes for
  pre/post, no for action); timeout (default 60 s for pre/post; on timeout Questkeep stops waiting
  and continues, the script keeps running); stop the launch if this pre script fails (default
  off).

**Ordering.** Settings has two ordered lists, **pre** and **post**, holding the global and filtered
scripts plus one fixed placeholder item **"Game's own scripts"** (default: last in pre, first in
post). Drag and drop plus keyboard (move up/down). A game's own pre/post scripts run, in their own
order, at the placeholder. Action scripts are listed in list order: game's own first, then global
and filtered.

**Opting out.** Per game: "Run global scripts for this game" (on by default).

**Filters** are checked against the library entry (as auto categories are), not the launched copy;
the launched copy's store is available to the script as a variable instead. This also makes the
Edit dialog's "these global scripts apply" list exact.

**Combined games.** Scripts belong to the library entry; combining gives the combined entry the
union; separating leaves them with the entry that owned them.

### Runtimes

Offered only when found on this PC (detected at start and on demand, cached):

| Runtime | Windows | Linux | Extension |
|---|---|---|---|
| PowerShell 7 (`pwsh`) | PATH, `%ProgramFiles%\PowerShell\7\` | PATH | `.ps1` |
| Windows PowerShell 5.1 | `%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe` | – | `.ps1` |
| Bash | Git for Windows' `bash.exe` (not WSL's `System32\bash.exe`) | `/bin/bash` | `.sh` |
| Command Prompt | `cmd.exe` | – | `.cmd` |
| Python | `py.exe`, or `python.exe` on PATH **not** under `WindowsApps` | `python3` | `.py` |

Both PowerShell versions are separately pickable (most people only have 5.1). Detection never
executes the Store stub. A script whose runtime is missing is skipped at run time with a warning
notification. Invocation is fixed per runtime (e.g. `powershell.exe -NoProfile -ExecutionPolicy
Bypass -File <file> <args>`); the user's arguments are appended.

### Variables

Every script gets environment variables, and the same values as `{Name}` placeholders in its
arguments and working folder. Placeholders are inserted **verbatim**: the user quotes them as the
target program needs; the dialog says so and recommends environment variables for free text (no
quoting problems, no command injection through names from stores). Unknown placeholders stay as
typed; empty values become empty strings.

At least:

- Context: `QK_PHASE` (pre/post/action), `QK_SCRIPT_NAME`, `QK_SCRIPT_FILE`, `QK_DATA_DIR`,
  `QK_GAME_JSON` (path to a temp JSON file with the complete game record plus the launch info;
  removed after the script ends or at next start).
- Game: id, name, sorting name, kind, store id and name, store game id, installed, install folder,
  favorite, hidden, completion status, user/critic/community score, release date, playtime
  (seconds and formatted), play count, last played, date added. Description and links only in the
  JSON.
- Lists (joined with `;`): genres, developers, publishers, tags, features, series, platforms,
  categories (names), game modes, themes, perspectives, age ratings.
- Images: cover, background, icon file paths.
- Launch: action kind, target, arguments, working folder, launch profile name, launched copy's
  store and store game id.
- Post only: session length in seconds, start/end time, whether the game ran.

The script dialog lists every variable with its value for the game being edited (or a sample game
for global scripts), click to insert, and says how lists are joined.

### UI

- **Settings > Scripts** (new section): pre list, post list (sortable, with the placeholder), and
  action list; each item a one-line summary (name, scope/filter summary, what it runs) with Edit,
  Enable/disable, Delete; "Add script" per list. Filtered scripts use the auto-category filter
  editor (extracted into a shared component with its live list of matching games).
- **Edit game dialog**: a Scripts tab with the game's own scripts as one-line summaries, the
  global-scripts opt-out, and the resolved run order for this game (global, filtered and own
  scripts). Its "Open in Settings" button opens Settings › Scripts; with unsaved changes it asks
  first: **Save and open** / **Discard and open** / **Cancel**.
- **Unsaved changes (Edit game dialog and script dialog):** Escape and clicking outside the
  dialog ask before discarding (Save / Discard / Keep editing); the Cancel button discards
  without asking. Without changes they close right away.
- **Script dialog** (modal, from both places): name, kind, scope (filter button for filtered;
  hidden in the Edit dialog), when (pre only), "What it runs" choice (**Program** / **Script** /
  **Linked script file**), its fields, "Advanced" disclosure, "Variables" panel, and "Run now"
  against a chosen game (runs as an action).
  - Script: runtime picker, the editor, and "Import file…", which loads a file into the editor as
    a library copy (so "inline" and "imported" are one choice).
  - Linked script file: path with Browse…, "Import a copy" (switches to Script), and "Run with"
    (picked from the extension).
  - Program: path with Browse…, arguments, and a **preview box** under the arguments showing the
    full command line and working folder with the chosen game's values, with a copy button.
  - Every working-folder field has a Browse… button opening the OS folder picker.
  - Action scripts have an icon for their button.
  - The Variables panel has a "Values for" game picker (also for global scripts) and switches
    between environment-variable and `{placeholder}` syntax; click inserts at the cursor.
- **Mockup:** `docs/mockups/launch-automation.html` (tabs 0–3e) shows every screen above.
- **Editor**: CodeMirror 6, lazy-loaded with the dialog, modes for PowerShell, shell, batch and
  Python; light autocomplete with the language's keywords and the `QK_*` variables in the
  language's syntax (`$env:QK_…`, `$QK_…`, `%QK_…%`, `os.environ["QK_…"]`).
- **Action scripts**: buttons in the game page's action row (up to two, the rest under "More
  actions"), an "Actions" submenu in the game context menu (single game only), and "Run: <name>"
  in the command bar.

### Execution

- **Launch flow.** The launch command returns as soon as the launch is accepted; pre scripts, the
  game start and its errors continue on the tracker thread and report through notifications. The
  game counts as running (the "already running" guard, the UI's starting state) from the moment
  the launch is accepted, so a second click can't start a second launch.
- **Pre scripts** ("before") run in order before the game's process starts; each waits (with its
  timeout) when "wait" is on. "Once running" pre scripts run in order when the game is detected.
  A failing script (non-zero exit, missing runtime, cannot start) posts an error notification;
  with "stop the launch" on, the launch stops with that message.
- **Post scripts** run in order after the session ends, whenever any pre script ran, even if the
  game never ran, so pairs stay balanced. With no pre scripts they run only if the game ran.
  Then the launcher close (part 1) is scheduled.
- **Games without tracking** (no matcher): today `launch` returns without a tracker. With scripts
  the launch still gets a thread that runs pre scripts, starts the game, and runs post scripts
  right away when no tracking is possible (documented in the dialog).
- **Action scripts** run on their own thread; failures post a notification.
- Output (stdout/stderr) goes to a log in `<data>/logs/scripts/` (last N runs); the notification's
  action opens it.
- Backups include the scripts tables and `<data>/scripts/`.

### Data

A migration adds `scripts` (id, name, kind, scope, game_id nullable, filter JSON nullable, when,
run JSON, options JSON, enabled, position), the placeholder positions of the pre and post lists,
and a per-game flag for the global-scripts opt-out. Deleting a game deletes its scripts and their
files.

## Approaches considered

- **A. Scripts as library data in the database (chosen by the user).** Filters run where the
  games are; backups and combining handle scripts like other library data. Costs a migration and
  core code.
- B. Global scripts in `settings.json`, game scripts on the game record: less schema work, but
  filters still need the DB, settings grow with script text, two storage rules.
- C. Scripts as a JavaScript-extension feature: extensions don't run on tracker threads or with
  the window hidden; doesn't fit pre/post.

Launcher closing follows Playnite's model (wait, minimum session, per-store method), chosen by the
user over "never force-quit" and "store declares whether killing is safe".

Editor: CodeMirror 6 over Monaco (much lighter, enough for keywords and variables).

## Decided by the user

- Notification bell in the title bar for errors and warnings (sync failures included), plus
  system notifications while the window is minimized or hidden.
- Close-launcher policy per store: Never / Always / Only if not running at launch; default Never.
  Force-quit only for stores without a clean exit command, as Playnite does.
- Window on launch: minimize or hide; restore when the game closes as an option, off by default.
- Scripts: global, filtered (auto-category filter dialog with matching games), per game;
  pre/post/action; drag-and-drop order for pre and post with a placeholder for game scripts;
  pre/post only for launches from Questkeep; pre scripts "before start" or "once running" (for
  games with their own launcher); post scripts run whenever pre scripts ran; action scripts as
  buttons on the game page and in the context menu; as much information as possible as variables
  and placeholders, image paths included; program + args + working folder with advanced options;
  inline scripts with syntax highlighting and light autocomplete; only installed runtimes,
  PowerShell 7 and Windows PowerShell 5 separately; a script dialog with details, summaries
  elsewhere; variables visible in the dialog; link or import external scripts; database storage.
- Mockup review: the command-line preview in its own box; Browse… for working folders; "Open in
  Settings" asks first with unsaved changes; Escape and click-outside ask before discarding, the
  Cancel button doesn't.

## Calls made while writing (check these)

- Window minimize/hide happens when the game is detected running, not on Play.
- Any running session of the store (also watcher-detected ones) prevents closing its launcher.
- Placeholders are inserted verbatim; environment variables recommended for free text.
- Filters match the library entry, not the launched copy.
- Launch command returns at once; pre-script and start errors arrive as notifications; the game
  counts as running from the accepted launch.
- Failing pre scripts don't stop the launch unless the script's option says so; scripts run hidden
  by default; 60 s default timeout.
- Notifications: last 100 kept across restarts; informational toasts stay out of the bell.
- Inline and imported scripts are one "Script" choice with an import button; actions have icons;
  at most two action buttons on the game page.

## Left to the builder

Exact variable names beyond those listed, process names and close methods per store (EA, Heroic,
Lutris), notification shortcut, log retention, combine/separate details for game scripts,
CodeMirror packages per language, the OS notification plugin.
