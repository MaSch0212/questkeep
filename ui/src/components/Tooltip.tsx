import { autoUpdate, computePosition, flip, offset, shift } from "@floating-ui/dom";
import { createEffect, createSignal, onCleanup, onMount, Show } from "solid-js";
import { readCssPx } from "../lib/layoutMode";
import { tooltipDelay } from "../lib/tooltip";

interface Tip {
  target: HTMLElement;
  text: string;
}

const TOOLTIP_ID = "qk-tooltip";

const targetOf = (node: EventTarget | null): HTMLElement | null =>
  node instanceof Element ? node.closest<HTMLElement>("[data-tooltip]") : null;

/** The tooltip to show; none while the element's own popup is open (it would cover it). */
const textOf = (element: HTMLElement) =>
  element.getAttribute("aria-expanded") === "true" ? "" : (element.dataset.tooltip?.trim() ?? "");

/**
 * The app's tooltips: any element with a `data-tooltip` attribute shows its text in a styled
 * bubble centered below it (above, or shifted, where there is no room), after a short rest or at
 * once while another tooltip was just showing. Keyboard focus shows it too. Mounted once; the
 * attribute replaces the native `title`, whose tooltip is slow and unstyled.
 */
export function TooltipLayer() {
  const [tip, setTip] = createSignal<Tip | null>(null);
  const [bubble, setBubble] = createSignal<HTMLDivElement>();
  let timer: ReturnType<typeof setTimeout> | undefined;
  let pending: HTMLElement | null = null;
  let lastHidden: number | null = null;

  function show(target: HTMLElement) {
    pending = null;
    const text = textOf(target);
    if (!target.isConnected || text === "") {
      hide();
      return;
    }
    setTip({ target, text });
  }

  function schedule(target: HTMLElement) {
    if (tip()?.target === target || pending === target) return;
    clearTimeout(timer);
    pending = target;
    const delay = tooltipDelay(performance.now(), lastHidden, tip() !== null);
    if (delay === 0) show(target);
    else timer = setTimeout(() => show(target), delay);
  }

  function hide() {
    clearTimeout(timer);
    pending = null;
    if (tip() === null) return;
    lastHidden = performance.now();
    setTip(null);
  }

  // Positions the bubble while it shows and follows the target (scrolling, resizing, its text).
  createEffect(() => {
    const current = tip();
    const element = bubble();
    if (!current || !element) return;
    const { target } = current;
    const root = document.documentElement;
    const gap = readCssPx(root, "--qk-tooltip-gap", 6);
    const padding = readCssPx(root, "--qk-tooltip-edge", 8);
    element.dataset.placed = "false";
    const place = () => {
      if (!target.isConnected) {
        hide();
        return;
      }
      void computePosition(target, element, {
        placement: "bottom",
        strategy: "fixed",
        middleware: [offset(gap), flip({ padding }), shift({ padding })],
      }).then(({ x, y }) => {
        element.style.left = `${x}px`;
        element.style.top = `${y}px`;
        element.dataset.placed = "true";
      });
    };
    const stop = autoUpdate(target, element, place);
    const observer = new MutationObserver(() => {
      const text = textOf(target);
      if (text === "") hide();
      else if (text !== current.text) setTip({ target, text });
    });
    observer.observe(target, {
      attributes: true,
      attributeFilter: ["data-tooltip", "aria-expanded"],
    });
    // Screen readers hear the tooltip as the target's description.
    const describedBy = target.getAttribute("aria-describedby");
    if (describedBy === null) target.setAttribute("aria-describedby", TOOLTIP_ID);
    onCleanup(() => {
      stop();
      observer.disconnect();
      if (describedBy === null && target.getAttribute("aria-describedby") === TOOLTIP_ID) {
        target.removeAttribute("aria-describedby");
      }
    });
  });

  onMount(() => {
    const onPointerOver = (event: PointerEvent) => {
      const target = targetOf(event.target);
      if (target) schedule(target);
      else hide();
    };
    const onPointerOut = (event: PointerEvent) => {
      // Leaving the window.
      if (event.relatedTarget === null) hide();
    };
    // oxlint-disable-next-line unicorn/consistent-function-scoping -- calls `schedule` above
    const onFocusIn = (event: FocusEvent) => {
      const target = targetOf(event.target);
      const focused = event.target;
      // Text fields count as focus-visible even when clicked; their tooltip would cover the
      // suggestions and what is typed.
      if (
        target &&
        focused instanceof Element &&
        focused.matches(":focus-visible") &&
        !focused.matches("input, textarea, select, [contenteditable]")
      ) {
        schedule(target);
      }
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && tip()) hide();
    };
    document.addEventListener("pointerover", onPointerOver, true);
    document.addEventListener("pointerout", onPointerOut, true);
    document.addEventListener("pointerdown", hide, true);
    document.addEventListener("wheel", hide, { capture: true, passive: true });
    document.addEventListener("focusin", onFocusIn, true);
    document.addEventListener("focusout", hide, true);
    document.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("blur", hide);
    onCleanup(() => {
      clearTimeout(timer);
      document.removeEventListener("pointerover", onPointerOver, true);
      document.removeEventListener("pointerout", onPointerOut, true);
      document.removeEventListener("pointerdown", hide, true);
      document.removeEventListener("wheel", hide, true);
      document.removeEventListener("focusin", onFocusIn, true);
      document.removeEventListener("focusout", hide, true);
      document.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("blur", hide);
    });
  });

  return (
    <Show when={tip()}>
      {(current) => (
        <div ref={setBubble} id={TOOLTIP_ID} class="tooltip" role="tooltip">
          {current().text}
        </div>
      )}
    </Show>
  );
}
