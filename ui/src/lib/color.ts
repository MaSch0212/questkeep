/** Color maths for themes: hex parsing, WCAG contrast and the CSS `color-mix()` in oklab/srgb. */

/** sRGB channels 0..1 and alpha 0..1. */
export interface Rgba {
  r: number;
  g: number;
  b: number;
  a: number;
}

const HEX = /^#([0-9a-f]{3}|[0-9a-f]{4}|[0-9a-f]{6}|[0-9a-f]{8})$/i;

/** Parses `#rgb`, `#rgba`, `#rrggbb` and `#rrggbbaa`; null for anything else. */
export function parseHex(text: string): Rgba | null {
  const value = text.trim();
  if (!HEX.test(value)) return null;
  let digits = value.slice(1);
  if (digits.length <= 4) digits = [...digits].map((d) => d + d).join("");
  const channel = (at: number) => Number.parseInt(digits.slice(at, at + 2), 16) / 255;
  return {
    r: channel(0),
    g: channel(2),
    b: channel(4),
    a: digits.length === 8 ? channel(6) : 1,
  };
}

const byte = (value: number) =>
  Math.round(Math.min(1, Math.max(0, value)) * 255)
    .toString(16)
    .padStart(2, "0");

/** `#rrggbb`, or `#rrggbbaa` when not opaque. */
export function toHex(color: Rgba): string {
  const alpha = color.a < 1 ? byte(color.a) : "";
  return `#${byte(color.r)}${byte(color.g)}${byte(color.b)}${alpha}`;
}

/** An opaque color as lower-case `#rrggbb` (what `<input type="color">` takes); null if invalid. */
export function normalizeHex(text: string): string | null {
  const color = parseHex(text);
  return color ? toHex({ ...color, a: 1 }) : null;
}

const toLinear = (c: number) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
const toGamma = (c: number) => (c <= 0.0031308 ? c * 12.92 : 1.055 * c ** (1 / 2.4) - 0.055);
/** A linear channel as an sRGB one, kept within 0..1 (colors outside the gamut are clipped). */
const clamp = (c: number) => Math.min(1, Math.max(0, toGamma(c)));

/** WCAG relative luminance (alpha ignored). */
export function luminance(color: Rgba): number {
  return 0.2126 * toLinear(color.r) + 0.7152 * toLinear(color.g) + 0.0722 * toLinear(color.b);
}

/** `over` painted on the opaque `under`. */
export function composite(over: Rgba, under: Rgba): Rgba {
  const mix = (a: number, b: number) => a * over.a + b * (1 - over.a);
  return { r: mix(over.r, under.r), g: mix(over.g, under.g), b: mix(over.b, under.b), a: 1 };
}

/** WCAG contrast ratio (1..21) of a text color on a background; translucent text is blended. */
export function contrastRatio(text: Rgba, background: Rgba): number {
  const a = luminance(text.a < 1 ? composite(text, background) : text);
  const b = luminance(background);
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}

/** Dark and light text for colored backgrounds. */
export const ON_COLOR_DARK = "#0e1220";
export const ON_COLOR_LIGHT = "#ffffff";

/**
 * The text color that reads on `background`: dark or white, whichever contrasts more. For the few
 * mid-tones where neither reaches WCAG AA (4.5), pure black does.
 */
export function contrastColor(background: string): string {
  const color = parseHex(background);
  if (!color) return ON_COLOR_LIGHT;
  const dark = contrastRatio(parseHex(ON_COLOR_DARK) as Rgba, color);
  const light = contrastRatio(parseHex(ON_COLOR_LIGHT) as Rgba, color);
  if (Math.max(dark, light) < 4.5) return "#000000";
  return dark >= light ? ON_COLOR_DARK : ON_COLOR_LIGHT;
}

type Lab = [number, number, number];

function toOklab(color: Rgba): Lab {
  const r = toLinear(color.r);
  const g = toLinear(color.g);
  const b = toLinear(color.b);
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  return [
    0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
    1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
    0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
  ];
}

function fromOklab([L, A, B]: Lab, alpha: number): Rgba {
  const l = (L + 0.3963377774 * A + 0.2158037573 * B) ** 3;
  const m = (L - 0.1055613458 * A - 0.0638541728 * B) ** 3;
  const s = (L - 0.0894841775 * A - 1.291485548 * B) ** 3;
  return {
    r: clamp(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s),
    g: clamp(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s),
    b: clamp(-0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s),
    a: alpha,
  };
}

/**
 * CSS `color-mix(in <space>, first p1, second p2)` for the oklab and srgb spaces. Percentages are
 * 0..100 or undefined (as in CSS: a missing one is the rest to 100; both missing is half each).
 */
export function colorMix(
  space: "oklab" | "srgb",
  first: Rgba,
  p1: number | undefined,
  second: Rgba,
  p2: number | undefined,
): Rgba {
  let w1 = p1 ?? (p2 === undefined ? 50 : 100 - p2);
  let w2 = p2 ?? 100 - w1;
  const sum = w1 + w2;
  if (sum <= 0) return { r: 0, g: 0, b: 0, a: 0 };
  // Weights below 100% in total make the result that much more transparent.
  const alphaScale = sum < 100 ? sum / 100 : 1;
  w1 /= sum;
  w2 /= sum;
  const coords = (c: Rgba): Lab => (space === "oklab" ? toOklab(c) : [c.r, c.g, c.b]);
  const a = coords(first);
  const b = coords(second);
  const alpha = first.a * w1 + second.a * w2;
  // Premultiplied interpolation: a transparent color adds nothing but transparency.
  const mixed = a.map((value, i) =>
    alpha === 0 ? 0 : (value * first.a * w1 + (b[i] as number) * second.a * w2) / alpha,
  ) as Lab;
  const result =
    space === "oklab"
      ? fromOklab(mixed, alpha)
      : { r: mixed[0], g: mixed[1], b: mixed[2], a: alpha };
  return { ...result, a: result.a * alphaScale };
}
