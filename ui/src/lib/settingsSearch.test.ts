import { describe, expect, it } from "vitest";
import { normalize, searchEntries, type SearchEntry } from "./settingsSearch";

const entry = (id: string, label: string, keywords?: string): SearchEntry => ({
  id,
  page: "general",
  label,
  keywords,
});

const ENTRIES = [
  entry("a", "Close to the tray instead of quitting"),
  entry("b", "Tray icon"),
  entry("c", "Quick launch rows", "tray popup"),
  entry("d", "Ignore words at the start of names when sorting"),
  entry("e", "Größe der Übersicht"),
];

describe("settings search", () => {
  it("normalizes case, accents and spaces", () => {
    expect(normalize("  Café   ÜBER ")).toBe("cafe uber");
  });

  it("finds nothing for an empty query", () => {
    expect(searchEntries(ENTRIES, "   ")).toEqual([]);
  });

  it("needs every word, in any order, in the label or the keywords", () => {
    expect(searchEntries(ENTRIES, "tray close").map((e) => e.id)).toEqual(["a"]);
    expect(searchEntries(ENTRIES, "popup tray").map((e) => e.id)).toEqual(["c"]);
    expect(searchEntries(ENTRIES, "nothing like this")).toEqual([]);
  });

  it("puts labels starting with the query first, then words starting with it", () => {
    expect(searchEntries(ENTRIES, "tray").map((e) => e.id)).toEqual(["b", "a", "c"]);
    expect(searchEntries(ENTRIES, "ray").map((e) => e.id)).toEqual(["a", "b", "c"]);
  });

  it("ignores accents when matching", () => {
    expect(searchEntries(ENTRIES, "uber").map((e) => e.id)).toEqual(["e"]);
    expect(searchEntries(ENTRIES, "OBERSICHT")).toEqual([]);
  });

  it("returns at most the limit", () => {
    expect(searchEntries(ENTRIES, "r", 2)).toHaveLength(2);
  });
});
