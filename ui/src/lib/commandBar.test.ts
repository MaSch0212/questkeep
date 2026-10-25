import { describe, expect, it } from "vitest";
import { makeGame } from "../test-utils";
import { COMMAND_RESULTS, rankMatches, splitMatch } from "./commandBar";

const game = (id: string, name: string, hidden = false) =>
  makeGame({ id, name, sortingName: name.toLowerCase(), hidden });

describe("command bar", () => {
  it("ranks visible games first, then earlier matches, then by name", () => {
    const ranked = rankMatches(
      [
        game("1", "The Witcher"),
        game("2", "Witcher 2"),
        game("3", "Witch Hunt", true),
        game("4", "Awitch"),
      ],
      "witch",
    );
    expect(ranked.map((g) => g.id)).toEqual(["2", "4", "1", "3"]);
  });

  it("keeps a handful of results", () => {
    const many = Array.from({ length: 20 }, (_, i) => makeGame({ id: `${i}`, name: `Game ${i}` }));
    expect(rankMatches(many, "game")).toHaveLength(COMMAND_RESULTS);
  });

  it("splits a name around the match, ignoring case", () => {
    expect(splitMatch("Hollow Knight", "kNI")).toEqual({
      before: "Hollow ",
      match: "Kni",
      after: "ght",
    });
    expect(splitMatch("Hades", "zz")).toEqual({ before: "Hades", match: "", after: "" });
    expect(splitMatch("Hades", " ")).toEqual({ before: "Hades", match: "", after: "" });
  });
});
