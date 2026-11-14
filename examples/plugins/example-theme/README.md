# Example Theme

A Questkeep theme plugin: one theme, "Arcade", made of a `plugin.json` and a CSS file. No build
step, no code.

| What you see in Questkeep | Where it comes from |
|---|---|
| "Arcade" in Settings > Appearance, marked "Plugin: Example Theme", with its own swatches | the `themes` entry in [`plugin.json`](plugin.json) (`id`, `name`, `description`, `preview`) |
| Neon on deep violet in dark mode, ink on cream in light mode | the two palette rules at the top of [`theme.css`](theme.css) |
| "Accent color" under "Arcade options", with a different default per mode | the `accent` color parameter; the app's palette reads `--qk-param-accent` by itself |
| "Glow": game covers and menus get a glowing edge | a `toggle` parameter and a rule on `[data-param-glow="1"]` |
| "Sidebar: Left / Right" under "Advanced": the sidebar changes sides | a `select` parameter with `"advanced": true` and three rules that place the app's grid areas differently |
| A taller title bar, lower corners, section headings in capitals | variables (`--qk-titlebar-height`, `--qk-radius-scale`) and a few ordinary rules |
| Fine lines across the title bar | [`assets/scanlines.svg`](assets/scanlines.svg), referenced as `url("assets/scanlines.svg")` |

Every rule starts with `:root[data-theme="example-theme"]`, the theme's id, so nothing applies
while another theme is chosen.

## Try it

1. In Questkeep: Settings > Plugins > "Install from folder…" and choose this folder.
2. Settings > Appearance: choose "Arcade". Try its options, and Light and Dark.
3. Back in Settings > Plugins, turn on "Reload on change" in the plugin's row.
4. Change `theme.css` and save: the app follows within a moment, with the theme still chosen.

## Make it yours

Copy the folder, then change the plugin's `id` and the theme's `id` in `plugin.json` (the
theme's id must be the plugin's id, or start with it and a dash) and replace `example-theme` in
every selector of `theme.css`. All variables a theme can set are listed in
[`ui/src/theme.css`](../../../ui/src/theme.css); the theme editor in the app ("Customize…" on a
theme) shows them with their current values and can export a finished theme as a plugin like
this one.

Keep text readable: at least 4.5:1 against the surfaces. This example is checked for that by
`ui/src/lib/pluginThemes.test.ts`, with the same rules as the built-in themes.

The full guide is [`docs/THEMES.md`](../../../docs/THEMES.md).
