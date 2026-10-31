import { describe, expect, it } from "vitest";
import {
  defaultOrder,
  effectiveOrder,
  hasOwnOrder,
  moveSource,
  withFieldOrder,
  withGlobalOrder,
} from "./metadataPriority";

const sources = ["steam", "xbox", "epic", "igdb", "hltb", "gog"];
const none = { order: [], fields: {} };

describe("metadata priority", () => {
  it("defaults to IGDB, Steam, HLTB, then the rest by id; Steam first for scores", () => {
    expect(defaultOrder(null, sources)).toEqual(["igdb", "steam", "hltb", "epic", "gog", "xbox"]);
    expect(defaultOrder("criticScore", sources).slice(0, 2)).toEqual(["steam", "igdb"]);
  });

  it("completes orders with sources they miss and drops unknown ones", () => {
    const priority = withGlobalOrder(none, ["gog", "gone", "steam"]);
    expect(effectiveOrder(priority, null, sources)).toEqual([
      "gog",
      "steam",
      "igdb",
      "hltb",
      "epic",
      "xbox",
    ]);
    // A field follows the user's global order once there is one, Steam-first ones included.
    expect(effectiveOrder(priority, "features", sources)[0]).toBe("gog");
    expect(effectiveOrder(none, "features", sources)[0]).toBe("steam");
  });

  it("sets and clears a field's own order", () => {
    const own = withFieldOrder(none, "tags", ["xbox"]);
    expect(hasOwnOrder(own, "tags")).toBe(true);
    expect(effectiveOrder(own, "tags", sources)[0]).toBe("xbox");
    expect(hasOwnOrder(withFieldOrder(own, "tags", null), "tags")).toBe(false);
  });

  it("moves entries within bounds", () => {
    expect(moveSource(["a", "b", "c"], "c", -1)).toEqual(["a", "c", "b"]);
    expect(moveSource(["a", "b", "c"], "a", -1)).toEqual(["a", "b", "c"]);
  });
});
