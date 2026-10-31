import { describe, expect, it } from "vitest";
import { makeGame } from "../test-utils";
import {
  COLUMNS,
  columnTemplate,
  moveColumn,
  pickerColumns,
  resizeColumn,
  sameColumns,
  shownColumns,
  toggleColumn,
} from "./columns";

const ids = (columns: { id: string }[]) => columns.map((c) => c.id);

describe("columns", () => {
  it("shows the defaults for an empty list and skips unknown or repeated ids", () => {
    expect(shownColumns([]).map((c) => c.def.id)).toEqual([
      "source",
      "state",
      "playtime",
      "lastPlayed",
      "releaseYear",
    ]);
    const stored = [
      { id: "gone", width: null },
      { id: "genres", width: 200 },
      { id: "genres", width: null },
    ];
    expect(shownColumns(stored).map((c) => [c.def.id, c.width])).toEqual([["genres", 200]]);
  });

  it("toggles, moves and resizes, storing the defaults as an empty list", () => {
    const added = toggleColumn([], "criticScore");
    expect(ids(added)).toEqual([
      "source",
      "state",
      "playtime",
      "lastPlayed",
      "releaseYear",
      "criticScore",
    ]);
    expect(toggleColumn(added, "criticScore")).toEqual([]);
    expect(ids(moveColumn([], "releaseYear", "source"))).toEqual([
      "releaseYear",
      "source",
      "state",
      "playtime",
      "lastPlayed",
    ]);
    const wide = resizeColumn([], "playtime", 40.6, 60);
    expect(wide.find((c) => c.id === "playtime")?.width).toBe(60);
    expect(sameColumns(wide, [])).toBe(false);
    expect(sameColumns([], toggleColumn(toggleColumn([], "tags"), "tags"))).toBe(true);
  });

  it("builds the grid template from default sizes and set widths", () => {
    const shown = shownColumns([
      { id: "genres", width: null },
      { id: "playtime", width: 123 },
    ]);
    expect(columnTemplate(shown)).toBe(
      "var(--qk-list-thumb-width) minmax(var(--qk-column-name-min), 1fr) " +
        "minmax(var(--qk-column-min), var(--qk-column-lg)) 123px",
    );
  });

  it("lists shown columns first in order, then the rest by label", () => {
    const entries = pickerColumns([
      { id: "tags", width: null },
      { id: "source", width: null },
    ]);
    expect(entries.slice(0, 3).map((e) => [e.def.id, e.shown])).toEqual([
      ["tags", true],
      ["source", true],
      ["ageRatings", false],
    ]);
    expect(entries).toHaveLength(COLUMNS.length);
  });

  it("formats values, lists and missing ones", () => {
    const game = makeGame({ genres: ["RPG", "Action"], criticScore: null, completionStatus: "b" });
    const names = { status: () => "Beaten", category: () => undefined };
    const text = (id: string) => COLUMNS.find((c) => c.id === id)?.text?.(game, names);
    expect(text("genres")).toBe("RPG, Action");
    expect(text("criticScore")).toBe("–");
    expect(text("completionStatus")).toBe("Beaten");
  });
});
