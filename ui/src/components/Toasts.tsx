import { For } from "solid-js";
import { store } from "../lib/store";

export function Toasts() {
  return (
    <div class="toasts">
      <For each={store.toasts()}>
        {(toast) => (
          <div class="toast" classList={{ "toast--error": toast.kind === "error" }} role="status">
            <span class="toast__message">{toast.message}</span>
            <button
              class="icon-btn"
              type="button"
              aria-label="Dismiss notification"
              onClick={() => store.dismissToast(toast.id)}
            >
              ×
            </button>
          </div>
        )}
      </For>
    </div>
  );
}
