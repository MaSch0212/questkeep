# Themes

How Questkeep's look is put together, how to make a theme in the app, and how to write one by hand
as a plugin.

## How a theme reaches the page

- Every visual value of the UI is a CSS variable named `--qk-*`, defined in `ui/src/theme.css`.
  `ui/src/app.css` only uses those variables. `theme.css` is also the palette of the default
  theme ("Questkeep") for light and dark.
- The app sets two attributes on `<html>`: `data-scheme="light"` or `"dark"` (the Mode setting,
  with "System" resolved) and `data-theme="<theme id>"`.
- A theme is CSS that overrides variables in rules written against its id:

  ```css
  :root[data-theme="fjord"][data-scheme="dark"] {
    --qk-color-bg: #131a23;
    --qk-color-text: #e3eaf2;
  }
  :root[data-theme="fjord"][data-scheme="light"] {
    --qk-color-bg: #edf1f5;
    --qk-color-text: #17212c;
  }
  ```

  Only the active theme's CSS is in the page, in one `<style id="qk-theme">` after the app's own
  stylesheets. A theme may also contain ordinary rules (`:root[data-theme="console"] .btn { … }`)
  for things variables cannot do.
- Shades of the accent (hover, soft backgrounds, the heat map, focus tints) are derived from
  `--qk-color-accent` with `color-mix()`, so a theme usually sets surfaces, text and the accent
  and gets the rest. Spacing, corners and text size scale with `--qk-space-scale`,
  `--qk-radius-scale` and `--qk-font-scale`.

The registry and the code that applies all this live in `ui/src/lib/themes.ts`.

## Parameters

A theme can offer options (Settings → Appearance shows them below the theme picker). Each
parameter is described next to the theme's CSS:

```json
{ "key": "accent", "label": "Accent color", "type": "color",
  "default": { "light": "#0f6f96", "dark": "#6cc5e8" } }
```

| `type` | Extra fields | What the CSS gets |
|---|---|---|
| `color` | `default`: a hex color, or `{ light, dark }` | `--qk-param-<key>`: the color; `--qk-param-<key>-contrast`: dark or white text that reads on it |
| `number` | `default`, `min`, `max`, optional `step` and `unit` | `--qk-param-<key>`: the number with its unit |
| `select` | `default`, `options: [{ value, label }]` | `--qk-param-<key>`: the value; also the attribute `data-param-<key>="<value>"` |
| `toggle` | `default`: true or false | `--qk-param-<key>`: `1` or `0`; also `data-param-<key>="1"` or `"0"` |

Every parameter may have `hint` (a line of help) and `advanced: true` (shown under "Advanced").
Keys are letters, digits and dashes. The values are set on `<html>`; what they mean is up to the
theme's CSS:

```css
:root[data-theme="my-theme"] {
  --qk-sidebar-width: var(--qk-param-sidebar-width, 264px);
}
:root[data-theme="my-theme"][data-param-pills="1"] .btn {
  border-radius: var(--qk-radius-pill);
}
```

The base palette reads the parameter named `accent` by itself
(`--qk-color-accent: var(--qk-param-accent, …)`), so a theme with an `accent` color parameter
needs no CSS for it.

## Making a theme in the app

Settings → Appearance → "Create theme…" (or "Customize…" on any theme) opens the theme editor, a
panel beside the app. The app stays usable while it is open, and every change shows at once.

- A user theme has a **base** theme: it starts as that theme, parameters included.
- **Variables** are grouped as in `theme.css` (surfaces, text, accent, status, charts, game cards,
  typography, shape and spacing, shadows, motion, and the long list of component sizes). Search
  finds them by name. A changed variable is marked with the mode it is changed in and has a Reset.
- The **Light/Dark** switch picks the mode being edited and previewed. By default colors and
  shadows change in that mode only and everything else in both; "Changes apply to" overrides that.
