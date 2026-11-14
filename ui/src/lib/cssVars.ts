/**
 * A small reader for theme CSS: which custom properties a set of stylesheets gives `<html>` for
 * a scheme, theme and parameters, and what color a variable ends up as. It understands what
 * themes use (rules on `:root` with attribute selectors, `var()`, hex colors, `color-mix()`), not
 * CSS in general. Used to check themes (contrast tests) and their previews.
 */
import { colorMix, parseHex, type Rgba } from "./color";

/** The attributes of `<html>` a rule is matched against (`data-scheme`, `data-theme`, ...). */
export type RootAttrs = Record<string, string>;

interface Rule {
  selectors: string[];
  declarations: [name: string, value: string][];
}

const stripComments = (css: string) => css.replace(/\/\*[\s\S]*?\*\//g, "");

/** The index of the `}` that closes the block opened just before `from`. */
function blockEnd(css: string, from: number): number {
  let depth = 1;
  for (let i = from; i < css.length; i++) {
    if (css[i] === "{") depth++;
    else if (css[i] === "}" && --depth === 0) return i;
  }
  return css.length;
}

/** Style rules in order. `@supports` blocks count as supported; other at-rules are skipped. */
function parseRules(source: string): Rule[] {
  const css = stripComments(source);
  const rules: Rule[] = [];
  let at = 0;
  while (at < css.length) {
    const open = css.indexOf("{", at);
    if (open < 0) break;
    const close = blockEnd(css, open + 1);
    const prelude = css.slice(at, open).trim();
    const body = css.slice(open + 1, close);
    if (prelude.startsWith("@supports")) rules.push(...parseRules(body));
    else if (!prelude.startsWith("@")) {
      rules.push({
        selectors: prelude.split(",").map((s) => s.trim()),
        declarations: body
          .split(";")
          .map((d) => d.trim())
          .filter((d) => d.includes(":"))
          .map((d) => {
            const colon = d.indexOf(":");
            return [
              d.slice(0, colon).trim(),
              d
                .slice(colon + 1)
                .trim()
                .replace(/\s+/g, " "),
            ];
          }),
      });
    }
    at = close + 1;
  }
  return rules;
}

const ROOT_SELECTOR = /^:root((?:\[[\w-]+(?:="[^"]*")?\])*)$/;

/** How many attribute selectors a matching `:root[...]` selector has; null when it doesn't match. */
function specificity(selector: string, attrs: RootAttrs): number | null {
  const match = ROOT_SELECTOR.exec(selector);
  if (!match) return null;
  const conditions = [...(match[1] ?? "").matchAll(/\[([\w-]+)(?:="([^"]*)")?\]/g)];
  for (const [, name, value] of conditions) {
    const actual = attrs[name as string];
    if (actual === undefined || (value !== undefined && actual !== value)) return null;
  }
  return conditions.length;
}

/**
 * The custom properties `<html>` gets from `sheets` (in document order) with the given
 * attributes, cascaded like CSS: more attribute selectors win, then the later rule. `inline`
 * properties (the theme parameters) win over everything.
 */
export function rootVars(
  sheets: string[],
  attrs: RootAttrs,
  inline: Record<string, string> = {},
): Map<string, string> {
  const found: { weight: number; order: number; name: string; value: string }[] = [];
  let order = 0;
  for (const rule of sheets.flatMap(parseRules)) {
    const weights = rule.selectors
      .map((s) => specificity(s, attrs))
      .filter((w): w is number => w !== null);
    if (weights.length === 0) continue;
    const weight = Math.max(...weights);
    for (const [name, value] of rule.declarations) {
      if (name.startsWith("--")) found.push({ weight, order: order++, name, value });
    }
  }
  found.sort((a, b) => a.weight - b.weight || a.order - b.order);
  const vars = new Map<string, string>();
  for (const { name, value } of found) vars.set(name, value);
  for (const [name, value] of Object.entries(inline)) vars.set(name, value);
  return vars;
}

/** Splits at top-level commas (not those inside parentheses). */
function splitArgs(text: string): string[] {
  const parts: string[] = [];
  let depth = 0;
  let start = 0;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (c === "(") depth++;
    else if (c === ")") depth--;
    else if (c === "," && depth === 0) {
      parts.push(text.slice(start, i));
      start = i + 1;
    }
  }
  parts.push(text.slice(start));
  return parts.map((p) => p.trim());
}

/** The index of the `)` matching the `(` at `open`. */
function closing(text: string, open: number): number {
  let depth = 0;
  for (let i = open; i < text.length; i++) {
    if (text[i] === "(") depth++;
    else if (text[i] === ")" && --depth === 0) return i;
  }
  return text.length - 1;
}

/** Replaces every `var(--name[, fallback])` by its value (or the fallback), recursively. */
export function substituteVars(value: string, vars: Map<string, string>, depth = 0): string {
  if (depth > 32) throw new Error(`var() cycle in "${value}"`);
  let text = value;
  for (;;) {
    const at = text.indexOf("var(");
    if (at < 0) return text;
    const end = closing(text, at + 3);
    const [name = "", ...fallback] = splitArgs(text.slice(at + 4, end));
    const found = vars.get(name);
    const replacement =
      found !== undefined
        ? substituteVars(found, vars, depth + 1)
        : fallback.length > 0
          ? substituteVars(fallback.join(", "), vars, depth + 1)
          : null;
    if (replacement === null) throw new Error(`${name} is not defined`);
    text = text.slice(0, at) + replacement + text.slice(end + 1);
  }
}

/** A color and an optional percentage: `#fff 20%`, `20% #fff`, `color-mix(...)`. */
function mixPart(text: string): { color: Rgba; percent: number | undefined } {
  const percent = /(?:^|\s)(-?[\d.]+)%(?:\s|$)/.exec(
    // Percentages inside a nested color-mix() belong to that one.
    text.replace(/\([^()]*(?:\([^()]*\)[^()]*)*\)/g, (inner) => " ".repeat(inner.length)),
  );
  if (!percent) return { color: parseColor(text), percent: undefined };
  const start = percent.index;
  const rest = text.slice(0, start) + " " + text.slice(start + percent[0].length);
  return { color: parseColor(rest), percent: Number(percent[1]) };
}

/** A color without variables: hex, `transparent`, `white`, `black` or `color-mix()`. */
export function parseColor(text: string): Rgba {
  const value = text.trim();
  if (value === "transparent") return { r: 0, g: 0, b: 0, a: 0 };
  if (value === "white") return { r: 1, g: 1, b: 1, a: 1 };
  if (value === "black") return { r: 0, g: 0, b: 0, a: 1 };
  const hex = parseHex(value);
  if (hex) return hex;
  const mix = /^color-mix\(\s*in\s+(oklab|srgb)\s*,/.exec(value);
  if (mix && value.endsWith(")")) {
    const [first, second] = splitArgs(value.slice(mix[0].length, -1));
    if (first !== undefined && second !== undefined) {
      const a = mixPart(first);
      const b = mixPart(second);
      return colorMix(mix[1] as "oklab" | "srgb", a.color, a.percent, b.color, b.percent);
    }
  }
  throw new Error(`Cannot read the color "${text}"`);
}

/** The color a variable resolves to. Throws when it is missing or not a color this reads. */
export function resolveColor(name: string, vars: Map<string, string>): Rgba {
  return parseColor(substituteVars(`var(${name})`, vars));
}
