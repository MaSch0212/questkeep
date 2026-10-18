import { describe, expect, it } from "vitest";
import { deriveGameName } from "./names";

describe("deriveGameName", () => {
  it("uses the file stem", () => {
    expect(deriveGameName("C:\\Games\\Hades\\Hades.exe")).toBe("Hades");
    expect(deriveGameName("/opt/games/celeste")).toBe("celeste");
  });
  it("uses the parent folder for generic stems", () => {
    expect(deriveGameName("C:\\Games\\Cool_Game\\launcher.exe")).toBe("Cool Game");
    expect(deriveGameName("D:/Stuff/Dark Souls/START.bat")).toBe("Dark Souls");
    expect(deriveGameName("D:/Stuff/Thing/Client.exe")).toBe("Thing");
  });
  it("replaces underscores and collapses whitespace", () => {
    expect(deriveGameName("C:\\a\\my__cool_game.exe")).toBe("my cool game");
  });
  it("falls back to the stem without a parent", () => {
    expect(deriveGameName("game.exe")).toBe("game");
  });
  it("returns empty for empty input", () => {
    expect(deriveGameName("")).toBe("");
  });
  it("keeps dotfiles intact", () => {
    expect(deriveGameName("/x/.hidden")).toBe(".hidden");
  });
});
