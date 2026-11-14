# Example Extension

A Questkeep extension in plain JavaScript, without a build step. It shows every part of the
extension API in one small file ([`main.js`](main.js)):

| What you see in Questkeep | API |
|---|---|
| A dice button in the title bar that picks a random game in a dialog (it opens the game's page; it never starts a game) | `ui.addTitleBarButton`, `ui.showDialog`, `games.list`, `games.open` |
| "Copy name" and an "Example extension" submenu in a game's context menu | `ui.addMenuItems("game", …)` |
| "Library facts" in the Questkeep menu and in the sidebar under "Plugins": games per store and the total playtime, updated while the library changes | `ui.addMenuItems("app", …)`, `ui.addPage`, `events.on` |
| Four settings under Settings > Example Extension, drawn by the app from `plugin.json` | `extension.settings`, `settings.get` |
| A "Preview" below them that follows the settings | `ui.addSettingsSection`, `settings.onChange`, `settings.set` |
| A message when the extension starts (if the first setting is on) | `ui.toast` |

[`style.css`](style.css) uses only the app's `--qk-*` variables, so it follows every theme.

## Try it

1. In Questkeep: Settings > Plugins > "Install from folder…" and choose this folder.
2. Turn on "Reload on change" in the plugin's row.
3. Change `main.js` or `style.css` and save: the extension restarts with the new files, and
   everything the old version added is removed first.

## Types

`main.js` starts with `// @ts-check` and refers to
[`../questkeep-plugin.d.ts`](../questkeep-plugin.d.ts) through JSDoc, which gives completion and
type errors in editors that understand TypeScript. For your own plugin, copy that file next to
it and adjust the import paths.

The full guide is [`docs/PLUGINS.md`](../../../docs/PLUGINS.md).
