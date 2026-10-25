import ChevronUp from "lucide-solid/icons/chevron-up";
import { createSignal, For, onCleanup, Show } from "solid-js";
import * as api from "../lib/api";
import { addGameMenuItems, nextMenuIndex, type AddGameMenuItem } from "../lib/addGameOptions";
import { errorMessage } from "../lib/errors";
import { store } from "../lib/store";
import { SourceIcon } from "./SourceIcon";

const items = () => addGameMenuItems(store.providers());

async function runOption(item: AddGameMenuItem) {
  try {
    await api.addGameWithOption(item.providerId, item.optionId);
    store.pushToast(
      "info",
      `Opened ${item.providerName}. The new game appears here once it is set up there.`,
    );
  } catch (error) {
    store.pushToast("error", errorMessage(error));
  }
}

/**
 * The sidebar's "Add game" button. When providers offer their own ways to add games (e.g. a new
 * Prism Launcher instance), an arrow next to it opens a menu with them. The menu opens upwards
 * (the button sits at the bottom of the sidebar) and is fixed-positioned so the sidebar does not
 * clip it.
 */
export function AddGameButton(props: { onAdd: () => void }) {
  const [open, setOpen] = createSignal(false);
  const [wrapper, setWrapper] = createSignal<HTMLDivElement>();
  const [arrow, setArrow] = createSignal<HTMLButtonElement>();
  const [anchor, setAnchor] = createSignal({ bottom: 0, left: 0, width: 0 });
  const menuItems: HTMLButtonElement[] = [];

  function focusItem(index: number) {
    menuItems[index]?.focus();
  }

  function show(focusIndex: number) {
    const rect = wrapper()?.getBoundingClientRect();
    if (rect) {
      setAnchor({ bottom: window.innerHeight - rect.top, left: rect.left, width: rect.width });
    }
    setOpen(true);
    queueMicrotask(() => focusItem(focusIndex));
  }

  function close(returnFocus: boolean) {
    setOpen(false);
    if (returnFocus) arrow()?.focus();
  }

  function onDocumentPointerDown(event: PointerEvent) {
    const root = wrapper();
    if (open() && root && event.target instanceof Node && !root.contains(event.target)) {
      close(false);
    }
  }
  document.addEventListener("pointerdown", onDocumentPointerDown);
  onCleanup(() => document.removeEventListener("pointerdown", onDocumentPointerDown));

  function choose(item: AddGameMenuItem) {
    close(true);
    void runOption(item);
  }

  return (
    <div
      ref={setWrapper}
      class="split-btn"
      onKeyDown={(event) => {
        if (event.key === "Escape" && open()) {
          event.preventDefault();
          event.stopPropagation();
          close(true);
        }
      }}
      onFocusOut={(event) => {
        const next = event.relatedTarget;
        if (open() && next instanceof Node && !wrapper()?.contains(next)) close(false);
      }}
    >
      <button
        type="button"
        class="btn btn--primary split-btn__main"
        classList={{ "split-btn__main--alone": items().length === 0 }}
        onClick={props.onAdd}
      >
        Add game
      </button>
      <Show when={items().length > 0}>
        <button
          ref={setArrow}
          type="button"
          class="btn btn--primary split-btn__arrow"
          aria-label="More ways to add games"
          title="More ways to add games"
          aria-haspopup="menu"
          aria-expanded={open()}
          onClick={() => (open() ? close(false) : show(0))}
          onKeyDown={(event) => {
            if (event.key === "ArrowUp" || event.key === "ArrowDown") {
              event.preventDefault();
              show(event.key === "ArrowUp" ? items().length - 1 : 0);
            }
          }}
        >
          <ChevronUp class="icon" aria-hidden="true" />
        </button>
        <Show when={open()}>
          <div
            class="split-btn__menu"
            role="menu"
            aria-label="Add game from"
            style={{
              bottom: `calc(${anchor().bottom}px + var(--qk-popover-gap))`,
              left: `${anchor().left}px`,
              "min-width": `${anchor().width}px`,
            }}
            onKeyDown={(event) => {
              const current = menuItems.findIndex((el) => el === document.activeElement);
              const next = nextMenuIndex(event.key, current, items().length);
              if (next !== null) {
                event.preventDefault();
                focusItem(next);
              } else if (event.key === "Tab") {
                close(false);
              }
            }}
          >
            <For each={items()}>
              {(item, index) => (
                <button
                  ref={(el) => (menuItems[index()] = el)}
                  type="button"
                  role="menuitem"
                  class="split-btn__item"
                  tabIndex={-1}
                  onClick={() => choose(item)}
                >
                  <SourceIcon source={item.providerId} />
                  <span>{item.label}</span>
                </button>
              )}
            </For>
          </div>
        </Show>
      </Show>
    </div>
  );
}
