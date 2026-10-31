import Combine from "lucide-solid/icons/combine";
import Download from "lucide-solid/icons/download";
import EyeOff from "lucide-solid/icons/eye-off";
import Eye from "lucide-solid/icons/eye";
import Pencil from "lucide-solid/icons/pencil";
import RefreshCw from "lucide-solid/icons/refresh-cw";
import Star from "lucide-solid/icons/star";
import Trash from "lucide-solid/icons/trash";
import { createSignal, Show } from "solid-js";
import type { Game } from "../bindings/Game";
import { changeAll, installAll, refreshAll, removeAll } from "../lib/bulkActions";
import { categoryCoverage, common } from "../lib/selection";
import { store } from "../lib/store";
import { ConfirmDialog } from "./ConfirmDialog";
import { Menu, type MenuEntry } from "./Menu";
import type { PopoverAnchor } from "./Popover";

const iconOf = (Icon: typeof Star) => () => <Icon class="icon" aria-hidden="true" />;
const NO_CATEGORIES = { addCategories: [], removeCategories: [] };

/** The right-click menu of a multi-selection: everything in it acts on all selected games. */
export function MultiContextMenu(props: {
  games: Game[];
  anchor: PopoverAnchor;
  onDone: (returnFocus: boolean) => void;
}) {
  const [removing, setRemoving] = createSignal(false);
  const count = () => props.games.length;
  const ids = () => props.games.map((g) => g.id);
  const favorite = () => common(props.games, (g) => g.favorite);
  const hidden = () => common(props.games, (g) => g.hidden);
  const status = () => common(props.games, (g) => g.completionStatus);

  const statusEntries = (): MenuEntry[] => [
    {
      label: "None",
      checked: status() === null,
      onSelect: () => void store.setCompletionStatus(ids(), null),
    },
    ...store.completion().statuses.map((s) => ({
      label: s.name,
      checked: status() === s.id,
      onSelect: () => void store.setCompletionStatus(ids(), s.id),
    })),
  ];

  const categoryEntries = (): MenuEntry[] =>
    store.categories().length === 0
      ? [{ label: "No categories yet", disabled: true, onSelect: () => undefined }]
      : store.categories().map((category) => {
          const coverage = categoryCoverage(props.games, category.id);
          return {
            label: category.name,
            checked: coverage === "all",
            hint: coverage === "some" ? "Some" : undefined,
            keepOpen: true,
            onSelect: () =>
              void changeAll(props.games, {
                addCategories: coverage === "all" ? [] : [category.id],
                removeCategories: coverage === "all" ? [category.id] : [],
              }),
          };
        });

  const entries = (): MenuEntry[] => [
    ...(props.games.some((g) => !g.installed)
      ? [
          {
            label: "Install all",
            icon: iconOf(Download),
            onSelect: () => void installAll(props.games),
          },
          { kind: "separator" } as const,
        ]
      : []),
    {
      label: favorite() === true ? "Remove all from favorites" : "Add all to favorites",
      icon: iconOf(Star),
      onSelect: () =>
        void changeAll(props.games, { favorite: favorite() !== true, ...NO_CATEGORIES }),
    },
    { kind: "submenu", label: "Completion status", entries: statusEntries },
    { kind: "submenu", label: "Categories", entries: categoryEntries },
    { label: "Edit…", icon: iconOf(Pencil), hint: "F2", disabled: true, onSelect: () => undefined },
    { kind: "separator" },
    {
      label: "Refresh metadata",
      icon: iconOf(RefreshCw),
      onSelect: () => void refreshAll(props.games),
    },
    {
      label: "Combine into one game…",
      icon: iconOf(Combine),
      onSelect: () => store.openCombine(ids()),
    },
    {
      label: hidden() === true ? "Unhide all" : "Hide all",
      icon: iconOf(hidden() === true ? Eye : EyeOff),
      hint: "Del",
      onSelect: () => void changeAll(props.games, { hidden: hidden() !== true, ...NO_CATEGORIES }),
    },
    { kind: "separator" },
    {
      label: "Remove from library…",
      icon: iconOf(Trash),
      danger: true,
      onSelect: () => setRemoving(true),
    },
  ];

  function menuClosed(returnFocus: boolean) {
    queueMicrotask(() => {
      if (!removing()) props.onDone(returnFocus);
    });
  }

  return (
    <>
      <Show when={!removing()}>
        <Menu
          anchor={props.anchor}
          label={`${count()} selected games`}
          entries={entries()}
          onClose={menuClosed}
        />
      </Show>
      <Show when={removing()}>
        <ConfirmDialog
          title="Remove games"
          message={<>Remove {count()} games from your library?</>}
          confirmLabel="Remove"
          onConfirm={() => void removeAll(props.games)}
          onClose={() => props.onDone(true)}
        />
      </Show>
    </>
  );
}
