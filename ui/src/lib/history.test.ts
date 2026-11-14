import { describe, expect, it } from "vitest";
import {
  EMPTY_HISTORY,
  type NavLocation,
  canGoBack,
  canGoForward,
  parseLocation,
  replaceCurrent,
  sameStep,
  step,
  visit,
} from "./history";

const view = (viewId: string, gameId: string | null = null): NavLocation => ({
  page: "library",
  viewId,
  gameId,
});
function must<T>(value: T | null | undefined): T {
  if (value === null || value === undefined) throw new Error("missing");
  return value;
}
const home: NavLocation = { page: "home", viewId: "all", gameId: null };

describe("navigation history", () => {
  it("goes back and forward through visited steps", () => {
    let h = visit(EMPTY_HISTORY, home);
    h = visit(h, view("all"));
    h = visit(h, view("fav", "g1"));
    expect(canGoBack(h)).toBe(true);
    expect(canGoForward(h)).toBe(false);
    const back = step(h, -1);
    expect(back?.location).toEqual(view("all"));
    const forward = step(must(back).history, 1);
    expect(forward?.location).toEqual(view("fav", "g1"));
    expect(step(must(forward).history, 1)).toBeNull();
  });

  it("drops the steps ahead when visiting after going back", () => {
    let h = visit(visit(visit(EMPTY_HISTORY, home), view("a")), view("b"));
    h = must(step(h, -1)).history;
    h = visit(h, view("c"));
    expect(h.entries).toEqual([home, view("a"), view("c")]);
    expect(canGoForward(h)).toBe(false);
  });

  it("keeps at most the limit", () => {
    let h = EMPTY_HISTORY;
    for (let i = 0; i < 10; i++) h = visit(h, view(`v${i}`), 3);
    expect(h.entries.map((e) => e.viewId)).toEqual(["v7", "v8", "v9"]);
    expect(h.index).toBe(2);
  });

  it("updates the current step in place", () => {
    const h = replaceCurrent(visit(EMPTY_HISTORY, view("a")), view("a", "g"));
    expect(h.entries).toEqual([view("a", "g")]);
    expect(replaceCurrent(EMPTY_HISTORY, home)).toEqual({ entries: [home], index: 0 });
  });

  it("compares steps by page and view", () => {
    expect(sameStep(view("a", "x"), view("a", "y"))).toBe(true);
    expect(sameStep(view("a"), view("b"))).toBe(false);
    expect(sameStep(home, { ...home, viewId: "other" })).toBe(true);
  });

  it("reads a saved location and rejects anything else", () => {
    expect(parseLocation(JSON.stringify(view("a", "g")))).toEqual(view("a", "g"));
    expect(parseLocation('{"page":"nope"}')).toBeNull();
    expect(parseLocation("not json")).toBeNull();
    expect(parseLocation(null)).toBeNull();
  });
});
