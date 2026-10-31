import type { Game } from "../bindings/Game";
import { updateGame } from "./actions";
import { changeAll } from "./bulkActions";
import type { GroupedLayout } from "./grouping";
import { store } from "./store";

/**
 * Keys a game list or grid shares for its selected game: the Menu key and Shift+F10 open the
 * context menu under the game's element, Del hides (or unhides) the game. Returns whether the
 * key was handled.
 */
export function handleGameKeys(
  event: KeyboardEvent,
  game: Game | undefined,
  element: Element | null,
): boolean {
  if (!game) return false;
  if (event.key === "ContextMenu" || (event.key === "F10" && event.shiftKey)) {
    event.preventDefault();
    const rect = element?.getBoundingClientRect();
    store.openContextMenu(
      game.id,
      rect ? { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 } : { x: 0, y: 0 },
    );
    return true;
  }
  if (event.key === "Delete" && !event.ctrlKey && !event.altKey && !event.metaKey) {
    event.preventDefault();
    const all = store.selectedGames();
    if (all.length > 1) {
      const hide = !all.every((g) => g.hidden);
      void changeAll(all, { hidden: hide, addCategories: [], removeCategories: [] });
      return true;
    }
    void updateGame(game.id, { hidden: !game.hidden }).then((failure) => {
      if (failure) store.pushToast("error", failure);
      else store.pushToast("info", `${game.hidden ? "Unhid" : "Hid"} “${game.name}”`);
    });
    return true;
  }
  return false;
}

type MoveTo = (slot: number, select: (id: string) => void) => void;

/** The game ids of a layout in visual order (Shift ranges follow it). */
const orderOf = (layout: GroupedLayout) => layout.slots.map((slot) => slot.game.id);

/** A click on a game: Ctrl toggles it in the selection, Shift selects a range, else `plain`. */
export function clickSelect(
  event: MouseEvent,
  slot: number,
  layout: GroupedLayout,
  moveTo: MoveTo,
  plain: (id: string) => void,
) {
  if (event.ctrlKey || event.metaKey) moveTo(slot, store.toggleSelect);
  else if (event.shiftKey) moveTo(slot, (id) => store.selectRange(orderOf(layout), id));
  else moveTo(slot, plain);
}

/** Right-click on a game: a game of the multi-selection keeps it, any other selects only itself. */
export function contextSelect(slot: number, layout: GroupedLayout, moveTo: MoveTo) {
  const game = layout.slots[slot]?.game;
  if (!game) return;
  if (!(store.isMulti() && store.isSelected(game.id))) moveTo(slot, store.selectGame);
}

/**
 * Selection keys of a game list or grid: Shift with a navigation key extends the selection to
 * `target` (the slot the key moves to), Ctrl+A selects every game, Esc drops a multi-selection.
 * Returns whether the key was handled.
 */
export function handleSelectionKeys(
  event: KeyboardEvent,
  layout: GroupedLayout,
  moveTo: MoveTo,
  target: () => number,
  isNav: boolean,
): boolean {
  if (isNav && event.shiftKey && !event.altKey && !event.ctrlKey && !event.metaKey) {
    event.preventDefault();
    moveTo(target(), (id) => store.selectRange(orderOf(layout), id));
    return true;
  }
  if ((event.ctrlKey || event.metaKey) && !event.altKey && event.key.toLowerCase() === "a") {
    event.preventDefault();
    store.selectAll([...new Set(orderOf(layout))]);
    return true;
  }
  if (event.key === "Escape" && store.isMulti()) {
    event.preventDefault();
    store.clearSelection();
    return true;
  }
  return false;
}
