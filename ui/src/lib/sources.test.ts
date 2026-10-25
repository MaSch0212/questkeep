import { describe, expect, it } from "vitest";
import { sourceInfo } from "./sources";

describe("sourceInfo", () => {
  it("uses brand glyphs for the stores Simple Icons has", () => {
    for (const id of ["steam", "epic", "gog", "ubisoft", "heroic", "lutris"]) {
      const info = sourceInfo(id);
      expect(info.glyph).toMatchObject({ kind: "path", path: expect.stringMatching(/^[Mm]/) });
    }
  });

  it("falls back to lucide glyphs for xbox, amazon and manual", () => {
    expect(sourceInfo("xbox").glyph).toEqual({ kind: "fallback", icon: "gamepad" });
    expect(sourceInfo("amazon").glyph).toEqual({ kind: "fallback", icon: "bag" });
    expect(sourceInfo("manual")).toMatchObject({
      name: "Manual",
      glyph: { kind: "fallback", icon: "folder" },
    });
  });

  it("prefers the provider's display name", () => {
    const providers = [{ id: "steam", name: "Steam (Valve)" }];
    expect(sourceInfo("steam", providers).name).toBe("Steam (Valve)");
    expect(sourceInfo("epic", providers).name).toBe("Epic Games");
  });

  it("shows unknown ids with a generic glyph and the provider name or the id", () => {
    expect(sourceInfo("itch")).toEqual({
      id: "itch",
      name: "itch",
      glyph: { kind: "fallback", icon: "generic" },
    });
    expect(sourceInfo("itch", [{ id: "itch", name: "itch.io" }]).name).toBe("itch.io");
  });
});
