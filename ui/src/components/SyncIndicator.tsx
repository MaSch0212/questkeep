import { createSignal, For, onCleanup, Show } from "solid-js";
import RefreshCw from "lucide-solid/icons/refresh-cw";
import * as api from "../lib/api";
import { errorMessage } from "../lib/errors";
import { formatClockTime } from "../lib/format";
import { store } from "../lib/store";
import { sourceInfo } from "../lib/sources";
import { syncEntryText, syncLabel } from "../lib/syncStatus";
import { SourceIcon } from "./SourceIcon";

async function syncAll() {
  try {
    await api.scanAllLibraries();
  } catch (error) {
    store.pushToast("error", errorMessage(error));
  }
}

/**
 * Top bar button showing library sync: spins while any store is being scanned; the popover lists
 * the stores of the current or last run and starts a full sync.
 */
export function SyncIndicator() {
  const [open, setOpen] = createSignal(false);
  const [wrapper, setWrapper] = createSignal<HTMLDivElement>();
  const [button, setButton] = createSignal<HTMLButtonElement>();

  function close(returnFocus: boolean) {
    setOpen(false);
    if (returnFocus) button()?.focus();
  }

  function onDocumentPointerDown(event: PointerEvent) {
    const root = wrapper();
    if (open() && root && event.target instanceof Node && !root.contains(event.target)) {
      close(false);
    }
  }
  document.addEventListener("pointerdown", onDocumentPointerDown);
  onCleanup(() => document.removeEventListener("pointerdown", onDocumentPointerDown));

  return (
    <div
      ref={setWrapper}
      class="sync"
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
        ref={setButton}
        type="button"
        class="icon-btn icon-btn--bordered"
        classList={{ "icon-btn--active": open() }}
        aria-label={syncLabel(store.syncState())}
        title={syncLabel(store.syncState())}
        aria-haspopup="dialog"
        aria-expanded={open()}
        onClick={() => setOpen((value) => !value)}
      >
        <RefreshCw class="icon" classList={{ "icon--spin": store.syncBusy() }} aria-hidden="true" />
      </button>
      <Show when={open()}>
        <div class="sync__popover" role="dialog" aria-label="Library sync">
          <h2 class="sync__title">Library sync</h2>
          <Show
            when={store.syncState().entries.length > 0}
            fallback={<p class="muted">No sync has run yet.</p>}
          >
            <ul class="sync__list">
              <For each={store.syncState().entries}>
                {(entry) => (
                  <li
                    class="sync__item"
                    classList={{ "sync__item--failed": entry.status === "failed" }}
                  >
                    <SourceIcon source={entry.provider} />
                    <span class="sync__store">
                      {sourceInfo(entry.provider, store.providers()).name}
                    </span>
                    <span class="sync__status" title={syncEntryText(entry)}>
                      {syncEntryText(entry)}
                    </span>
                  </li>
                )}
              </For>
            </ul>
          </Show>
          <Show when={store.syncState().finishedAt}>
            {(time) => (
              <p class="muted sync__finished">Last sync finished at {formatClockTime(time())}</p>
            )}
          </Show>
          <button
            type="button"
            class="btn btn--primary"
            disabled={store.syncState().active}
            onClick={() => void syncAll()}
          >
            Sync all
          </button>
        </div>
      </Show>
    </div>
  );
}
