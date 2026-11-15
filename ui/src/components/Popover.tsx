import {
  autoUpdate,
  computePosition,
  flip,
  offset,
  type Placement,
  shift,
  size,
  type VirtualElement,
} from "@floating-ui/dom";
import { createSignal, onCleanup, onMount, type JSX } from "solid-js";
import { Portal } from "solid-js/web";

/** Where a popover opens: under an element (aligned to its start or end), right below it like a
 *  combobox list (flipped above when there is no room), beside it, or at a point (context
 *  menus). */
export type PopoverAnchor =
  | { rect: DOMRect; align: "start" | "end" | "center" | "below" }
  | { rect: DOMRect; align: "beside" }
  | { x: number; y: number };

const GAP = 6; // px; matches --qk-popover-gap
const MARGIN = 8; // keeps popovers off the window edge
const LIST_GAP = 2; // a combobox list sits right at its input

/** Open popovers, oldest first: a popover opened later from inside another one is its child. */
const openPanels: HTMLElement[] = [];

/** How floating-ui places a popover for an anchor. */
function placement(anchor: PopoverAnchor): {
  reference: VirtualElement;
  placement: Placement;
  offset: { mainAxis: number; crossAxis: number };
} {
  if ("x" in anchor) {
    const point = new DOMRect(anchor.x, anchor.y, 0, 0);
    return {
      reference: { getBoundingClientRect: () => point },
      placement: "bottom-start",
      offset: { mainAxis: 0, crossAxis: 0 },
    };
  }
  const reference = { getBoundingClientRect: () => anchor.rect };
  switch (anchor.align) {
    case "beside":
      return { reference, placement: "right-start", offset: { mainAxis: 2, crossAxis: -GAP } };
    case "below":
      return { reference, placement: "bottom-start", offset: { mainAxis: LIST_GAP, crossAxis: 0 } };
    case "start":
      return { reference, placement: "bottom-start", offset: { mainAxis: GAP, crossAxis: 0 } };
    case "end":
      return { reference, placement: "bottom-end", offset: { mainAxis: GAP, crossAxis: 0 } };
    default:
      return { reference, placement: "bottom", offset: { mainAxis: GAP, crossAxis: 0 } };
  }
}

export interface PopoverProps {
  anchor: PopoverAnchor;
  /** `returnFocus` is false when the user clicked elsewhere (focus goes where they clicked). */
  onClose: (returnFocus: boolean) => void;
  class?: string;
  role?: JSX.HTMLAttributes<HTMLDivElement>["role"];
  label?: string;
  /** Elements that count as "inside" (the button that opened it, a parent menu). */
  inside?: () => (Element | undefined)[];
  ref?: (el: HTMLDivElement) => void;
  onKeyDown?: (event: KeyboardEvent) => void;
  children: JSX.Element;
}

/**
 * A floating panel in a portal (so the sidebar or title bar doesn't clip it), kept inside the
 * window by floating-ui: it flips to the other side of its anchor or shifts along it when there is
 * no room, also when its content grows after it opened. Closes on Escape and on a pointer press outside it.
 */
export function Popover(props: PopoverProps) {
  const [position, setPosition] = createSignal({ left: -9999, top: -9999 });
  let panel: HTMLDivElement | undefined;

  onMount(() => {
    if (!panel) return;
    openPanels.push(panel);
    const element = panel;
    const how = placement(props.anchor);
    // Placed again whenever the panel changes size (a list filling in after it opened), so it
    // flips or shifts to stay inside the window.
    const stop = autoUpdate(how.reference, element, () => {
      void computePosition(how.reference, element, {
        placement: how.placement,
        strategy: "fixed",
        middleware: [
          offset(how.offset),
          flip({ padding: MARGIN }),
          shift({ padding: MARGIN, crossAxis: true }),
          size({
            padding: MARGIN,
            apply({ availableHeight }) {
              element.style.setProperty("--qk-popover-room", `${Math.max(0, availableHeight)}px`);
            },
          }),
        ],
      }).then(({ x, y }) => setPosition({ left: x, top: y }));
    });
    onCleanup(stop);
  });

  function onPointerDown(event: PointerEvent) {
    const target = event.target;
    if (!(target instanceof Node) || panel?.contains(target)) return;
    // A submenu or picker opened from this popover lives in its own portal and counts as
    // inside; a popover this one was opened from does not (a click there closes this one).
    const index = panel ? openPanels.indexOf(panel) : -1;
    if (openPanels.slice(index + 1).some((p) => p.contains(target))) return;
    if (props.inside?.().some((el) => el?.contains(target))) return;
    props.onClose(false);
  }
  document.addEventListener("pointerdown", onPointerDown, true);
  onCleanup(() => {
    document.removeEventListener("pointerdown", onPointerDown, true);
    const index = panel ? openPanels.indexOf(panel) : -1;
    if (index >= 0) openPanels.splice(index, 1);
  });

  return (
    <Portal>
      <div
        ref={(el) => {
          panel = el;
          props.ref?.(el);
        }}
        class={`popover ${props.class ?? ""}`}
        role={props.role}
        aria-label={props.label}
        style={{ left: `${position().left}px`, top: `${position().top}px` }}
        onKeyDown={(event) => {
          props.onKeyDown?.(event);
          if (event.defaultPrevented) return;
          if (event.key === "Escape") {
            event.preventDefault();
            event.stopPropagation();
            props.onClose(true);
          }
        }}
      >
        {props.children}
      </div>
    </Portal>
  );
}
