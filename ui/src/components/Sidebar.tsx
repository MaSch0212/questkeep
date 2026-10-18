import Settings from "lucide-solid/icons/settings";
import { For } from "solid-js";
import { libraryView, type LibraryView } from "../lib/query";
import { store } from "../lib/store";
import { Logo } from "./Logo";

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

export function Sidebar(props: SidebarProps) {
  return (
    <nav class="sidebar" aria-label="Library navigation">
      <div class="sidebar__brand">
        <Logo class="logo" />
        <span class="sidebar__wordmark">Questkeep</span>
      </div>
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

      <h2 class="sidebar__heading">Sources</h2>
      <ul class="sidebar__list">
        <For each={store.sources()}>
          {(source) => (
            <li>
              <button
                type="button"
                class="nav-item"
                classList={{ "nav-item--active": store.query().sources.includes(source.id) }}
                aria-pressed={store.query().sources.includes(source.id)}
                onClick={() => store.toggleSourceFilter(source.id)}
              >
                {source.name}
              </button>
            </li>
          )}
        </For>
      </ul>

      <div class="sidebar__footer">
        <button type="button" class="btn btn--primary" onClick={props.onAdd}>
          Add game
        </button>
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
