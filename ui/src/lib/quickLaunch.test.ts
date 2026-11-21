import { describe, expect, it } from "vitest";
import {
  moveQuickItemTo,
  parseQuickKey,
  quickKey,
  quickLaunchItems,
  withQuickItem,
} from "./quickLaunch";

describe("quick launch", () => {
  it("starts from the installed launchers until the user arranges it", () => {
    expect(quickLaunchItems(null, ["steam", "gog"])).toEqual([
      { kind: "store", id: "steam" },
      { kind: "store", id: "gog" },
    ]);
    const saved = [{ kind: "game" as const, id: "g1" }];
    expect(quickLaunchItems(saved, ["steam"])).toEqual(saved);
    expect(quickLaunchItems([], ["steam"])).toEqual([]);
  });

  it("keys entries and adds each once", () => {
    const app = { kind: "game" as const, id: "g1" };
    expect(quickKey(app)).toBe("game:g1");
    expect(parseQuickKey("store:steam")).toEqual({ kind: "store", id: "steam" });
    expect(parseQuickKey("game:")).toBeNull();
    expect(parseQuickKey("other:x")).toBeNull();
    expect(parseQuickKey("")).toBeNull();
    expect(withQuickItem([app], app)).toEqual([app]);
    expect(withQuickItem([], app)).toEqual([app]);
  });

  it("moves an entry to where another one is", () => {
    const a = { kind: "store" as const, id: "a" };
    const b = { kind: "store" as const, id: "b" };
    const c = { kind: "game" as const, id: "c" };
    expect(moveQuickItemTo([a, b, c], "store:a", "game:c")).toEqual([b, c, a]);
    expect(moveQuickItemTo([a, b, c], "game:c", "store:a")).toEqual([c, a, b]);
    expect(moveQuickItemTo([a, b, c], "store:a", "store:a")).toEqual([a, b, c]);
    expect(moveQuickItemTo([a, b, c], "store:x", "store:a")).toEqual([a, b, c]);
  });
});
