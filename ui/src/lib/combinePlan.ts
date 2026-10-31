import type { CombinePlan } from "../bindings/CombinePlan";
import type { CompletionStatus } from "../bindings/CompletionStatus";
import type { Game } from "../bindings/Game";
import type { GameEditInfo } from "../bindings/GameEditInfo";
import type { LaunchProfiles } from "../bindings/LaunchProfiles";

/** One game on the combine screen with what the screen shows about it. */
export interface CombineSource {
  game: Game;
  edits: GameEditInfo | null;
  profiles: LaunchProfiles | null;
}

type Single =
  | "nameFrom"
  | "imagesFrom"
  | "detailsFrom"
  | "statusFrom"
  | "scoreFrom"
  | "profileFrom";
type Multi = "categoriesFrom" | "notesFrom" | "linksFrom" | "profilesFrom";

/** A row of the combine screen: one value per game (null: it has none). */
export type CombineRow =
  | {
      kind: "single";
      key: Single;
      label: string;
      section: "game" | "you";
      values: (string | null)[];
    }
  | { kind: "favorite"; label: string; section: "you"; values: (string | null)[] }
  | { kind: "multi"; key: Multi; label: string; section: "you"; values: (string | null)[] };

/** "ageRatings" -> "Age ratings". */
export function fieldLabel(field: string): string {
  const words = field.replace(/([A-Z])/g, " $1").toLowerCase();
  return words.charAt(0).toUpperCase() + words.slice(1);
}

/** A list as one value, or null when empty. */
const list = (items: readonly string[]) => (items.length === 0 ? null : items.join(", "));

const ownProfiles = (s: CombineSource) => s.profiles?.profiles.filter((p) => p.own) ?? [];

/**
 * The suggested choices: the most-played game is the base (name, images, edited details,
 * default profile); the most advanced completion status (latest in the status list); the score
 * of the most-played game that has one; favorite if any is; everything that adds up is kept.
 */
export function suggestedPlan(
  sources: readonly CombineSource[],
  statuses: readonly CompletionStatus[],
): CombinePlan {
  const ids = sources.map((s) => s.game.id);
  const byPlay = sources.toSorted((a, b) => b.game.playtimeSeconds - a.game.playtimeSeconds);
  const base = byPlay[0]?.game.id ?? ids[0] ?? "";
  const rank = (s: CombineSource) =>
    statuses.findIndex((status) => status.id === s.game.completionStatus);
  // Ties go to the most-played game.
  const advanced = byPlay.reduce<CombineSource | undefined>(
    (best, s) => (rank(s) >= 0 && (best === undefined || rank(s) > rank(best)) ? s : best),
    undefined,
  );
  const scored = byPlay.find((s) => s.game.userScore !== null);
  return {
    entries: ids,
    base,
    nameFrom: base,
    imagesFrom: base,
    detailsFrom: base,
    statusFrom: advanced?.game.id ?? base,
    scoreFrom: scored?.game.id ?? base,
    favorite: sources.some((s) => s.game.favorite),
    profileFrom: base,
    categoriesFrom: ids,
    notesFrom: ids,
    linksFrom: ids,
    profilesFrom: ids,
  };
}

/** The rows that differ between the games; rows where all agree or none has a value are left out. */
export function combineRows(
  sources: readonly CombineSource[],
  names: {
    status: (id: string | null) => string | null;
    category: (id: string) => string;
    source: (id: string) => string;
  },
): CombineRow[] {
  const values = (pick: (s: CombineSource) => string | null) => sources.map(pick);
  const rows: CombineRow[] = [
    {
      kind: "single",
      key: "nameFrom",
      label: "Name",
      section: "game",
      values: values((s) => s.game.name),
    },
    {
      kind: "single",
      key: "imagesFrom",
      label: "Cover and images",
      section: "game",
      values: values((s) =>
        s.game.coverPath === null
          ? null
          : s.edits?.userImages.cover
            ? `Your image (${names.source(s.game.source)})`
            : `From ${names.source(s.game.source)}`,
      ),
    },
    {
      kind: "single",
      key: "detailsFrom",
      label: "Edited details",
      section: "game",
      values: values((s) => list(s.edits?.edits.edited.map(fieldLabel) ?? [])),
    },
    {
      kind: "single",
      key: "statusFrom",
      label: "Completion status",
      section: "you",
      values: values((s) => names.status(s.game.completionStatus)),
    },
    {
      kind: "single",
      key: "scoreFrom",
      label: "Your score",
      section: "you",
      values: values((s) => (s.game.userScore === null ? null : String(s.game.userScore))),
    },
    {
      kind: "favorite",
      label: "Favorite",
      section: "you",
      values: values((s) => (s.game.favorite ? "Yes" : "No")),
    },
    {
      kind: "multi",
      key: "categoriesFrom",
      label: "Categories",
      section: "you",
      values: values((s) => list(s.game.categories.map(names.category))),
    },
    {
      kind: "multi",
      key: "notesFrom",
      label: "Notes",
      section: "you",
      values: values((s) => s.game.notes?.trim() || null),
    },
    {
      kind: "multi",
      key: "linksFrom",
      label: "Your links",
      section: "you",
      values: values((s) => list(s.edits?.edits.meta.links.map((l) => l.name) ?? [])),
    },
    {
      kind: "multi",
      key: "profilesFrom",
      label: "Launch profiles",
      section: "you",
      values: values((s) => list(ownProfiles(s).map((p) => p.name))),
    },
    {
      kind: "single",
      key: "profileFrom",
      label: "Default profile",
      section: "you",
      values: values((s) => {
        const chosen = s.profiles?.profiles.find((p) => p.id === s.profiles?.defaultId);
        return chosen ? chosen.name : null;
      }),
    },
  ];
  return rows.filter((row) => {
    const present = row.values.filter((v) => v !== null);
    if (present.length === 0) return false;
    // Images always differ per game; show the row when more than one has some.
    if (row.kind === "single" && row.key === "imagesFrom") return present.length > 1;
    return !(present.length === row.values.length && present.every((v) => v === present[0]));
  });
}
