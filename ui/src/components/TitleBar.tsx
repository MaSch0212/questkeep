import Copy from "lucide-solid/icons/copy";
import Minus from "lucide-solid/icons/minus";
import Square from "lucide-solid/icons/square";
import X from "lucide-solid/icons/x";
import { createSignal, onCleanup, onMount, Show } from "solid-js";
import * as api from "../lib/api";
import { Logo } from "./Logo";

/**
 * Replaces the native window frame (`decorations: false`). Dragging the bar moves the window and
 * double-clicking it toggles maximize (`data-tauri-drag-region`). The buttons are left out of the
 * tab order like native caption buttons; Win+Up/Down and Alt+F4 remain the keyboard paths.
 */
export function TitleBar() {
  const [maximized, setMaximized] = createSignal(false);
  const [focused, setFocused] = createSignal(true);

  const syncMaximized = () =>
    void Promise.resolve(api.isWindowMaximized())
      .then((value) => setMaximized(value === true))
      .catch(() => {});

  onMount(() => {
    syncMaximized();

    const unlisteners = [
      api.onWindowResized(syncMaximized),
      api.onWindowFocusChanged((value) => setFocused(value)),
    ];
    onCleanup(() => {
      for (const unlisten of unlisteners) {
        void Promise.resolve(unlisten)
          .then((fn) => fn?.())
          .catch(() => {});
      }
    });
  });

  return (
    <header
      class="titlebar"
      classList={{ "titlebar--inactive": !focused() }}
      data-tauri-drag-region
    >
      <div class="titlebar__brand" data-tauri-drag-region>
        <Logo class="logo titlebar__logo" />
        <span class="titlebar__wordmark">Questkeep</span>
      </div>
      <div class="titlebar__controls">
        <button
          type="button"
          class="titlebar__button"
          tabIndex={-1}
          aria-label="Minimize"
          title="Minimize"
          onClick={() => void api.minimizeWindow()}
        >
          <Minus class="titlebar__icon" aria-hidden="true" />
        </button>
        <button
          type="button"
          class="titlebar__button"
          tabIndex={-1}
          aria-label={maximized() ? "Restore" : "Maximize"}
          title={maximized() ? "Restore" : "Maximize"}
          onClick={() => void api.toggleMaximizeWindow()}
        >
          <Show
            when={maximized()}
            fallback={<Square class="titlebar__icon titlebar__icon--small" aria-hidden="true" />}
          >
            <Copy
              class="titlebar__icon titlebar__icon--small titlebar__icon--mirrored"
              aria-hidden="true"
            />
          </Show>
        </button>
        <button
          type="button"
          class="titlebar__button titlebar__button--close"
          tabIndex={-1}
          aria-label="Close"
          title="Close"
          onClick={() => void api.closeWindow()}
        >
          <X class="titlebar__icon" aria-hidden="true" />
        </button>
      </div>
    </header>
  );
}
