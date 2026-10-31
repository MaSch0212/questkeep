import type { Game } from "../bindings/Game";
import { updateGame } from "./actions";
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
    void updateGame(game.id, { hidden: !game.hidden }).then((failure) => {
      if (failure) store.pushToast("error", failure);
      else store.pushToast("info", `${game.hidden ? "Unhid" : "Hid"} “${game.name}”`);
    });
    return true;
  }
  return false;
}