- Values are any CSS that fits the variable (`#aabbccdd`, `rgb(…)`, `color-mix(…)`, `12px`,
  `calc(…)`), without `{`, `}`, `;` or `</`. In length and number fields ArrowUp/ArrowDown step
  the value (Shift: ten times as far).
- **Custom CSS** is for what variables cannot do. It applies only while the theme is active: it is
  nested in `:root[data-theme="<id>"] { … }`, so write ordinary selectors (`.btn { … }`). To
  target one mode, start a rule with `&[data-scheme="dark"]` (for example
  `&[data-scheme="dark"] .tile { … }`). Because of the nesting, `@keyframes`, `@font-face` and
  `@import` do not work there. Changing an override of `--qk-color-accent` makes the theme's accent
  option ineffective; the editor says so.
- Nothing is saved until **Save**. Closing or discarding puts the previous look back.

Saved themes are files: `<data folder>/themes/<id>.json` (the data folder is shown in Settings →
About), with the id `user-<name-in-kebab-case>`:

```json
{
  "id": "user-neon",
  "name": "Neon",
  "base": "fjord",
  "overrides": {
    "both": { "--qk-radius-scale": "1.5" },
    "light": { "--qk-color-bg": "#fff8f0" },
    "dark": { "--qk-color-bg": "#050508" }
  },
  "css": ".btn { text-transform: uppercase; }"
}
```

When the base theme is not available (a plugin that is not loaded), the theme builds on Questkeep
until it is.

## Exporting a theme as a plugin

"Export…" on a user theme (or "Export as plugin…" in the editor) asks for a folder and writes a
plugin folder into it. For the theme `user-neon`:

```
neon-theme/
  plugin.json
  theme.css
```

```json
{
  "id": "neon-theme",
  "name": "Neon",
  "version": "1.0.0",
  "apiVersion": 1,
  "themes": [
    {
      "id": "neon-theme",
      "name": "Neon",
      "css": "theme.css",
      "params": [
        { "key": "accent", "label": "Accent color", "type": "color",
          "default": { "light": "#0f6f96", "dark": "#6cc5e8" } }
      ],
      "preview": {
        "light": ["#fff8f0", "#f6f8fb", "#17212c", "#0f6f96"],
        "dark": ["#050508", "#18212c", "#e3eaf2", "#6cc5e8"]
      }
    }
  ]
}
```

`theme.css` is the complete theme, written against `:root[data-theme="neon-theme"]`: the base
theme's rules, the overrides and the custom CSS. It no longer depends on the base theme being
installed. `preview` holds four colors per mode for the theme picker: background, surface, text,
accent. Parameter labels are exported in the language the app was in.

## Theme plugins

A plugin (see [`docs/PLUGINS.md`](PLUGINS.md)) lists its themes in `themes`. While the plugin is
enabled, each one shows in Settings → Appearance with the badge "Plugin: <plugin name>", and its
parameters show below the picker like those of a built-in theme. Disabling or removing the plugin
takes its themes away; if one of them was chosen, the default theme shows until it is back (the
choice and the parameter values are kept).

