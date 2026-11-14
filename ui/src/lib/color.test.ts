import { describe, expect, it } from "vitest";
import {
  colorMix,
  composite,
  contrastColor,
  contrastRatio,
  normalizeHex,
  ON_COLOR_DARK,
  ON_COLOR_LIGHT,
  parseHex,
  toHex,
  type Rgba,
} from "./color";
import { parseColor, resolveColor, rootVars, substituteVars } from "./cssVars";

const hex = (text: string) => parseHex(text) as Rgba;

describe("color", () => {
  it("parses and writes hex colors", () => {
    expect(parseHex("#fff")).toEqual({ r: 1, g: 1, b: 1, a: 1 });
    expect(parseHex(" #0000ff80 ")?.a).toBeCloseTo(0.5, 2);
    expect(parseHex("#f008")?.a).toBeCloseTo(0.533, 2);
    expect(parseHex("red")).toBeNull();
    expect(parseHex("#12345")).toBeNull();
    expect(toHex(hex("#7C9CFF"))).toBe("#7c9cff");
    expect(toHex(hex("#7c9cff44"))).toBe("#7c9cff44");
    expect(normalizeHex("#ABC")).toBe("#aabbcc");
    expect(normalizeHex("#11223344")).toBe("#112233");
    expect(normalizeHex("blue")).toBeNull();
  });

  it("computes WCAG contrast", () => {
    expect(contrastRatio(hex("#000"), hex("#fff"))).toBeCloseTo(21, 5);
    expect(contrastRatio(hex("#fff"), hex("#fff"))).toBeCloseTo(1, 5);
    expect(contrastRatio(hex("#777777"), hex("#ffffff"))).toBeCloseTo(4.48, 2);
    // Translucent text is blended with the background first.
    expect(contrastRatio(hex("#00000080"), hex("#ffffff"))).toBeCloseTo(
      contrastRatio(composite(hex("#00000080"), hex("#ffffff")), hex("#ffffff")),
      5,
    );
  });

  it("picks dark or white text for a colored background", () => {
    expect(contrastColor("#7c9cff")).toBe(ON_COLOR_DARK);
    expect(contrastColor("#ffe14a")).toBe(ON_COLOR_DARK);
    expect(contrastColor("#3d5fd9")).toBe(ON_COLOR_LIGHT);
    expect(contrastColor("#000000")).toBe(ON_COLOR_LIGHT);
    expect(contrastColor("nonsense")).toBe(ON_COLOR_LIGHT);
    // Whatever the color, the pick reaches AA.
    const unreadable: string[] = [];
    for (let i = 0; i < 4096; i += 7) {
      const color = `#${i.toString(16).padStart(3, "0")}`;
      if (contrastRatio(hex(contrastColor(color)), hex(color)) < 4.5) unreadable.push(color);
    }
    expect(unreadable).toEqual([]);
  });

  it("mixes colors like CSS color-mix()", () => {
    const black = hex("#000");
    const white = hex("#fff");
    // 0% of the first color is the second one, untouched.
    expect(toHex(colorMix("oklab", hex("#f00"), 0, hex("#111317"), undefined))).toBe("#111317");
    expect(toHex(colorMix("srgb", black, undefined, white, undefined))).toBe("#808080");
    // Oklab is perceptual: half way between black and white is a darker grey than in sRGB.
    expect(toHex(colorMix("oklab", black, undefined, white, undefined))).toBe("#636363");
    expect(toHex(colorMix("oklab", black, 25, white, undefined))).toBe(
      toHex(colorMix("oklab", black, undefined, white, 75)),
    );
    // Mixing with transparent only fades the color.
    const faded = colorMix("oklab", hex("#7c9cff"), 27, { r: 0, g: 0, b: 0, a: 0 }, undefined);
    expect(toHex({ ...faded, a: 1 })).toBe("#7c9cff");
    expect(faded.a).toBeCloseTo(0.27, 5);
    // Percentages that add up to less than 100 make the result see-through.
    expect(colorMix("srgb", black, 20, white, 20).a).toBeCloseTo(0.4, 5);
  });
});

describe("cssVars", () => {
  const base = `
    /* a comment { with braces } */
    :root, :root[data-scheme="dark"] { --bg: #000; --text: #fff; --accent: var(--param-accent, #00f); }
    :root[data-scheme="light"] { --bg: #fff; --text: #000; }
    :root { --soft: color-mix(in oklab, var(--accent) var(--mix, 50%), var(--bg)); --size: 4px; }
    @media (prefers-reduced-motion: reduce) { :root { --size: 0px; } }
    .btn { --bg: #123456; }
  `;
  const theme = `
    :root[data-theme="t"] { --text: #eee; --mix: 0%; }
    @supports (color: color-mix(in oklab, red, blue)) {
      :root[data-theme="t"][data-scheme="light"] { --bg: #fafafa; }
    }
    :root[data-theme="t"][data-param-wide="1"] { --size: 8px; }
    :root[data-theme="other"] { --text: #f00; }
    :root[data-theme="t"] .btn { --text: #0f0; }
  `;

  it("cascades the rules that match the root attributes", () => {
    const dark = rootVars([base], { "data-scheme": "dark" });
    expect(dark.get("--bg")).toBe("#000");
    expect(dark.get("--size")).toBe("4px");
    const light = rootVars([base, theme], { "data-scheme": "light", "data-theme": "t" });
    expect(light.get("--bg")).toBe("#fafafa");
    expect(light.get("--text")).toBe("#eee");
    expect(light.get("--size")).toBe("4px");
    const wide = rootVars(
      [base, theme],
      { "data-scheme": "dark", "data-theme": "t", "data-param-wide": "1" },
      { "--param-accent": "#ff0000" },
    );
    expect(wide.get("--size")).toBe("8px");
    expect(wide.get("--bg")).toBe("#000");
    expect(toHex(resolveColor("--accent", wide))).toBe("#ff0000");
    // Without the attribute at all, only plain :root rules apply.
    expect(rootVars([base], {}).get("--bg")).toBe("#000");
  });

  it("resolves var() chains, fallbacks and color-mix()", () => {
    const vars = rootVars([base, theme], { "data-scheme": "dark", "data-theme": "t" });
    expect(substituteVars("1px solid var(--accent)", vars)).toBe("1px solid #00f");
    expect(toHex(resolveColor("--soft", vars))).toBe("#000000");
    const mixed = rootVars([base], { "data-scheme": "dark" });
    expect(toHex(resolveColor("--soft", mixed))).toBe(
      toHex(colorMix("oklab", hex("#00f"), 50, hex("#000"), undefined)),
    );
    expect(() => resolveColor("--missing", vars)).toThrow(/--missing is not defined/);
    expect(() => resolveColor("--size", vars)).toThrow(/Cannot read the color/);
    expect(() => substituteVars("var(--a)", new Map([["--a", "var(--a)"]]))).toThrow(/cycle/);
  });

  it("reads nested color-mix() with percentages on either side", () => {
    expect(toHex(parseColor("color-mix(in srgb, #000 25%, #fff)"))).toBe("#bfbfbf");
    expect(toHex(parseColor("color-mix(in srgb, 25% #000, #fff)"))).toBe("#bfbfbf");
    expect(toHex(parseColor("color-mix(in srgb, #000, #fff 75%)"))).toBe("#bfbfbf");
    expect(
      toHex(parseColor("color-mix(in srgb, color-mix(in srgb, #000 50%, #fff) 50%, white)")),
    ).toBe("#bfbfbf");
    expect(parseColor("transparent").a).toBe(0);
  });
});
