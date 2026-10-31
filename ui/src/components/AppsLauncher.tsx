import ExternalLink from "lucide-solid/icons/external-link";
import Search from "lucide-solid/icons/search";
import { createMemo, createResource, createSignal, For, onMount, Show } from "solid-js";
import type { Game } from "../bindings/Game";
import { activateGame } from "../lib/actions";
import * as api from "../lib/api";
import { defaultQuery } from "../lib/query";
import { store } from "../lib/store";
import { GameIcon } from "./GameIcon";
import { Popover, type PopoverAnchor } from "./Popover";

/** At most two rows of four. */
const SHOWN = 8;

/** Installed, visible apps, most used first. */
const loadApps = () =>
  api.listGames({ ...defaultQuery(), kind: "apps", sort: "playtime", descending: true });

// The launcher stays open behind the menu (the menu counts as inside it, see Popover).
function menu(app: Game, event: MouseEvent) {
  event.preventDefault();
  store.openContextMenu(app.id, { x: event.clientX, y: event.clientY }, app);
}

/**
 * The title bar's apps launcher: find box and app icons (most used first). Click starts an app,
 * right-click opens its menu, "Show all apps" opens Explore filtered to apps.
 */
export function AppsLauncher(props: {
  anchor: PopoverAnchor;
  onClose: (returnFocus: boolean) => void;
  inside: () => (Element | undefined)[];
}) {
  const [apps] = createResource(loadApps);
  const [text, setText] = createSignal("");
  let input: HTMLInputElement | undefined;

  const shown = createMemo(() => {
    const needle = text().trim().toLocaleLowerCase();
    const list = apps() ?? [];
    const matching =
      needle === "" ? list : list.filter((a) => a.name.toLocaleLowerCase().includes(needle));
    return matching.slice(0, SHOWN);
  });

  onMount(() => queueMicrotask(() => input?.focus()));

  function start(app: Game) {
    props.onClose(false);
    void activateGame(app.id);
  }

  return (
    <Popover
      anchor={props.anchor}
      class="apps"
      role="dialog"
      label="Apps"
      inside={props.inside}
      onClose={props.onClose}
    >
      <label class="picker__search">
        <Search class="icon" aria-hidden="true" />
        <input
          ref={(el) => (input = el)}
          type="text"
          value={text()}
          placeholder="Find an app"
          aria-label="Find an app"
          onInput={(event) => setText(event.currentTarget.value)}
          onKeyDown={(event) => {
            const first = shown()[0];
            if (event.key === "Enter" && first) {
              event.preventDefault();
              start(first);
            }
          }}
        />
      </label>
      <Show
        when={shown().length > 0}
        fallback={
          <p class="apps__empty">
            {apps.loading
              ? "Loading…"
              : text().trim() !== ""
                ? "No app matches."
                : "No apps yet. Mark a game as an app from its right-click menu."}
          </p>
        }
      >
        <div class="apps__grid">
          <For each={shown()}>
            {(app) => (
              <button
                type="button"
                class="apps__app"
                title={app.installed ? `Start ${app.name}` : `${app.name} (not installed)`}
                onClick={() => start(app)}
                onContextMenu={(event) => menu(app, event)}
              >
                <span class="apps__icon">
                  <GameIcon game={app} />
                </span>
                <span class="apps__name">{app.name}</span>
              </button>
            )}
          </For>
        </div>
      </Show>
      <div class="apps__foot">
        <span class="muted">Click to start · right-click for more</span>
        <button
          type="button"
          class="link-btn"
          onClick={() => {
            props.onClose(false);
            store.openExplore({ kind: "apps" });
          }}
        >
          Show all apps
          <ExternalLink class="icon" aria-hidden="true" />
        </button>
      </div>
    </Popover>
  );
}