| Field | Meaning |
|---|---|
| `id` | What `data-theme` will be: 2 to 64 lowercase letters, digits and dashes. It must be the plugin's id, or start with it and a dash (`my-themes-dusk`), and must not be the id of a built-in theme or start with `user-`. |
| `name` | Shown in the picker, as written. |
| `description` | Optional; a line under the name. |
| `css` | The theme's CSS file, relative to the plugin folder; at most 1 MiB. |
| `params` | Optional; [parameters](#parameters) with literal labels (they are not translated). A malformed parameter is left out, the theme still loads. |
| `preview` | Optional; `{ "light": [bg, surface, text, accent], "dark": […] }` as hex colors for the picker's swatches. Without it the app computes them from the CSS with the parameters on their defaults. |

The plugin's `author` is shown on the theme's card. One plugin may bring several themes, and
stores or an extension beside them.

**Files of the plugin.** Relative `url(…)` references in the CSS (`url("fonts/arcade.woff2")`,
`url(../img/bg.png)`) are resolved against the CSS file's folder and loaded from the plugin, so a
theme can ship fonts and images. References with a scheme (`data:`, `https:`), absolute paths
(`/…`) and fragments (`#…`) are left as they are. `@import` of a relative file is **not**
supported: put everything into the one CSS file. `@font-face` and `@keyframes` work (the CSS of a
plugin theme is not nested like the editor's custom CSS).

**When something is wrong.** A theme whose entry is invalid, whose id is taken or whose file
cannot be read is skipped; the plugin's row in Settings → Plugins says which theme and why. The
plugin's other themes still load.

**Trust.** Theme CSS cannot run code and has no access to files or the network beyond loading
its own fonts and images, but it can restyle everything, including hiding parts of the app.

### Two ways to make one

**The quick path: the theme editor.** Build the theme in the app ("Create theme…"), then
"Export as plugin…". The exported folder is a complete plugin; install it on another PC or share
it as a `.zip`. To keep changing it, edit the user theme and export again (into an empty folder).

**The full path: a hand-written plugin**, for layout changes, fonts, several themes in one
plugin, or anything else beyond variables:

1. Make a folder with a `plugin.json` like the one above (or copy
   [`examples/plugins/example-theme`](../examples/plugins/example-theme)).
2. Settings → Plugins → "Install from folder…", then turn on **Reload on change** in the plugin's
   row and choose the theme in Settings → Appearance.
3. Edit the CSS (or `plugin.json`) and save. Questkeep copies the folder again and replaces the
   theme in place: the page updates within a moment, the chosen theme and its parameter values
   stay. User themes that are based on the plugin's theme follow too.
4. The theme editor is a good companion even then: "Customize…" on your theme shows every
   variable with the value your theme gives it, and what a derived color comes out as.

The CSS itself:

1. The theme's `id` is what `data-theme` will be; `css` is a file in the folder.
2. In the CSS, write every rule against `:root[data-theme="<id>"]`, with
   `[data-scheme="light"]` or `[data-scheme="dark"]` for values that differ per mode. Set at least
   `--qk-color-bg`, `-surface`, `-surface-raised`, `-surface-hover`, `-surface-active`, `-border`,
   `-text`, `-text-muted` and `-text-faint` for both modes; everything you leave out keeps the
   default theme's value for that mode.
3. Give it an `accent` color parameter with a default per mode. Add other parameters as needed and
   use them through `--qk-param-<key>` and `data-param-<key>`.
4. Keep text readable: at least 4.5:1 for text on the surfaces and for the text on the accent
   (the built-in themes are held to that by `ui/src/themes/themes.test.ts`).
5. Prefer variables over rules: `ui/src/theme.css` lists all of them with comments, and the theme
   editor shows them grouped with their current values.
6. Rules can go beyond variables, up to a different layout. The app shell is a grid (`.app`: the
   title bar across the top, `.sidebar` and `.app__main` below it); the example theme moves the
   sidebar to the right with grid placement alone, switched by a `select` parameter:

   ```css
   :root[data-theme="example-theme"][data-param-sidebar="right"] .app {
     grid-template-columns: minmax(0, 1fr) var(--qk-sidebar-width);
   }
   :root[data-theme="example-theme"][data-param-sidebar="right"] .app__main {
     grid-column: 1;
     grid-row: 2;
   }
   :root[data-theme="example-theme"][data-param-sidebar="right"] .sidebar {
     grid-column: 2;
     grid-row: 2;
   }
   ```

   The app's class names are not a stable interface the way the variables are: a rule may need
   adjusting after an update. Sizes that scripts read (row heights, grid metrics, the panel
   widths) must stay plain lengths in their variables.

Built-in themes are made the same way, in `ui/src/themes/` (`<id>.css` plus the definition in
`index.ts`).
