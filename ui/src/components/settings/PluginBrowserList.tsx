import Search from "lucide-solid/icons/search";
import { createMemo, createSignal, For, Show } from "solid-js";
import type { RegistryPluginView } from "../../bindings/RegistryPluginView";
import { translator } from "../../i18n";
import { pluginBrowser } from "../../i18n/pluginBrowser";
import { settingsPlugins } from "../../i18n/settingsPlugins";
import {
  filterRegistry,
  type RegistryFilter,
  registryKinds,
  sortRegistry,
} from "../../lib/pluginRegistry";
import type { PluginKind } from "../../lib/plugins";
import { PluginIcon } from "./PluginIcon";

const tr = translator(pluginBrowser);
const trPlugins = translator(settingsPlugins);

const KIND_LABEL = {
  store: "kindStore",
  extension: "kindExtension",
  theme: "kindTheme",
} as const;
const KINDS: readonly PluginKind[] = ["store", "extension", "theme"];

/** The search box, the filter chips and the plugin cards. Arrow keys move between the cards. */
export function PluginBrowserList(props: {
  plugins: readonly RegistryPluginView[];
  filter: RegistryFilter;
  onFilter: (filter: RegistryFilter) => void;
  onOpen: (id: string) => void;
}) {
  const shown = createMemo(() => sortRegistry(filterRegistry(props.plugins, props.filter)));
  const [list, setList] = createSignal<HTMLUListElement>();
  const [search, setSearch] = createSignal<HTMLInputElement>();

  const cards = () => Array.from(list()?.querySelectorAll<HTMLElement>("[data-plugin-card]") ?? []);

  function toggleKind(kind: PluginKind) {
    const kinds = new Set(props.filter.kinds);
    if (!kinds.delete(kind)) kinds.add(kind);
    props.onFilter({ ...props.filter, kinds });
  }

  function onListKeyDown(event: KeyboardEvent) {
    const items = cards();
    const index = items.indexOf(document.activeElement as HTMLElement);
    let target: HTMLElement | undefined;
    if (event.key === "ArrowDown") target = items[index + 1];
    else if (event.key === "ArrowUp") target = index <= 0 ? search() : items[index - 1];
    else if (event.key === "Home") target = items[0];
    else if (event.key === "End") target = items.at(-1);
    else return;
    event.preventDefault();
    target?.focus();
  }

  return (
    <>
      <div class="plugin-browser__toolbar">
        <div class="plugin-browser__search">
          <Search class="icon" aria-hidden="true" />
          <input
            ref={setSearch}
            class="input"
            type="search"
            data-plugin-search
            data-autofocus
            aria-label={tr("search")}
            placeholder={tr("searchHint")}
            value={props.filter.query}
            onInput={(event) =>
              props.onFilter({ ...props.filter, query: event.currentTarget.value })
            }
            onKeyDown={(event) => {
              if (event.key === "ArrowDown") {
                event.preventDefault();
                cards()[0]?.focus();
              } else if (event.key === "Escape" && props.filter.query !== "") {
                // Clears the search first; the next Esc closes the dialog.
                event.preventDefault();
                event.stopPropagation();
                props.onFilter({ ...props.filter, query: "" });
              }
            }}
          />
        </div>
        <div class="plugin-browser__chips" role="group" aria-label={tr("kindFilter")}>
          <For each={KINDS}>
            {(kind) => (
              <button
                type="button"
                class="plugin-browser__chip"
                aria-pressed={props.filter.kinds.has(kind)}
                onClick={() => toggleKind(kind)}
              >
                {trPlugins(KIND_LABEL[kind])}
              </button>
            )}
          </For>
          <button
            type="button"
            class="plugin-browser__chip"
            aria-pressed={props.filter.installed}
            onClick={() => props.onFilter({ ...props.filter, installed: !props.filter.installed })}
          >
            {tr("filterInstalled")}
          </button>
          <button
            type="button"
            class="plugin-browser__chip"
            aria-pressed={props.filter.updates}
            onClick={() => props.onFilter({ ...props.filter, updates: !props.filter.updates })}
          >
            {tr("filterUpdates")}
          </button>
        </div>
      </div>
      <Show
        when={props.plugins.length > 0}
        fallback={<p class="muted plugin-browser__empty">{tr("emptyRegistry")}</p>}
      >
        <Show
          when={shown().length > 0}
          fallback={
            <p class="muted plugin-browser__empty" role="status">
              {tr("noMatch")}
            </p>
          }
        >
          <ul
            ref={setList}
            class="plugin-browser__list"
            aria-label={tr("list")}
            onKeyDown={onListKeyDown}
          >
            <For each={shown()}>
              {(plugin) => (
                <li>
                  <button
                    type="button"
                    class="plugin-card"
                    classList={{ "plugin-card--blocked": plugin.latest === null }}
                    data-plugin-card={plugin.id}
                    onClick={() => props.onOpen(plugin.id)}
                  >
                    <PluginIcon src={plugin.iconData} />
                    <span class="plugin-card__text">
                      <span class="plugin-card__title">
                        <span class="account__name">{plugin.name}</span>
                        <span class="muted">{trPlugins("by", { author: plugin.author })}</span>
                      </span>
                      <span class="plugin-card__summary muted">{plugin.summary}</span>
                      <Show when={plugin.latest === null}>
                        <span class="plugin-card__note">
                          {tr("notCompatible", { reason: plugin.incompatibleReason ?? "" })}
                        </span>
                      </Show>
                    </span>
                    <span class="plugin-card__badges">
                      <For each={registryKinds(plugin)}>
                        {(kind) => (
                          <span class="account__badge">{trPlugins(KIND_LABEL[kind])}</span>
                        )}
                      </For>
                      <Show when={plugin.installed}>
                        {(installed) => (
                          <span class="account__badge account__badge--on">
                            {tr("installedVersion", { version: installed().version })}
                          </span>
                        )}
                      </Show>
                      <Show when={plugin.installed?.pinned}>
                        <span class="account__badge">{tr("pinned")}</span>
                      </Show>
                      <Show when={plugin.updateAvailable}>
                        <span class="account__badge plugin-card__update">
                          {tr("updateAvailable")}
                        </span>
                      </Show>
                    </span>
                  </button>
                </li>
              )}
            </For>
          </ul>
        </Show>
      </Show>
    </>
  );
}
