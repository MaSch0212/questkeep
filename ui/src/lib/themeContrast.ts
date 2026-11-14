/**
 * The contrast every theme is held to, as a check over the variables a theme gives `<html>` (see
 * `rootVars` in cssVars.ts). Used by the tests of the built-in themes and of the example theme
 * plugin; not part of the app.
 */
import { composite, contrastRatio } from "./color";
import { resolveColor } from "./cssVars";

/**
 * The pairs of text and background that are too close, as readable lines ("text on surface:
 * 3.90 < 4.5"); empty when the theme is fine. Throws when a color cannot be computed.
 */
export function contrastFailures(vars: Map<string, string>): string[] {
  const color = (name: string) => resolveColor(`--qk-color-${name}`, vars);
  const bg = color("bg");
  /** Contrast of `text` on `background`, itself painted on the page background if see-through. */
  const ratio = (text: string, background: string) => {
    const under = color(background);
    return contrastRatio(color(text), under.a < 1 ? composite(under, bg) : under);
  };
  const failures: string[] = [];
  const check = (text: string, background: string, min: number) => {
    const actual = ratio(text, background);
    if (actual < min) failures.push(`${text} on ${background}: ${actual.toFixed(2)} < ${min}`);
  };

  for (const surface of ["bg", "surface", "surface-raised", "surface-hover"]) {
    check("text", surface, 4.5);
    check("text-muted", surface, 3);
  }
  check("text", "surface-active", 4.5);
  check("text-muted", "bg", 4.5);
  check("text-faint", "bg", 3);
  check("text-faint", "surface", 3);
  // The accent: its text, as a text color itself, and as a soft background.
  check("accent-contrast", "accent", 4.5);
  check("accent-contrast", "accent-hover", 4.5);
  check("accent", "bg", 4.5);
  check("accent", "surface", 4.5);
  check("text", "accent-soft", 4.5);
  check("accent", "accent-soft", 3);
  check("text", "group-band", 4.5);
  // Status colors as text, and text on their soft backgrounds.
  for (const status of ["danger", "success", "warn"]) {
    check(status, "bg", 4.5);
    check(status, "surface-raised", 4.5);
    check("text", `${status}-soft`, 4.5);
    check(status, `${status}-soft`, 3);
  }
  check("warn", "warn-hover", 3);
  check("running", "surface", 3);
  check("star", "surface", 3);
  check("close-hover-text", "close-hover", 4.5);
  // Labels over images take the page's text color on their own background.
  check("text", "tile-date-bg", 4.5);
  check("text", "hero-button", 4.5);
  check("text-muted", "overlay", 4.5);
  check("running", "quick-bg", 3);
  check("text", "play", 4.5);
  check("title-on-cover", "tile-title-fade", 4.5);
  // Charts stand off their background.
  for (const series of ["series-1", "series-2", "series-3", "series-4", "series-5", "series-6"]) {
    check(series, "surface", 2.5);
  }
  check("heat-4", "heat-0", 2.5);
  return failures;
}
