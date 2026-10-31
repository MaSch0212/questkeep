import { describe, expect, it } from "vitest";
import {
  activitySummary,
  exploreFor,
  formatHours,
  linksThatFit,
  rarestUnlocked,
  ttbBar,
} from "./gamePage";

const H = 3600;

describe("linksThatFit", () => {
  it("shows all when they fit, else leaves room for the more button", () => {
    expect(linksThatFit([50, 50, 50], 160, 40, 5)).toBe(3);
    // 40 (more) + 5 + 50 + 5 + 50 = 150 <= 155, a third would need 205.
    expect(linksThatFit([50, 50, 50, 50], 155, 40, 5)).toBe(2);
    expect(linksThatFit([200], 100, 40, 5)).toBe(0);
  });
});

describe("ttbBar", () => {
  it("stacks the lengths and scales to the longest", () => {
    const bar = ttbBar({ main: 27 * H, extra: 42 * H, complete: 105 * H }, 146 * H);
    expect(bar?.segments.map((s) => [s.key, Math.round(s.left), Math.round(s.width)])).toEqual([
      ["main", 0, 18],
      ["extra", 18, 10],
      ["complete", 29, 43],
    ]);
    expect(bar?.you).toBe(100);
    expect(bar?.youAtEnd).toBe(true);
  });

  it("skips unknown lengths and has no marker without playtime", () => {
    const bar = ttbBar({ main: 10 * H, extra: null, complete: 20 * H }, 0);
    expect(bar?.segments.map((s) => s.key)).toEqual(["main", "complete"]);
    expect(bar?.you).toBeNull();
    expect(ttbBar({ main: null, extra: null, complete: null }, 5)).toBeNull();
    expect(ttbBar(null, 5)).toBeNull();
  });

  it("formats rough hours", () => {
    expect(formatHours(27.4 * H)).toBe("27h");
    expect(formatHours(600)).toBe("<1h");
  });
});

const a = (name: string, percent: number | null) => ({
  name,
  description: null,
  icon: null,
  unlockedAt: null,
  percent,
});

describe("rarestUnlocked", () => {
  it("picks the lowest percentage", () => {
    expect(
      rarestUnlocked({ total: 5, unlocked: [a("x", 50), a("y", 3.4), a("z", null)] })?.name,
    ).toBe("y");
    expect(rarestUnlocked({ total: 5, unlocked: [a("z", null)] })).toBeNull();
  });
});

describe("activitySummary", () => {
  it("sums weeks and scales bars to the busiest", () => {
    const weeks = [0, 3600, 7200].map((seconds, i) => ({ start: `2026-10-0${i + 1}`, seconds }));
    expect(activitySummary({ weeks, sessions: 3 })).toEqual({
      total: 10800,
      thisWeek: 7200,
      bars: [0, 50, 100],
    });
    expect(
      activitySummary({
        weeks: weeks.map((w) => Object.assign({}, w, { seconds: 0 })),
        sessions: 0,
      }),
    ).toBe(null);
  });
});

describe("exploreFor", () => {
  it("filters by the value's key", () => {
    expect(exploreFor("tag", "Open World").filter?.items).toEqual([
      { kind: "list", field: "tag", mode: "any", values: ["openworld"] },
    ]);
  });
});

describe("linkKind", () => {
  it("guesses icons from names and addresses", async () => {
    const { linkKind } = await import("./gamePage");
    expect(linkKind("Steam store page", "https://store.steampowered.com/app/1")).toBe("store");
    expect(linkKind("PCGamingWiki", "https://pcgamingwiki.com")).toBe("wiki");
    expect(linkKind("Discussions", "https://steamcommunity.com/app/1/discussions/")).toBe(
      "community",
    );
    expect(linkKind("Official website", "https://example.com")).toBe("web");
  });
});
