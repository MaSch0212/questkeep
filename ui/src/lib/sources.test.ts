import { describe, expect, it } from "vitest";
import { sourceInfo } from "./sources";

describe("sourceInfo", () => {
  const providers = [{ id: "steam", name: "Steam", icon: '<svg viewBox="0 0 24 24"></svg>' }];

  it("takes the name and logo from the provider", () => {
    expect(sourceInfo("steam", providers)).toEqual({
      id: "steam",
      name: "Steam",
      icon: '<svg viewBox="0 0 24 24"></svg>',
    });
  });

  it("shows unknown sources by id without a logo", () => {
    expect(sourceInfo("itch", providers)).toEqual({ id: "itch", name: "Itch", icon: null });
  });
});
