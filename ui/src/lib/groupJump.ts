import { createEffect, on } from "solid-js";
import { groupStart, type GroupedLayout } from "./grouping";
import { store } from "./store";

/**
 * Follows "Jump to group" requests in a grid or list: scrolls the group's header to the top,
 * selects its first game and focuses the list.
 */
export function followGroupJumps(options: {
  layout: () => GroupedLayout;
  scroller: () => HTMLElement | undefined;
  setScrollTop: (top: number) => void;
  moveTo: (slot: number, select: (id: string) => void) => void;
}) {
  createEffect(
    on(
      store.groupJump,
      (jump) => {
        const scroller = options.scroller();
        if (!jump || !scroller) return;
        const start = groupStart(options.layout(), jump.key);
        if (!start) return;
        if (start.slot >= 0) options.moveTo(start.slot, store.selectGame);
        scroller.scrollTop = start.top;
        options.setScrollTop(start.top);
        scroller.focus({ preventScroll: true });
      },
      { defer: true },
    ),
  );
}
