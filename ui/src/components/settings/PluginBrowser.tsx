import { createMemo, createSignal, Match, onMount, Show, Switch } from "solid-js";
import type { RegistryListing } from "../../bindings/RegistryListing";
import { translator } from "../../i18n";
import { pluginBrowser } from "../../i18n/pluginBrowser";
import * as api from "../../lib/api";
import { errorMessage } from "../../lib/errors";
import { formatDateTime } from "../../lib/format";
import { NO_FILTER, type RegistryFilter } from "../../lib/pluginRegistry";
import { store } from "../../lib/store";
import { Modal } from "../Modal";
import { createRegistryInstall } from "../RegistryInstall";
import { PluginBrowserDetail } from "./PluginBrowserDetail";
import { PluginBrowserList } from "./PluginBrowserList";

const tr = translator(pluginBrowser);

type State =
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "ready"; listing: RegistryListing };

/**
 * The plugin registry: search and filter its plugins, open one for its versions, install or
 * update. Esc leaves a plugin's page first, then closes the browser.
 */
export function PluginBrowser(props: { onClose: () => void }) {
  const [state, setState] = createSignal<State>({ status: "loading" });
  const [selected, setSelected] = createSignal<string | null>(null);
  const [filter, setFilter] = createSignal<RegistryFilter>(NO_FILTER);
  const [root, setRoot] = createSignal<HTMLDivElement>();

  /** Loads the list; `quiet` keeps what is shown until the new list is there (after an install). */
  async function load(quiet: boolean) {
    if (!quiet) setState({ status: "loading" });
    try {
      const listing = await api.fetchPluginRegistry();
      setState({ status: "ready", listing });
      void store.loadPluginUpdates().catch(() => undefined);
      if (!quiet) {
        queueMicrotask(() => root()?.querySelector<HTMLElement>("[data-plugin-search]")?.focus());
      }
    } catch (error) {
      if (!quiet || state().status !== "ready") {
        setState({ status: "error", message: errorMessage(error) });
      }
    }
  }
  onMount(() => void load(false));

  const install = createRegistryInstall(() => void load(true));

  const listing = () => {
    const current = state();
    return current.status === "ready" ? current.listing : null;
  };
  const plugin = createMemo(() => {
    const id = selected();
    return id === null ? null : (listing()?.plugins.find((p) => p.id === id) ?? null);
  });

  function open(id: string) {
    setSelected(id);
  }

  function back() {
    const id = selected();
    setSelected(null);
    // The list stays mounted while a plugin is open: focus returns to its card.
    queueMicrotask(() =>
      root()
        ?.querySelector<HTMLElement>(`[data-plugin-card="${CSS.escape(id ?? "")}"]`)
        ?.focus(),
    );
  }

  return (
    <Modal title={tr("title")} wide tall onClose={props.onClose}>
      <div
        ref={setRoot}
        class="plugin-browser"
        onKeyDown={(event) => {
          if (event.key === "Escape" && plugin() !== null) {
            event.preventDefault();
            event.stopPropagation();
            back();
          }
        }}
      >
        <Switch>
          <Match when={state().status === "loading"}>
            <p class="muted plugin-browser__empty" role="status">
              {tr("loading")}
            </p>
          </Match>
          <Match when={state().status === "error"}>
            <div class="plugin-browser__empty">
              <p class="plugin__error selectable" role="alert">
                {tr("loadFailed", { error: (state() as { message: string }).message })}
              </p>
              <button type="button" class="btn" data-autofocus onClick={() => void load(false)}>
                {tr("retry")}
              </button>
            </div>
          </Match>
          <Match when={listing()}>
            {(ready) => (
              <>
                <Show when={ready().offline}>
                  <p class="plugin-browser__notice" role="status">
                    {tr("offline", { date: formatDateTime(ready().fetchedAt) })}
                  </p>
                </Show>
                <div class="plugin-browser__page" hidden={plugin() !== null}>
                  <PluginBrowserList
                    plugins={ready().plugins}
                    filter={filter()}
                    onFilter={setFilter}
                    onOpen={open}
                  />
                </div>
                <Show when={plugin()}>
                  {(current) => (
                    <div class="plugin-browser__page">
                      <PluginBrowserDetail
                        plugin={current()}
                        preparing={install.preparing()}
                        busy={install.busy()}
                        onBack={back}
                        onInstall={(version) => void install.begin(current().id, version)}
                      />
                    </div>
                  )}
                </Show>
              </>
            )}
          </Match>
        </Switch>
      </div>
      {install.dialog()}
    </Modal>
  );
}
