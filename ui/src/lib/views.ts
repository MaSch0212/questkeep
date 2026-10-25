import type { Game } from "../bindings/Game";
import type { GameQuery } from "../bindings/GameQuery";
import type { GroupBy } from "../bindings/GroupBy";
import type { LibraryView } from "../bindings/LibraryView";
import type { SectionOrder } from "../bindings/SectionOrder";
import type { ViewLayout } from "../bindings/ViewLayout";
import type { ViewSection } from "../bindings/ViewSection";
import { storesOf } from "./copies";

/** What a view shows right now: its saved state or the unsaved changes made to it. */
export interface ViewState {
  query: GameQuery;
  layout: ViewLayout;
  group: GroupBy;
}

/** `GameQuery.categories` entry for games without a category (the core's `NO_CATEGORY`). */
export const NO_CATEGORY = ":none";

/** Unsaved changes per view id, kept until the app closes. */
export type Drafts = Readonly<Record<string, ViewState>>;

/** The saved state of a view, without the keyword (which a saved query never has). */
export function savedState(view: LibraryView): ViewState {
  return { query: { ...view.query, search: null }, layout: view.layout, group: view.group };
}

/** The draft of a view if it has one, else its saved state. */
export function effectiveState(view: LibraryView, drafts: Drafts): ViewState {
  return drafts[view.id] ?? savedState(view);
}

function sameSet(a: readonly string[], b: readonly string[]): boolean {
  return a.length === b.length && a.every((s) => b.includes(s));
}

function sameQuery(a: GameQuery, b: GameQuery): boolean {
  return (
    a.sort === b.sort &&
    a.descending === b.descending &&
    a.favorite === b.favorite &&
    a.installed === b.installed &&
    a.hidden === b.hidden &&
    a.played === b.played &&
    sameSet(a.sources, b.sources) &&
    sameSet(a.categories, b.categories)
  );
}

/** Whether `state` differs from what the view has saved (the keyword never counts). */
export function differsFromSaved(view: LibraryView, state: ViewState): boolean {
  return (
    state.layout !== view.layout ||
    state.group !== view.group ||
    !sameQuery(state.query, view.query)
  );
}

/**
 * Applies a change to a view's current state and returns the new drafts. A change that brings
 * the view back to its saved state drops the draft, so the "unsaved" marker goes away.
 */
export function withChange(
  drafts: Drafts,
  view: LibraryView,
  change: (state: ViewState) => ViewState,
): Drafts {
  const next = change(effectiveState(view, drafts));
  const rest = Object.fromEntries(Object.entries(drafts).filter(([id]) => id !== view.id));
  return differsFromSaved(view, next) ? { ...rest, [view.id]: next } : rest;
}

export function withoutDraft(drafts: Drafts, id: string): Drafts {
  return Object.fromEntries(Object.entries(drafts).filter(([key]) => key !== id));
}

/** How many filters (not sort or layout) a query sets: shown on the Filters button. */
export function activeFilterCount(query: GameQuery): number {
  return (
    (query.sources.length > 0 ? 1 : 0) +
    (query.categories.length > 0 ? 1 : 0) +
    (query.favorite !== "any" ? 1 : 0) +
    (query.installed !== "any" ? 1 : 0) +
    (query.played !== "any" ? 1 : 0) +
    (query.hidden !== "visible" ? 1 : 0)
  );
}

/** The filters of a query, with sort and layout kept. */
export function clearedFilters(query: GameQuery): GameQuery {
  return {
    ...query,
    sources: [],
    categories: [],
    favorite: "any",
    installed: "any",
    played: "any",
    hidden: "visible",
  };
}

function hasPlayed(game: Game): boolean {
  return game.playtimeSeconds > 0 || game.playCount > 0;
}

/** Whether a game passes a query's filters (the keyword is ignored). Mirrors the core's SQL. */
export function matchesFilters(query: GameQuery, game: Game): boolean {
  if (query.sources.length > 0 && !storesOf(game).some((s) => query.sources.includes(s))) {
    return false;
  }
  if (query.favorite !== "any" && game.favorite !== (query.favorite === "yes")) return false;
  if (query.installed !== "any" && game.installed !== (query.installed === "yes")) return false;
  if (query.hidden === "visible" && game.hidden) return false;
  if (query.hidden === "hidden" && !game.hidden) return false;
  if (query.played !== "any" && hasPlayed(game) !== (query.played === "played")) return false;
  if (
    query.categories.length > 0 &&
    !query.categories.some((id) =>
      id === NO_CATEGORY ? game.categories.length === 0 : game.categories.includes(id),
    )
  )
    return false;
  return true;
}

export function allViews(sections: readonly ViewSection[]): LibraryView[] {
  return sections.flatMap((s) => s.views);
}

export function findView(sections: readonly ViewSection[], id: string | null): LibraryView | null {
  return allViews(sections).find((v) => v.id === id) ?? null;
}

/**
 * The view to show a game in: the active one if it contains the game, else the first view (in
 * sidebar order) that does, else `null`.
 */
