import RefreshCw from "lucide-solid/icons/refresh-cw";
import TriangleAlert from "lucide-solid/icons/triangle-alert";
import X from "lucide-solid/icons/x";
import { For, onMount, Show } from "solid-js";
import * as api from "../lib/api";
import { errorMessage } from "../lib/errors";
import { store } from "../lib/store";
import { syncRowText } from "../lib/syncStatus";
import { Popover, type PopoverAnchor } from "./Popover";
import { SourceIcon } from "./SourceIcon";

export async function syncAll() {
  try {
    await api.scanAllLibraries();
  } catch (error) {
    store.pushToast("error", errorMessage(error));
  }
}

/** Progress, the result and failures arrive as scan events and end up in the sync status. */
export async function syncStore(id: string) {
  try {
    await api.scanLibrary(id);
  } catch {
    // Already reported through the scanFailed event.
  }
}

const stores = () => store.providers().filter((p) => p.syncable);
const entry = (id: string) => store.syncState().entries.find((e) => e.provider === id);

/** Every store that can sync, always listed with its status and its own sync button. */
export function SyncPanel(props: {
  anchor: PopoverAnchor;
  onClose: (returnFocus: boolean) => void;
  inside: () => (Element | undefined)[];
}) {
  let allButton: HTMLButtonElement | undefined;
  onMount(() => {
    queueMicrotask(() => allButton?.focus());
    void store.loadAccountWarnings();
  });

  return (
    <Popover
      anchor={props.anchor}
      class="sync-panel"
      role="dialog"
      label="Library sync"
      inside={props.inside}
      onClose={props.onClose}
    >
      <div class="sync-panel__head">
        <span class="sync-panel__last">
          {syncRowText(undefined, store.syncTimes().all, "Last full sync")}
        </span>
        <button
          ref={(el) => (allButton = el)}
          type="button"
          class="btn btn--primary btn--small"
          disabled={store.syncState().active}
          onClick={() => void syncAll()}
        >
          <RefreshCw class="icon" aria-hidden="true" /> Sync all
        </button>
      </div>
      <Show when={store.accountWarnings().length > 0}>
        <ul class="sync-panel__warnings" aria-label="Accounts signed out">
          <For each={store.accountWarnings()}>
            {(warning) => (
              <li class="sync-panel__warning">
                <TriangleAlert class="icon sync-panel__warning-icon" aria-hidden="true" />
                <span class="sync-panel__warning-text">
                  {warning.name}
                  {warning.displayName ? ` (${warning.displayName})` : ""} is signed out, so its
                  owned games and achievements don't update.
                </span>
                <button
                  type="button"
                  class="btn btn--small"
                  onClick={() => {
                    props.onClose(false);
                    store.showSettingsSection("stores");
                  }}
                >
                  Sign in…
                </button>
                <button
                  type="button"
                  class="icon-btn icon-btn--small"
                  aria-label={`Dismiss the warning about ${warning.name}`}
                  title="Dismiss (until you sign in again)"
                  onClick={() => void store.dismissAccountWarning(warning.provider)}
                >
                  <X class="icon" aria-hidden="true" />
                </button>
              </li>
            )}
          </For>
        </ul>
      </Show>
      <ul class="sync-panel__list">
        <For each={stores()}>
          {(provider) => {
            const busy = () => {
              const status = entry(provider.id)?.status;
              return status === "running" || status === "waiting";
            };
            const text = () =>
              syncRowText(entry(provider.id), store.syncTimes().providers[provider.id] ?? null);
            return (
              <li
                class="sync-panel__row"
                classList={{ "sync-panel__row--failed": entry(provider.id)?.status === "failed" }}
              >
                <SourceIcon source={provider.id} />
                <span class="sync-panel__name">{provider.name}</span>
                <span class="sync-panel__status" title={text()}>
                  {text()}
                </span>
                <button
                  type="button"
                  class="icon-btn icon-btn--small"
                  aria-label={busy() ? `Syncing ${provider.name}…` : `Sync ${provider.name}`}
                  title={busy() ? `Syncing ${provider.name}…` : `Sync ${provider.name}`}
                  disabled={busy()}
                  onClick={() => void syncStore(provider.id)}
                >
                  <RefreshCw class="icon" classList={{ "icon--spin": busy() }} aria-hidden="true" />
                </button>
              </li>
            );
          }}
        </For>
      </ul>
      <Show when={store.backgroundWorkLabel()}>
        {(label) => (
          <p class="sync-panel__work" role="status">
            <RefreshCw class="icon icon--spin" aria-hidden="true" /> {label()}
          </p>
        )}
      </Show>
    </Popover>
  );
}
