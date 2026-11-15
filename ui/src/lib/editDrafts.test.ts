import { describe, expect, it } from "vitest";
import {
  fileAction,
  NEW_GAME_ID,
  newGame,
  newGameProfiles,
  profilesDraft,
  sameAction,
} from "./editDrafts";

describe("editDrafts", () => {
  it("makes a file launch with empty parts left out", () => {
    expect(fileAction(" C:\\a.exe ", "  ", " C:\\ ")).toEqual({
      kind: "file",
      target: "C:\\a.exe",
      args: null,
      workingDir: "C:\\",
      tracking: "default",
      trackingPath: null,
    });
  });

  it("drafts a manual game with one copy", () => {
    const game = newGame();
    expect(game.id).toBe(NEW_GAME_ID);
    expect(game.source).toBe("manual");
    expect(game.copies.map((copy) => copy.id)).toEqual([NEW_GAME_ID]);
  });

  it("maps the new game's ids when saving its profiles", () => {
    const profiles = newGameProfiles("Executable");
    const [first] = profiles.profiles;
    if (!first) throw new Error("no profile");
    const own = {
      ...first,
      id: "p2",
      own: true,
      customName: "Windowed",
      args: "-w",
      action: null,
    };
    const draft = profilesDraft(
      { ...profiles, profiles: [...profiles.profiles, own], defaultId: "p2" },
      (id) => (id === NEW_GAME_ID ? "g9" : id),
    );
    expect(draft).toEqual({
      profiles: [
        { id: "g9", own: false, name: null, copyId: null, args: null, action: null },
        { id: "p2", own: true, name: "Windowed", copyId: "g9", args: "-w", action: null },
      ],
      defaultId: "p2",
    });
  });

  it("compares launches by value", () => {
    expect(sameAction(fileAction("a", "", ""), fileAction("a", "", ""))).toBe(true);
    expect(sameAction(fileAction("a", "", ""), fileAction("b", "", ""))).toBe(false);
    expect(sameAction(null, null)).toBe(true);
  });
});
