import { describe, expect, it } from "vitest";
import type { GameEditInfo } from "../bindings/GameEditInfo";
import type { GameMetadata } from "../bindings/GameMetadata";
import type { MetadataField } from "../bindings/MetadataField";
import {
  buildEdits,
  editedCount,
  gigabytesText,
  hoursText,
  moveItem,
  parseGigabytes,
  parseHours,
  parseScore,
  SECTION_FIELDS,
  shortUrl,
  sourceLabel,
  sourceName,
  startValues,
  webQuery,
} from "./editGame";

function meta(over: Partial<GameMetadata> = {}): GameMetadata {
  return {
    description: null,
    releaseDate: null,
    developers: [],
    publishers: [],
    genres: [],
    tags: [],
    features: [],
    series: [],
    ageRatings: [],
    platforms: [],
    gameModes: [],
    themes: [],
    perspectives: [],
    criticScore: null,
    communityScore: null,
    installSize: null,
    timeToBeat: null,
    links: [],
    ...over,
  };
}

const names = (id: string) => ({ steam: "Steam" })[id] ?? id;

describe("edit game helpers", () => {
  it("label sources", () => {
    expect(sourceLabel("igdb", names)).toBe("from IGDB");
    expect(sourceLabel("steam", names)).toBe("from Steam");
    expect(sourceLabel("measured", names)).toBe("measured");
    expect(sourceLabel(undefined, names)).toBe("no value");
    expect(sourceName("hltb", names)).toBe("HowLongToBeat");
    expect(sourceName(undefined, names)).toBe("automatic");
  });

  it("start from the user's values for edited fields only", () => {
    const info: GameEditInfo = {
      edits: {
        meta: meta({ description: "Mine", tags: [], genres: ["Ignored"] }),
        edited: ["description", "tags"],
        hiddenLinks: [],
      },
      automatic: meta({ description: "Auto", tags: ["Roguelike"], genres: ["Action"] }),
      automaticSources: { description: "igdb" },
      links: [],
      automaticSortingName: "Hades",
      sortingName: null,
      userImages: { cover: false, background: false, icon: false },
    };
    const values = startValues(info);
    expect(values.description).toBe("Mine");
    expect(values.tags).toEqual([]);
    expect(values.genres).toEqual(["Action"]);
    values.genres.push("x");
    expect(info.automatic.genres).toEqual(["Action"]);
  });

  it("build the edits to save and count them per section", () => {
    const edited = new Set<MetadataField>(["tags", "description"]);
    const edits = buildEdits(
      meta({ tags: ["A"] }),
      edited,
      [{ name: " Wiki ", url: " https://x.example " }],
      ["https://hidden"],
    );
    expect(edits.edited).toEqual(["description", "tags"]);
    expect(edits.meta.links).toEqual([{ name: "Wiki", url: "https://x.example" }]);
    expect(edits.hiddenLinks).toEqual(["https://hidden"]);
    expect(editedCount(SECTION_FIELDS.details, edited)).toBe(1);
    expect(editedCount(SECTION_FIELDS.general, edited)).toBe(1);
  });

  it("parse numbers as typed", () => {
    expect(parseHours("28")).toBe(28 * 3600);
    expect(parseHours("1,5h")).toBe(5400);
    expect(parseHours("")).toBeNull();
    expect(parseHours("soon")).toBeUndefined();
    expect(hoursText(5400)).toBe("1.5");
    expect(hoursText(null)).toBe("");
    expect(parseGigabytes("9.4 GB")).toBe(Math.round(9.4 * 1024 ** 3));
    expect(gigabytesText(Math.round(9.4 * 1024 ** 3))).toBe("9.4");
    expect(parseScore("85")).toBe(85);
    expect(parseScore("101")).toBeUndefined();
    expect(parseScore(" ")).toBeNull();
  });

  it("shorten links, prefill web searches and move items", () => {
    expect(shortUrl("https://www.pcgamingwiki.com/wiki/Hades/")).toBe(
      "pcgamingwiki.com/wiki/Hades",
    );
    expect(webQuery("Hades II", "background")).toBe("Hades II wallpaper");
    expect(moveItem(["a", "b", "c"], 2, -1)).toEqual(["a", "c", "b"]);
    expect(moveItem(["a", "b"], 0, -1)).toEqual(["a", "b"]);
  });
});
