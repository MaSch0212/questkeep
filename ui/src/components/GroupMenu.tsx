import Combine from "lucide-solid/icons/combine";
import Pencil from "lucide-solid/icons/pencil";
import Split from "lucide-solid/icons/split";
import { createSignal, Show } from "solid-js";
import { translator } from "../i18n";
import { libraryGroupMerge } from "../i18n/libraryGroupMerge";
import { mergeOf, type GameGroup } from "../lib/grouping";
import { store } from "../lib/store";
import { NO_CATEGORY } from "../lib/views";
import { Menu, type MenuEntry } from "./Menu";
import type { PopoverAnchor } from "./Popover";

const tr = translator(libraryGroupMerge);

/** What can be done with a group of a grouped view: combine it, or edit / split up a combined one. */
export function groupMenuEntries(group: GameGroup): MenuEntry[] {
  const merges = store.activeMerges();
  const merge = mergeOf(group.key, merges);
  if (merge) {
    return [
      {
        label: tr("editCombined"),
        icon: () => <Pencil class="icon" aria-hidden="true" />,
        onSelect: () => store.setMergeDialog({ keys: [], mergeId: merge.id }),
      },
      {
        label: tr("splitUp"),
        icon: () => <Split class="icon" aria-hidden="true" />,
        onSelect: () => {
          void store
            .saveGroupMerges(
              store.groupBy(),
              merges.filter((m) => m.id !== merge.id),
            )
            .then((failure) => failure && store.pushToast("error", failure));
        },
      },
    ];
  }
  if (group.key === NO_CATEGORY || group.key === "") return [];
  return [
    {
      label: tr("combineWith"),
      icon: () => <Combine class="icon" aria-hidden="true" />,
      onSelect: () => store.setMergeDialog({ keys: [group.key], mergeId: null }),
    },
  ];
}

/**
 * Right-click (or the Menu key / Shift+F10) on a group header or a group list entry. Returns
 * the handlers to put on the element and the menu to render next to it.
 */
export function createGroupMenu(group: () => GameGroup) {
  const [anchor, setAnchor] = createSignal<PopoverAnchor | null>(null);
  const entries = () => groupMenuEntries(group());

  function onContextMenu(event: MouseEvent) {
    if (entries().length === 0) return;
    event.preventDefault();
    setAnchor({ x: event.clientX, y: event.clientY });
  }

  /** Opens the menu below `at` (by default the element with the handler). */
  function onKeyDown(event: KeyboardEvent, at?: HTMLElement) {
    const menuKey = event.key === "ContextMenu" || (event.key === "F10" && event.shiftKey);
    if (!menuKey || entries().length === 0) return;
    event.preventDefault();
    const target = at ?? (event.currentTarget as HTMLElement);
    setAnchor({ rect: target.getBoundingClientRect(), align: "start" });
  }

  const menu = () => (
    <Show when={anchor()}>
      {(at) => (
        <Menu
          anchor={at()}
          label={tr("groupActions", { name: group().label })}
          entries={entries()}
          onClose={() => setAnchor(null)}
        />
      )}
    </Show>
  );

  return { onContextMenu, onKeyDown, menu };
}
