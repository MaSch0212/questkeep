import { createSignal, onCleanup, onMount, type JSX } from "solid-js";
import { Portal } from "solid-js/web";

/** Where a popover opens: under an element (aligned to its start or end), beside it, or at a
 *  point (context menus). */
export type PopoverAnchor =
  | { rect: DOMRect; align: "start" | "end" | "center" }
  | { rect: DOMRect; align: "beside" }
  | { x: number; y: number };

const GAP = 6; // px; matches --qk-popover-gap, read for the maths only
const MARGIN = 8; // keeps popovers off the window edge

function place(anchor: PopoverAnchor, width: number, height: number) {
  let left: number;
  let top: number;
  if ("x" in anchor) {
    left = anchor.x;
    top = anchor.y;
  } else if (anchor.align === "beside") {
    left = anchor.rect.right + 2;
    top = anchor.rect.top - GAP;
    if (left + width > window.innerWidth - MARGIN) left = anchor.rect.left - width - 2;
  } else {
    top = anchor.rect.bottom + GAP;
    left =
      anchor.align === "start"
        ? anchor.rect.left
        : anchor.align === "end"
          ? anchor.rect.right - width
          : anchor.rect.left + (anchor.rect.width - width) / 2;
  }
  left = Math.max(MARGIN, Math.min(left, window.innerWidth - width - MARGIN));
  if (top + height > window.innerHeight - MARGIN) {
    top = Math.max(MARGIN, window.innerHeight - height - MARGIN);
  }
  return { left, top };
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
 * window. Closes on Escape and on a pointer press outside it.
 */
export function Popover(props: PopoverProps) {
  const [position, setPosition] = createSignal({ left: -9999, top: -9999 });
  let panel: HTMLDivElement | undefined;

  onMount(() => {
    if (!panel) return;
    const rect = panel.getBoundingClientRect();
    setPosition(place(props.anchor, rect.width, rect.height));
  });

  function onPointerDown(event: PointerEvent) {
    const target = event.target;
    if (!(target instanceof Node) || panel?.contains(target)) return;
    // A submenu or picker opened from this popover lives in its own portal.
    if (target instanceof Element && target.closest(".popover")) return;
    if (props.inside?.().some((el) => el?.contains(target))) return;
    props.onClose(false);
  }
  document.addEventListener("pointerdown", onPointerDown, true);
  onCleanup(() => document.removeEventListener("pointerdown", onPointerDown, true));

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
