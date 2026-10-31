import { describe, expect, it } from "vitest";
import type { CompletionStatus } from "../bindings/CompletionStatus";
import { makeGame } from "../test-utils";
import { combineRows, fieldLabel, suggestedPlan, type CombineSource } from "./combinePlan";

const statuses: CompletionStatus[] = [
  { id: "not", name: "Not Played", gameCount: 0 },
  { id: "played", name: "Played", gameCount: 0 },
  { id: "beaten", name: "Beaten", gameCount: 0 },
];

const source = (overrides: Parameters<typeof makeGame>[0]): CombineSource => ({
  game: makeGame(overrides),
  edits: null,
  profiles: null,
});

const steam = source({
  id: "a",
  name: "Hades II",
  source: "steam",
  playtimeSeconds: 500,
  completionStatus: "played",
  userScore: null,
  favorite: false,
  categories: ["rpg"],
});
const xbox = source({
  id: "b",
  name: "Hades II",
  source: "xbox",
  playtimeSeconds: 40,
  completionStatus: "beaten",
  userScore: 85,
  favorite: true,
  categories: [],
});
const epic = source({
  id: "c",
  name: "HADES 2",
  source: "epic",
  playtimeSeconds: 0,
  completionStatus: null,
  userScore: 70,
  favorite: false,
  categories: [],
});

describe("suggestedPlan", () => {
  it("bases on the most played game and keeps the best of the rest", () => {
    expect(suggestedPlan([epic, xbox, steam], statuses)).toEqual({
      entries: ["c", "b", "a"],
      base: "a",
      nameFrom: "a",
      imagesFrom: "a",
      detailsFrom: "a",
      statusFrom: "b",
      scoreFrom: "b",
      favorite: true,
      profileFrom: "a",
      categoriesFrom: ["c", "b", "a"],
      notesFrom: ["c", "b", "a"],
      linksFrom: ["c", "b", "a"],
      profilesFrom: ["c", "b", "a"],
    });
  });
});

describe("combineRows", () => {
  it("shows only the values that differ", () => {
    const rows = combineRows([steam, xbox, epic], {
      status: (id) => statuses.find((s) => s.id === id)?.name ?? null,
      category: (id) => id.toUpperCase(),
      source: (id) => id,
    });
    expect(rows.map((r) => [r.label, r.values])).toEqual([
      ["Name", ["Hades II", "Hades II", "HADES 2"]],
      ["Completion status", ["Played", "Beaten", null]],
      ["Your score", [null, "85", "70"]],
      ["Favorite", ["No", "Yes", "No"]],
      ["Categories", ["RPG", null, null]],
    ]);
  });

  it("labels metadata fields", () => {
    expect(fieldLabel("ageRatings")).toBe("Age ratings");
    expect(fieldLabel("description")).toBe("Description");
  });
});
