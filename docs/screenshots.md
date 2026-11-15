# Screenshots for the wiki

The wiki's screenshots show a made-up library, never a real one: no account names, real games,
or local paths with user names. To get that library:

1. Fill an empty data folder with the demo library (35 games in eight stores, two apps, a few
   years of play sessions, metadata, statuses, scores, categories, one automatic category and
   two uncombined duplicates for the combine dialog):

   ```powershell
   cargo run -p questkeep-core --example demo_library -- target\debug\data
   ```

   `target\debug\data` is the portable data folder of the debug build; it must not exist yet
   (move a test library you want to keep out of the way first).

2. Start the hidden instance in demo mode. `-Demo` implies `-Portable` and keeps sign-ins in
   memory, so the keyring's real accounts never show:

   ```powershell
   scripts/dev-hidden.ps1 -Demo
   ```

3. Once the page is up, draw the artwork (covers, backgrounds, icons) and reload the page:

   ```powershell
   node scripts/demo-art.mjs target\debug\data
   ```

Then drive the page through the DevTools protocol (`http://127.0.0.1:9222/json`): set the size
with `Emulation.setDeviceMetricsOverride` and take shots with `Page.captureScreenshot` (with a
`clip` for single dialogs or panels). The page's modules can be imported for setup, e.g.
`await import('/src/lib/store.ts')` and `'/src/lib/api.ts'` (scripts, notifications).

Check every shot before committing it: no paths with user names, no account names. Never start a
game from the demo instance: a store's play action would still open the real store.
