/**
 * The built-in themes. Each has a CSS file next to this one (rules against
 * `:root[data-theme="<id>"]`) and its definition here. Together with theme.css this folder is
 * the only place for literal colors and sizes. lib/themes.ts registers them.
 *
 * Names are not translated; descriptions, labels and hints are message keys of i18n/themes.ts.
 * `preview` repeats four colors of the CSS (background, surface, text, accent); a test keeps
 * them equal.
 */
import { translator } from "../i18n";
import { themesText } from "../i18n/themes";
import type { ThemeDef, ThemeParam } from "../lib/themes";
import consoleCss from "./console.css?raw";
import contrastCss from "./contrast.css?raw";
import emberCss from "./ember.css?raw";
import fjordCss from "./fjord.css?raw";
import forestCss from "./forest.css?raw";
import midnightCss from "./midnight.css?raw";
import orchidCss from "./orchid.css?raw";
import questkeepCss from "./questkeep.css?raw";

const tr = translator(themesText);
type TextKey = keyof typeof themesText.en;

/** A message of i18n/themes.ts by its key; anything else as it is. */
const text = (key: string) => (Object.hasOwn(themesText.en, key) ? tr(key as TextKey) : key);

const key = (name: TextKey): string => name;

/** Every theme has the accent color as its first parameter. */
const accent = (dark: string, light: string): ThemeParam => ({
  key: "accent",
  label: key("accent"),
  type: "color",
  default: { light, dark },
});

const builtin = (
  def: Pick<ThemeDef, "id" | "name" | "css" | "params" | "preview"> & { description: TextKey },
): ThemeDef => ({ ...def, origin: "builtin", author: "Questkeep", text });

export const BUILTIN_THEMES: ThemeDef[] = [
  builtin({
    id: "questkeep",
    name: "Questkeep",
    description: "questkeepDescription",
    css: questkeepCss,
    preview: {
      dark: ["#111317", "#171a20", "#e6e8ec", "#7c9cff"],
      light: ["#f3f4f7", "#fafbfc", "#1b1e26", "#3d5fd9"],
    },
    params: [
      accent("#7c9cff", "#3d5fd9"),
      {
        key: "corners",
        label: key("corners"),
        type: "select",
        default: "default",
        options: [
          { value: "sharp", label: key("cornersSharp") },
          { value: "default", label: key("optionDefault") },
          { value: "round", label: key("cornersRound") },
        ],
      },
      {
        key: "density",
        label: key("density"),
        type: "select",
        default: "comfortable",
        options: [
          { value: "compact", label: key("densityCompact") },
          { value: "comfortable", label: key("densityComfortable") },
          { value: "spacious", label: key("densitySpacious") },
        ],
      },
      {
        key: "text-size",
        label: key("textSize"),
        type: "number",
        default: 100,
        min: 85,
        max: 130,
        step: 5,
      },
      {
        key: "tint",
        label: key("tint"),
        type: "color",
        default: { dark: "#7c9cff", light: "#3d5fd9" },
        advanced: true,
      },
      {
        key: "tint-strength",
        label: key("tintStrength"),
        type: "number",
        default: 0,
        min: 0,
        max: 30,
        step: 1,
        unit: "%",
        advanced: true,
        hint: key("tintHint"),
      },
      {
        key: "font",
        label: key("font"),
        type: "select",
        default: "system",
        options: [
          { value: "system", label: key("fontSystem") },
          { value: "serif", label: key("fontSerif") },
          { value: "mono", label: key("fontMono") },
          { value: "rounded", label: key("fontRounded") },
        ],
        advanced: true,
        hint: key("fontHint"),
      },
      { key: "flat", label: key("flat"), type: "toggle", default: false, advanced: true },
    ],
  }),
  builtin({
    id: "midnight",
    name: "Midnight",
    description: "midnightDescription",
    css: midnightCss,
    preview: {
      dark: ["#000000", "#060709", "#eceef2", "#f0c267"],
      light: ["#ffffff", "#fafafa", "#111214", "#8a5a00"],
    },
    params: [
      accent("#f0c267", "#8a5a00"),
      { key: "separators", label: key("separators"), type: "toggle", default: true },
    ],
  }),
  builtin({
    id: "fjord",
    name: "Fjord",
    description: "fjordDescription",
    css: fjordCss,
    preview: {
      dark: ["#131a23", "#18212c", "#e3eaf2", "#6cc5e8"],
      light: ["#edf1f5", "#f6f8fb", "#17212c", "#0f6f96"],
    },
    params: [accent("#6cc5e8", "#0f6f96")],
  }),
  builtin({
    id: "ember",
    name: "Ember",
    description: "emberDescription",
    css: emberCss,
    preview: {
      dark: ["#161210", "#1d1815", "#f1e9e2", "#ff9b54"],
      light: ["#f7f1e8", "#fcf8f1", "#2a211a", "#ad4a0c"],
    },
    params: [accent("#ff9b54", "#ad4a0c")],
  }),
  builtin({
    id: "forest",
    name: "Forest",
    description: "forestDescription",
    css: forestCss,
    preview: {
      dark: ["#0e1511", "#131c17", "#e4ede7", "#7fd49b"],
      light: ["#eff4ee", "#f7faf6", "#17211a", "#23743f"],
    },
    params: [accent("#7fd49b", "#23743f")],
  }),
  builtin({
    id: "orchid",
    name: "Orchid",
    description: "orchidDescription",
    css: orchidCss,
    preview: {
      dark: ["#15111b", "#1b1623", "#ede7f4", "#e58bd8"],
      light: ["#f6f1f8", "#fbf8fc", "#231a2b", "#a02f93"],
    },
    params: [accent("#e58bd8", "#a02f93")],
  }),
  builtin({
    id: "contrast",
    name: "High contrast",
    description: "contrastDescription",
    css: contrastCss,
    preview: {
      dark: ["#000000", "#000000", "#ffffff", "#ffe14a"],
      light: ["#ffffff", "#ffffff", "#000000", "#0037c8"],
    },
    params: [
      accent("#ffe14a", "#0037c8"),
      {
        key: "focus-ring",
        label: key("focusRing"),
        type: "number",
        default: 3,
        min: 2,
        max: 6,
        step: 1,
        unit: "px",
      },
    ],
  }),
  builtin({
    id: "console",
    name: "Console",
    description: "consoleDescription",
    css: consoleCss,
    preview: {
      dark: ["#121213", "#19191b", "#f2f2f4", "#3fd6c4"],
      light: ["#f1f1f3", "#f9f9fa", "#18181b", "#0a7268"],
    },
    params: [
      accent("#3fd6c4", "#0a7268"),
      {
        key: "sidebar-width",
        label: key("sidebarWidth"),
        type: "number",
        default: 264,
        min: 200,
        max: 340,
        step: 4,
        unit: "px",
      },
      { key: "pills", label: key("pills"), type: "toggle", default: true },
    ],
  }),
];
