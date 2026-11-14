import AppWindow from "lucide-solid/icons/app-window";
import Combine from "lucide-solid/icons/combine";
import Gamepad from "lucide-solid/icons/gamepad-2";
import Download from "lucide-solid/icons/download";
import EyeOff from "lucide-solid/icons/eye-off";
import Eye from "lucide-solid/icons/eye";
import Pencil from "lucide-solid/icons/pencil";
import RefreshCw from "lucide-solid/icons/refresh-cw";
import Star from "lucide-solid/icons/star";
import Trash from "lucide-solid/icons/trash";
import { createSignal, Show } from "solid-js";
import type { Game } from "../bindings/Game";
import { translator } from "../i18n";
import { shellMenus } from "../i18n/shellMenus";
import { changeAll, installAll, refreshAll, removeAll } from "../lib/bulkActions";
import { categoryCoverage, common } from "../lib/selection";
import { store } from "../lib/store";
import { ConfirmDialog } from "./ConfirmDialog";
import { extensionEntries } from "./ExtensionIcon";
import { Menu, type MenuEntry } from "./Menu";
import type { PopoverAnchor } from "./Popover";

const tr = translator(shellMenus);

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
  const allApps = () => props.games.every((g) => g.kind === "app");

  const statusEntries = (): MenuEntry[] => [
    {
      label: tr("none"),
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
      ? [{ label: tr("noCategories"), disabled: true, onSelect: () => undefined }]
      : store.categories().map((category) => {
          const coverage = categoryCoverage(props.games, category.id);
          return {
            label: category.name,
            checked: coverage === "all",
            hint: coverage === "some" ? tr("some") : undefined,
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
            label: tr("installAll"),
            icon: iconOf(Download),
            onSelect: () => void installAll(props.games),
          },
          { kind: "separator" } as const,
        ]
      : []),
    {
      label: favorite() === true ? tr("removeAllFromFavorites") : tr("addAllToFavorites"),
      icon: iconOf(Star),
      onSelect: () =>
        void changeAll(props.games, { favorite: favorite() !== true, ...NO_CATEGORIES }),
    },
    { kind: "submenu", label: tr("completionStatus"), entries: statusEntries },
    { kind: "submenu", label: tr("categories"), entries: categoryEntries },
    {
      label: tr("edit"),
      icon: iconOf(Pencil),
      hint: "F2",
      disabled: true,
      onSelect: () => undefined,
    },
    { kind: "separator" },
    {
      label: tr("refreshMetadata"),
      icon: iconOf(RefreshCw),
      onSelect: () => void refreshAll(props.games),
    },
    {
      label: tr("combineInto"),
      icon: iconOf(Combine),
      onSelect: () => store.openCombine(ids()),
    },
    {
      label: allApps() ? tr("markAllAsGames") : tr("markAllAsApps"),
      icon: iconOf(allApps() ? Gamepad : AppWindow),
      onSelect: () =>
        void changeAll(props.games, { kind: allApps() ? "game" : "app", ...NO_CATEGORIES }),
    },
    {
      label: hidden() === true ? tr("unhideAll") : tr("hideAll"),
      icon: iconOf(hidden() === true ? Eye : EyeOff),
      hint: tr("delKey"),
      onSelect: () => void changeAll(props.games, { hidden: hidden() !== true, ...NO_CATEGORIES }),
    },
    ...extensionEntries("games", { games: props.games }),
    { kind: "separator" },
    {
      label: tr("removeFromLibrary"),
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
          label={tr("selectedGames", { count: count() })}
          entries={entries()}
          onClose={menuClosed}
        />
      </Show>
      <Show when={removing()}>
        <ConfirmDialog
          title={tr("removeGamesTitle")}
          message={tr("removeGamesMessage", { count: count() })}
          confirmLabel={tr("remove")}
          onConfirm={() => void removeAll(props.games)}
          onClose={() => props.onDone(true)}
        />
      </Show>
    </>
  );
}
