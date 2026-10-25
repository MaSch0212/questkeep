import RefreshCw from "lucide-solid/icons/refresh-cw";
import Settings from "lucide-solid/icons/settings";
import { For, Show } from "solid-js";
import * as api from "../lib/api";
import { libraryView, type LibraryView } from "../lib/query";
import { store } from "../lib/store";
import { isProviderSyncing } from "../lib/syncStatus";
import { AddGameButton } from "./AddGameButton";
import { SourceIcon } from "./SourceIcon";
import { SyncIndicator } from "./SyncIndicator";

const VIEWS: { id: LibraryView; label: string }[] = [
  { id: "all", label: "All games" },
  { id: "favorites", label: "Favorites" },
  { id: "hidden", label: "Hidden" },
];

interface SidebarProps {
  onAdd: () => void;
}

const inLibrary = () => store.page() === "library";

const view = () => libraryView(store.query());

/** Progress, the result and failures arrive as scan events and end up in the sync summary. */
async function syncSource(id: string) {
  try {
    await api.scanLibrary(id);
  } catch {
    // Already reported through the scanFailed event.
  }
}

export function Sidebar(props: SidebarProps) {
  return (
    <nav class="sidebar" aria-label="Library navigation">
      <h1 class="sidebar__title">Library</h1>

      <ul class="sidebar__list">
        <For each={VIEWS}>
          {(item) => (
            <li>
              <button
                type="button"
                class="nav-item"
                classList={{ "nav-item--active": inLibrary() && view() === item.id }}
                aria-pressed={inLibrary() && view() === item.id}
                onClick={() => store.setView(item.id)}
              >
                {item.label}
              </button>
            </li>
          )}
        </For>
      </ul>

      <label class="check">
        <input
          type="checkbox"
          checked={store.query().installedOnly}
          onChange={(event) => store.setInstalledOnly(event.currentTarget.checked)}
        />
        <span>Installed only</span>
      </label>

      <div class="sidebar__section">
        <h2 class="sidebar__heading">Sources</h2>
        <SyncIndicator />
      </div>
      <ul class="sidebar__list">
        <For each={store.sources()}>
          {(source) => {
            const syncing = () => isProviderSyncing(store.syncState(), source.id);
            return (
              <li class="source-item" classList={{ "source-item--syncing": syncing() }}>
                <button
                  type="button"
                  class="nav-item nav-item--icon"
                  classList={{ "nav-item--active": store.query().sources.includes(source.id) }}
                  aria-pressed={store.query().sources.includes(source.id)}
                  onClick={() => store.toggleSourceFilter(source.id)}
                >
                  <SourceIcon source={source.id} />
                  <span class="nav-item__text">{source.name}</span>
                </button>
                <Show when={source.canScan}>
                  <button
                    type="button"
                    class="icon-btn icon-btn--small source-item__sync"
                    aria-label={syncing() ? `Syncing ${source.name}…` : `Sync ${source.name}`}
                    title={syncing() ? `Syncing ${source.name}…` : `Sync ${source.name}`}
                    disabled={syncing()}
                    onClick={() => void syncSource(source.id)}
                  >
                    <RefreshCw
                      class="icon"
                      classList={{ "icon--spin": syncing() }}
                      aria-hidden="true"
                    />
                  </button>
                </Show>
              </li>
            );
          }}
        </For>
      </ul>

      <div class="sidebar__footer">
        <AddGameButton onAdd={props.onAdd} />
        <button
          type="button"
          class="nav-item nav-item--icon"
          classList={{ "nav-item--active": store.page() === "settings" }}
          aria-current={store.page() === "settings" ? "page" : undefined}
          onClick={() => store.showSettings()}
        >
          <Settings class="icon" aria-hidden="true" />
          <span>Settings</span>
        </button>
      </div>
    </nav>
  );
}
