import { describe, expect, it } from "vitest";
import {
  formatDate,
  formatPlaytime,
  formatRelativeDate,
  formatReleaseDate,
  releaseYear,
} from "./format";

describe("formatPlaytime", () => {
  it("handles never played", () => {
    expect(formatPlaytime(0)).toBe("Never played");
    expect(formatPlaytime(-5)).toBe("Never played");
    expect(formatPlaytime(Number.NaN)).toBe("Never played");
  });
  it("handles sub-minute values", () => {
    expect(formatPlaytime(30)).toBe("Less than a minute");
  });
  it("formats minutes", () => {
    expect(formatPlaytime(45 * 60)).toBe("45m");
  });
  it("formats hours and minutes", () => {
    expect(formatPlaytime(12 * 3600 + 5 * 60 + 40)).toBe("12h 5m");
    expect(formatPlaytime(2 * 3600)).toBe("2h");
  });
});

describe("formatRelativeDate", () => {
  const now = new Date(2026, 5, 15, 12, 0, 0);
  it("handles null and invalid", () => {
    expect(formatRelativeDate(null, now)).toBe("Never");
    expect(formatRelativeDate("nope", now)).toBe("Unknown");
  });
  it("uses calendar days", () => {
    expect(formatRelativeDate(new Date(2026, 5, 15, 0, 5).toISOString(), now)).toBe("Today");
    expect(formatRelativeDate(new Date(2026, 5, 14, 23, 55).toISOString(), now)).toBe("Yesterday");
    expect(formatRelativeDate(new Date(2026, 5, 12, 8).toISOString(), now)).toBe("3 days ago");
  });
  it("falls back to the locale date", () => {
    const old = new Date(2026, 4, 1, 8);
    expect(formatRelativeDate(old.toISOString(), now)).toBe(old.toLocaleDateString());
  });
});

describe("formatReleaseDate", () => {
  it("formats full dates, months and bare years", () => {
    expect(formatReleaseDate("2020-09-17", "en-US")).toBe("September 17, 2020");
    expect(formatReleaseDate("2020-09", "en-US")).toBe("September 2020");
    expect(formatReleaseDate("2020", "en-US")).toBe("2020");
  });
  it("handles empty, bad months and unknown formats", () => {
    expect(formatReleaseDate(null)).toBeNull();
    expect(formatReleaseDate("  ")).toBeNull();
    expect(formatReleaseDate("2020-13")).toBe("2020");
    expect(formatReleaseDate("Coming soon")).toBe("Coming soon");
  });
  it("extracts the release year", () => {
    expect(releaseYear("2020-09-17")).toBe("2020");
    expect(releaseYear("1999")).toBe("1999");
    expect(releaseYear("soon")).toBeNull();
    expect(releaseYear(null)).toBeNull();
  });
});

describe("formatDate", () => {
  it("formats valid dates and rejects invalid ones", () => {
    const d = new Date(2026, 0, 2, 8);
    expect(formatDate(d.toISOString())).toBe(d.toLocaleDateString());
    expect(formatDate(null)).toBe("Unknown");
    expect(formatDate("x")).toBe("Unknown");
  });
});
