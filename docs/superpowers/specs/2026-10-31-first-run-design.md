# First-run setup: design

Status: ready for building. Step 4 (last) of the game page work. Approved mockup:
`docs/mockups/first-run.html` (screens 1, 2, 3, 3b and the library hint).

## Why

IGDB and SteamGridDB need the user's own free keys (no Questkeep-run proxy: the user doesn't want
to maintain a service). Without IGDB much of the new metadata stays empty (time to beat, links,
series, age ratings, game modes, themes, perspectives, metadata for stores without their own).
New users need to learn how to get the keys, and be told what they miss if they skip.

## Goals

- A setup flow shown **once, on a fresh database** (no games, setup never completed), in the
  main window before the library:
  1. **Welcome:** what Questkeep is; "Skip setup", "Get started".
  2. **Stores:** the same store list as Settings → Stores (found games per store, "Sign in"
     where the store has an account), reusing that component; only stores the system supports.
  3. **Metadata keys:** IGDB (recommended) with what it adds and the steps to get Twitch keys
     (dev.twitch.tv/console, register an app with redirect `http://localhost`, category
     "Application Integration", client type "Confidential", copy Client ID and a new secret),
     two fields, "Test keys" with the result inline; SteamGridDB (optional) with its one step
     and one field. Keys go to the keyring as the settings do today. "Skip" and "Finish".
  - **3b:** finishing or skipping without an IGDB key shows "Continue without IGDB?" with the
    list of what stays empty, and where to add the key later (Settings → Metadata, then
    "Refresh metadata for all games"). "Back to keys", "Continue without".
- **Existing users** (database with games) never see the flow. If they have no IGDB key, a
  dismissible bar above the library says "Game details are limited: no IGDB key is set." with
  "Set up" (opens Settings → Metadata) and "Don't show again". New users who skipped IGDB get
  the same bar.
- **Settings → Metadata** shows the same step-by-step instructions next to the key fields.
- The scan runs during the flow as today, so the library is filled when the user finishes.

## Non-goals

- A Questkeep proxy for IGDB; any other setup (themes, tray, start with system).

## Anti-goals

- Blocking the app: every step can be skipped, and closing the window ends the flow as skipped.
- Showing the flow again after it was finished or skipped, or to existing users.
- Storing keys anywhere but the keyring.

## Decisions

- "Setup done" and "bar dismissed" are app state flags in the database. (My call.)
- "Fresh database" is a flag written when the database file is created (not "no games", since
  the first scan may already have added games before the UI checks). Existing databases get
  "setup done" set by the migration that adds the flag. (My call.)
- "What stays empty" lists IGDB-only data: time to beat (only HowLongToBeat's numbers, when it finds the game), website/wiki/social links (store links
  still work), series, age ratings, game modes, themes, perspectives, and metadata for games
  whose store has none.
- "Test keys" requests a Twitch token and one tiny IGDB query; errors are shown in plain words
  (wrong ID/secret, no network).
- Links to Twitch and SteamGridDB open in the default browser.

## Constraints

- From AGENTS.md: keyboard paths for everything (the flow is a focus-trapped dialog; Enter =
  primary button, Escape = skip); no literal colors/sizes outside `theme.css`.
- Testing in the hidden app must use a portable throwaway data dir and never save credentials
  (the hidden app shares the user's keyring).

## Left to the builder

- Exact copy text, layout details within the mockup's look.
