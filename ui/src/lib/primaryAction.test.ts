import { describe, expect, it } from "vitest";
import { primaryAction } from "./primaryAction";

describe("primaryAction", () => {
  it("plays installed games and shows running ones as running", () => {
    expect(primaryAction({ installed: true, sourceGameId: null }, false)).toBe("play");
    expect(primaryAction({ installed: true, sourceGameId: "1" }, true)).toBe("running");
  });

  it("installs store games that are not installed", () => {
    expect(primaryAction({ installed: false, sourceGameId: "440" }, false)).toBe("install");
  });

  it("offers nothing for a missing manual game", () => {
    expect(primaryAction({ installed: false, sourceGameId: null }, false)).toBeNull();
  });
});
