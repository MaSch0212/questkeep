import { describe, expect, it } from "vitest";
import {
  coerceValue,
  isPluginPath,
  normalizeColor,
  normalizeSettings,
  parseExtension,
  settingDefaults,
} from "./declaredSettings";

describe("normalizeSettings", () => {
  it("fills in what a declaration leaves out", () => {
    expect(normalizeSettings([{ key: "on", label: "On", type: "toggle" }])).toEqual([
      {
        key: "on",
        label: "On",
        hint: null,
        type: "toggle",
        default: false,
        min: null,
        max: null,
        step: null,
        options: [],
      },
    ]);
  });

  it("keeps declared defaults that fit and replaces the ones that don't", () => {
    const settings = normalizeSettings([
      { key: "a", label: "A", type: "toggle", default: true },
      { key: "b", label: "B", type: "text", default: 5 },
      { key: "c", label: "C", type: "number", default: 50, min: 1, max: 10 },
      { key: "d", label: "D", type: "number", min: 3 },
      {
        key: "e",
        label: "E",
        type: "select",
        default: "nope",
        options: [{ value: "x", label: "X" }, { value: "y" }, { value: "x", label: "again" }, 7],
      },
      { key: "f", label: "F", type: "color", default: "#ABC" },
      { key: "g", label: "G", type: "color", default: "red" },
    ]);
    expect(settingDefaults(settings)).toEqual({
      a: true,
      b: "",
      c: 10,
      d: 3,
      e: "x",
      f: "#aabbcc",
      g: "#000000",
    });
    expect(settings[4]?.options).toEqual([
      { value: "x", label: "X" },
      { value: "y", label: "y" },
    ]);
  });

  it("leaves out unusable entries and repeated keys", () => {
    const settings = normalizeSettings([
      null,
      "text",
      { label: "No key", type: "toggle" },
      { key: "nolabel", type: "toggle" },
      { key: "weird", label: "Weird", type: "slider" },
      { key: "empty", label: "Empty select", type: "select", options: [] },
      { key: "ok", label: "First", type: "text", hint: "Kept" },
      { key: "ok", label: "Second", type: "toggle" },
    ]);
    expect(settings.map((s) => [s.key, s.label, s.hint])).toEqual([["ok", "First", "Kept"]]);
    expect(normalizeSettings(undefined)).toEqual([]);
    expect(normalizeSettings({ key: "a" })).toEqual([]);
  });

  it("ignores number limits that make no sense", () => {
    const [setting] = normalizeSettings([
      { key: "n", label: "N", type: "number", min: 5, max: 1, step: 0 },
    ]);
    expect(setting).toMatchObject({ min: 5, max: null, step: null, default: 5 });
  });
});

describe("coerceValue", () => {
  const [toggle, text, number, select, color] = normalizeSettings([
    { key: "t", label: "T", type: "toggle" },
    { key: "x", label: "X", type: "text" },
    { key: "n", label: "N", type: "number", min: 0, max: 10 },
    { key: "s", label: "S", type: "select", options: [{ value: "a", label: "A" }] },
    { key: "c", label: "C", type: "color" },
  ]);
  if (!toggle || !text || !number || !select || !color) throw new Error("settings missing");

  it("accepts values of the setting's type", () => {
    expect(coerceValue(toggle, true)).toBe(true);
    expect(coerceValue(text, "hi")).toBe("hi");
    expect(coerceValue(number, 4)).toBe(4);
    expect(coerceValue(number, "7.5")).toBe(7.5);
    expect(coerceValue(select, "a")).toBe("a");
    expect(coerceValue(color, "#FF0000")).toBe("#ff0000");
  });

  it("clamps numbers to the declared range", () => {
    expect(coerceValue(number, 99)).toBe(10);
    expect(coerceValue(number, "-3")).toBe(0);
  });

  it("rejects everything else", () => {
    expect(coerceValue(toggle, "true")).toBeUndefined();
    expect(coerceValue(text, 1)).toBeUndefined();
    expect(coerceValue(number, "abc")).toBeUndefined();
    expect(coerceValue(number, "")).toBeUndefined();
    expect(coerceValue(number, Number.NaN)).toBeUndefined();
    expect(coerceValue(select, "b")).toBeUndefined();
    expect(coerceValue(color, "blue")).toBeUndefined();
  });
});

describe("parseExtension", () => {
  it("reads main, styles and settings and ignores unknown keys", () => {
    expect(
      parseExtension({
        main: "./dist/main.js",
        styles: ["style.css", "../outside.css", 3, "sub/more.css"],
        settings: [{ key: "a", label: "A", type: "toggle" }],
        future: true,
      }),
    ).toMatchObject({
      main: "./dist/main.js",
      styles: ["style.css", "sub/more.css"],
      settings: [{ key: "a" }],
    });
  });

  it("needs a main file inside the plugin folder", () => {
    expect(parseExtension(null)).toBeNull();
    expect(parseExtension({})).toBeNull();
    expect(parseExtension({ main: "" })).toBeNull();
    expect(parseExtension({ main: "/abs/main.js" })).toBeNull();
    expect(parseExtension({ main: "../main.js" })).toBeNull();
    expect(parseExtension({ main: "https://example.com/main.js" })).toBeNull();
    expect(parseExtension({ main: "a\\b.js" })).toBeNull();
  });
});

describe("helpers", () => {
  it("knows plugin paths and colors", () => {
    expect(isPluginPath("main.js")).toBe(true);
    expect(isPluginPath("a/b/c.css")).toBe(true);
    expect(isPluginPath("a//b.css")).toBe(false);
    expect(isPluginPath("main.js?x=1")).toBe(false);
    expect(normalizeColor(" #AbCdEf ")).toBe("#abcdef");
    expect(normalizeColor("#abcd")).toBeNull();
    expect(normalizeColor(5)).toBeNull();
  });
});
