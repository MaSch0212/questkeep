import X from "lucide-solid/icons/x";
import { For, Show } from "solid-js";
import { store } from "../lib/store";

export function Toasts() {
  return (
    <div class="toasts">
      <For each={store.toasts()}>
        {(toast) => (
          <div class="toast" classList={{ "toast--error": toast.kind === "error" }} role="status">
            <span class="toast__message">{toast.message}</span>
            <Show when={toast.action}>
              {(action) => (
                <button
                  type="button"
                  class="btn btn--small toast__action"
                  onClick={() => {
                    store.dismissToast(toast.id);
                    action().run();
                  }}
                >
                  {action().label}
                </button>
              )}
            </Show>
            <button
              class="icon-btn"
              type="button"
              aria-label="Dismiss notification"
              onClick={() => store.dismissToast(toast.id)}
            >
              <X class="icon" aria-hidden="true" />
            </button>
          </div>
        )}
      </For>
    </div>
  );
}
