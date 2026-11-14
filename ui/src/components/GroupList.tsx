import Layers from "lucide-solid/icons/layers";
import { createMemo, For, Show } from "solid-js";
import { translator } from "../i18n";
import { libraryTopBar } from "../i18n/libraryTopBar";
import { store } from "../lib/store";
import { SourceIcon } from "./SourceIcon";

const tr = translator(libraryTopBar);

/** "All groups" in the list; any other key is a group's. */
const ALL = "";

/**
 * The open view's groups beside the games (the view's "Show groups as a list"). "All groups"
 * shows the view as usual; a group shows only its games. Up/Down move through the list and the
 * games follow at once; the pick is remembered per view.
 */
export function GroupList() {
  const total = createMemo(() => new Set(store.allGroups().flatMap((g) => g.games)).size);
  const keys = createMemo(() => [ALL, ...store.allGroups().map((g) => g.key)]);
  const current = () => store.pickedGroup() ?? ALL;
  let list: HTMLUListElement | undefined;

  function pick(key: string) {
    store.pickGroup(key === ALL ? null : key);
  }

  function onKeyDown(event: KeyboardEvent) {
    const all = keys();
    const index = all.indexOf(current());
    const next =
      event.key === "ArrowDown"
        ? Math.min(all.length - 1, index + 1)
        : event.key === "ArrowUp"
          ? Math.max(0, index - 1)
          : event.key === "Home"
            ? 0
            : event.key === "End"
              ? all.length - 1
              : null;
    if (next === null) return;
    event.preventDefault();
    const key = all[next];
    if (key === undefined) return;
    pick(key);
    list
      ?.querySelector<HTMLElement>(`[data-group-index="${next}"]`)
      ?.scrollIntoView?.({ block: "nearest" });
  }

  return (
    <nav class="group-list" aria-label={tr("groupListLabel")}>
      <ul
        class="group-list__items"
        role="listbox"
        aria-label={tr("groupListLabel")}
        aria-activedescendant={`group-option-${keys().indexOf(current())}`}
        tabindex="0"
        ref={(el) => (list = el)}
        onKeyDown={onKeyDown}
      >
        <li
          id="group-option-0"
          data-group-index="0"
          role="option"
          aria-selected={current() === ALL}
          class="nav-item nav-item--icon view-item"
          classList={{ "nav-item--active": current() === ALL }}
          onClick={() => pick(ALL)}
        >
          <Layers class="icon" aria-hidden="true" />
          <span class="nav-item__text">{tr("allGroups")}</span>
          <span class="view-item__count">{total()}</span>
        </li>
        <For each={store.allGroups()}>
          {(group, index) => (
            <li
              id={`group-option-${index() + 1}`}
              data-group-index={index() + 1}
              role="option"
              aria-selected={current() === group.key}
              class="nav-item nav-item--icon view-item"
              classList={{ "nav-item--active": current() === group.key }}
              data-tooltip={group.label}
              onClick={() => pick(group.key)}
            >
              <Show when={group.source}>{(source) => <SourceIcon source={source()} />}</Show>
              <span class="nav-item__text">{group.label}</span>
              <span class="view-item__count">{group.games.length}</span>
            </li>
          )}
        </For>
      </ul>
    </nav>
  );
}
