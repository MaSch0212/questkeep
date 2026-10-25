import { createMemo, createSignal } from "solid-js";
import type { GroupedLayout } from "./grouping";
import { store } from "./store";

/**
 * Which slot of a grouped layout the keyboard cursor is on. The store selects games, but a game
 * can show in several groups: the slot the user moved to or clicked wins while it still shows the
 * selected game, else the selection's first slot.
 */
export function createSlotCursor(layout: () => GroupedLayout) {
  const [chosen, setChosen] = createSignal(-1);
  const cursor = createMemo(() => {
    const id = store.selectedId();
    if (id === null) return -1;
    const slots = layout().slots;
    if (slots[chosen()]?.game.id === id) return chosen();
    return slots.findIndex((slot) => slot.game.id === id);
  });
  /** Moves the cursor to a slot and selects its game. */
  function moveTo(slot: number, select: (id: string) => void) {
    const target = layout().slots[slot];
    if (!target) return;
    setChosen(slot);
    select(target.game.id);
  }
  return { cursor, moveTo };
}
