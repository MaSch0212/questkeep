import { describe, expect, it } from "vitest";
import type { ProviderInfo } from "../bindings/ProviderInfo";
import { addGameMenuItems, nextMenuIndex } from "./addGameOptions";

const provider = (id: string, options: { id: string; label: string }[]): ProviderInfo => ({
  id,
  name: id.toUpperCase(),
  icon: "",
  canScan: true,
  canScanOwned: false,
  canUninstall: false,
  addGameOptions: options,
});

describe("addGameMenuItems", () => {
  it("flattens the options of every provider in order", () => {
    const items = addGameMenuItems([
      provider("steam", []),
      provider("prism", [{ id: "new-instance", label: "New Minecraft instance" }]),
      provider("x", [
        { id: "a", label: "A" },
        { id: "b", label: "B" },
      ]),
    ]);
    expect(items.map((i) => `${i.providerId}/${i.optionId}`)).toEqual([
      "prism/new-instance",
      "x/a",
      "x/b",
    ]);
    expect(items[0]).toMatchObject({ providerName: "PRISM", label: "New Minecraft instance" });
  });

  it("is empty without options", () => {
    expect(addGameMenuItems([provider("steam", [])])).toEqual([]);
  });
});

describe("nextMenuIndex", () => {
  it("wraps around and jumps to the ends", () => {
    expect(nextMenuIndex("ArrowDown", 2, 3)).toBe(0);
    expect(nextMenuIndex("ArrowUp", 0, 3)).toBe(2);
    expect(nextMenuIndex("Home", 2, 3)).toBe(0);
    expect(nextMenuIndex("End", 0, 3)).toBe(2);
  });

  it("ignores other keys and empty menus", () => {
    expect(nextMenuIndex("a", 0, 3)).toBeNull();
    expect(nextMenuIndex("ArrowDown", 0, 0)).toBeNull();
  });
});
