import { createSignal, Show } from "solid-js";

/** Arrow keys move the splitter this far; with Shift four times as far. */
const KEY_STEP = 16;

/**
 * A vertical bar between two panes that resizes one of them: drag it, or focus it and use the
 * arrow keys (Home/End for the smallest and largest size). `value` is the resized pane's width;
 * `grows` says which arrow direction makes it wider. The width shows while dragging.
 */
export function Splitter(props: {
  label: string;
  value: number;
  min: number;
  max: number;
  /** "left": dragging left widens the pane (it sits to the right of the bar). */
  grows: "left" | "right";
  onChange: (width: number) => void;
}) {
  const [dragging, setDragging] = createSignal(false);
  const clamp = (width: number) => Math.round(Math.min(props.max, Math.max(props.min, width)));
  const sign = () => (props.grows === "left" ? -1 : 1);

  function onPointerDown(event: PointerEvent) {
    if (event.button !== 0) return;
    event.preventDefault();
    const bar = event.currentTarget as HTMLElement;
    bar.setPointerCapture(event.pointerId);
    bar.focus({ preventScroll: true });
    const startX = event.clientX;
    const start = props.value;
    setDragging(true);
    const move = (e: PointerEvent) => props.onChange(clamp(start + (e.clientX - startX) * sign()));
    const end = () => {
      setDragging(false);
      bar.removeEventListener("pointermove", move);
      bar.removeEventListener("pointerup", end);
      bar.removeEventListener("pointercancel", end);
    };
    bar.addEventListener("pointermove", move);
    bar.addEventListener("pointerup", end);
    bar.addEventListener("pointercancel", end);
  }

  function onKeyDown(event: KeyboardEvent) {
    const step = event.shiftKey ? KEY_STEP * 4 : KEY_STEP;
    let next: number | null = null;
    if (event.key === "ArrowLeft") next = props.value - step * sign();
    else if (event.key === "ArrowRight") next = props.value + step * sign();
    else if (event.key === "Home") next = props.min;
    else if (event.key === "End") next = props.max;
    if (next === null) return;
    event.preventDefault();
    props.onChange(clamp(next));
  }

  return (
    <div
      class="splitter"
      classList={{ "splitter--dragging": dragging() }}
      role="separator"
      aria-orientation="vertical"
      aria-label={props.label}
      aria-valuenow={props.value}
      aria-valuemin={props.min}
      aria-valuemax={props.max}
      tabindex="0"
      onPointerDown={onPointerDown}
      onKeyDown={onKeyDown}
    >
      <Show when={dragging()}>
        <span class="splitter__value">{props.value} px</span>
      </Show>
    </div>
  );
}