export function viewForGame(
  sections: readonly ViewSection[],
  drafts: Drafts,
  activeId: string | null,
  game: Game,
): LibraryView | null {
  const contains = (view: LibraryView) => matchesFilters(effectiveState(view, drafts).query, game);
  const active = findView(sections, activeId);
  if (active && contains(active)) return active;
  return allViews(sections).find(contains) ?? null;
}

/** The order the core stores, from the sections as shown. */
export function sectionOrder(sections: readonly ViewSection[]): SectionOrder[] {
  return sections.map((s) => ({ id: s.id, views: s.views.map((v) => v.id) }));
}

/** Where a dragged item lands: a section index, or a view slot inside a section. */
export type DropTarget =
  | { kind: "section"; index: number }
  | { kind: "view"; sectionId: string; index: number };

/** Moves a view to `index` in `sectionId` (index counted without the moved view). */
export function moveView(
  sections: readonly ViewSection[],
  viewId: string,
  sectionId: string,
  index: number,
): ViewSection[] {
  const view = findView(sections, viewId);
  if (!view) return [...sections];
  const result: ViewSection[] = [];
  for (const section of sections) {
    const views = section.views.filter((v) => v.id !== viewId);
    if (section.id === sectionId) {
      views.splice(Math.max(0, Math.min(index, views.length)), 0, view);
    }
    result.push({ id: section.id, name: section.name, views });
  }
  return result;
}

/** Moves a section to `index` (counted without the moved section). */
export function moveSection(
  sections: readonly ViewSection[],
  sectionId: string,
  index: number,
): ViewSection[] {
  const section = sections.find((s) => s.id === sectionId);
  if (!section) return [...sections];
  const rest = sections.filter((s) => s.id !== sectionId);
  rest.splice(Math.max(0, Math.min(index, rest.length)), 0, section);
  return rest;
}

/** Applies a drop. `target` indexes count the dragged item where it currently is. */
export function applyDrop(
  sections: readonly ViewSection[],
  dragged: { kind: "section" | "view"; id: string },
  target: DropTarget,
): ViewSection[] {
  if (dragged.kind === "section" && target.kind === "section") {
    const from = sections.findIndex((s) => s.id === dragged.id);
    return moveSection(sections, dragged.id, target.index > from ? target.index - 1 : target.index);
  }
  if (dragged.kind === "view" && target.kind === "view") {
    const from = sections.find((s) => s.views.some((v) => v.id === dragged.id));
    const fromIndex = from?.views.findIndex((v) => v.id === dragged.id) ?? -1;
    const index =
      from?.id === target.sectionId && target.index > fromIndex ? target.index - 1 : target.index;
    return moveView(sections, dragged.id, target.sectionId, index);
  }
  return [...sections];
}

/** One row of the sidebar in edit mode, with its vertical middle (for drop targeting). */
export interface EditRow {
  kind: "section" | "view";
  sectionId: string;
  /** Index of the view in its section (views) or of the section (sections). */
  index: number;
  middle: number;
}

/**
 * Where a dragged item would land when the pointer is at `y`, given the rows in display order.
 * Sections drop between sections; views drop before the first row below the pointer.
 */
export function dropTargetAt(
  rows: readonly EditRow[],
  sectionIds: readonly string[],
  sectionSizes: Readonly<Record<string, number>>,
  dragging: "section" | "view",
  y: number,
): DropTarget | null {
  if (sectionIds.length === 0) return null;
  const below = rows.find((row) => y < row.middle);
  if (dragging === "section") {
    const next = rows.find((row) => row.kind === "section" && y < row.middle);
    return { kind: "section", index: next ? next.index : sectionIds.length };
  }
  const endOf = (sectionId: string): DropTarget => ({
    kind: "view",
    sectionId,
    index: sectionSizes[sectionId] ?? 0,
  });
  if (!below) return endOf(sectionIds[sectionIds.length - 1] ?? "");
  if (below.kind === "view")
    return { kind: "view", sectionId: below.sectionId, index: below.index };
  // Above a section heading: the end of the section before it (or the top of the first one).
  if (below.index === 0) return { kind: "view", sectionId: below.sectionId, index: 0 };
  return endOf(sectionIds[below.index - 1] ?? below.sectionId);
}

/**
 * Keyboard reordering (Alt+Up/Down) of a view: one step within its section, or across the
 * section edge into the neighbouring section.
 */
export function nudgeView(
  sections: readonly ViewSection[],
  viewId: string,
  direction: -1 | 1,
): ViewSection[] {
  const si = sections.findIndex((s) => s.views.some((v) => v.id === viewId));
  const section = sections[si];
  if (!section) return [...sections];
  const target = section.views.findIndex((v) => v.id === viewId) + direction;
  if (target >= 0 && target < section.views.length) {
    return moveView(sections, viewId, section.id, target);
  }
  const neighbour = sections[si + direction];
  if (!neighbour) return [...sections];
  return moveView(sections, viewId, neighbour.id, direction < 0 ? neighbour.views.length : 0);
}

export function nudgeSection(
  sections: readonly ViewSection[],
  sectionId: string,
  direction: -1 | 1,
): ViewSection[] {
  const index = sections.findIndex((s) => s.id === sectionId);
  return index < 0 ? [...sections] : moveSection(sections, sectionId, index + direction);
}
