import AppWindow from "lucide-solid/icons/app-window";
import Combine from "lucide-solid/icons/combine";
import Gamepad from "lucide-solid/icons/gamepad-2";
import EyeOff from "lucide-solid/icons/eye-off";
import Eye from "lucide-solid/icons/eye";
import FolderOpen from "lucide-solid/icons/folder-open";
import PackageMinus from "lucide-solid/icons/package-minus";
import Pencil from "lucide-solid/icons/pencil";
import Play from "lucide-solid/icons/play";
import RefreshCw from "lucide-solid/icons/refresh-cw";
import Star from "lucide-solid/icons/star";
import Trash from "lucide-solid/icons/trash";
import Download from "lucide-solid/icons/download";
import { createResource, createSignal, Show } from "solid-js";
import type { Game } from "../bindings/Game";
import {
  activateGame,
  launchGame,
  openInstallFolder,
  refreshMetadata,
  removeGame,
  uninstallGame,
  updateGame,
} from "../lib/actions";
import { translator } from "../i18n";
import { shellMenus } from "../i18n/shellMenus";
import { launchProfiles } from "../lib/api";
import { playCopy } from "../lib/copies";
import { store } from "../lib/store";
import { CombinePicker } from "./CombinePicker";
import { ConfirmDialog } from "./ConfirmDialog";
import { EditGameDialog } from "./EditGameDialog";
import { Menu, type MenuEntry } from "./Menu";
import type { PopoverAnchor } from "./Popover";
import { SourceIcon } from "./SourceIcon";

const tr = translator(shellMenus);

type Step = "menu" | "edit" | "combine" | "uninstall" | "remove";

const iconOf = (Icon: typeof Play) => () => <Icon class="icon" aria-hidden="true" />;
/** A profile's store icon, or a play symbol for an executable. */
const sourceIcon = (source: string | null) =>
  source ? () => <SourceIcon source={source} /> : iconOf(Play);

/** Runs an action that resolves to an error message (or null) and shows a failure as a toast. */
async function report(work: Promise<string | null>, done?: string) {
  const failure = await work;
  if (failure) store.pushToast("error", failure);
  else if (done) store.pushToast("info", done);
}

/**
 * The right-click menu of one game (also the Menu key and Shift+F10), plus the dialogs its
 * entries open (Edit, Combine, Uninstall, Remove). Closes itself when done.
 */
export function GameContextMenu(props: {
  game: Game;
  anchor: PopoverAnchor;
  /** Called once the menu and any dialog it opened are closed. */
  onDone: (returnFocus: boolean) => void;
}) {
  const [step, setStep] = createSignal<Step>("menu");
  const game = () => props.game;
  const [profiles] = createResource(
    () => props.game.id,
    (id) =>
      Promise.resolve()
        .then(() => launchProfiles(id))
        .catch(() => null),
  );

  const running = () => store.isRunning(game().id);
  const uninstallCopy = () => {
    const copy = playCopy(game());
    return copy?.installed ? copy : undefined;
  };
  const canUninstall = () => {
    const copy = uninstallCopy();
    return (
      copy !== undefined &&
      copy.sourceGameId !== null &&
      store.providers().some((p) => p.id === copy.source && p.canUninstall)
    );
  };

  /** Profiles to play with when there is more than one. */
  const playWith = (): MenuEntry[] => {
    const list = profiles();
    const installed = list?.profiles.filter((p) => p.installed) ?? [];
    if (!list || installed.length < 2) return [];
    return [
      {
        kind: "submenu",
        label: tr("playWith"),
        entries: () =>
          installed.map((profile) => ({
            label: profile.name,
            icon: sourceIcon(profile.source),
            hint: profile.id === list.playId ? tr("defaultProfile") : undefined,
            disabled: running(),
            onSelect: () => void report(launchGame(game().id, null, profile.id)),
          })),
      },
    ];
  };

  const statusEntries = (): MenuEntry[] => [
    {
      label: tr("none"),
      checked: game().completionStatus === null,
      onSelect: () => void store.setCompletionStatus([game().id], null),
    },
    ...store.completion().statuses.map((status) => ({
      label: status.name,
      checked: game().completionStatus === status.id,
      onSelect: () => void store.setCompletionStatus([game().id], status.id),
    })),
  ];

  const categoryEntries = (): MenuEntry[] =>
    store.categories().length === 0
      ? [{ label: tr("noCategories"), disabled: true, onSelect: () => undefined }]
      : store.categories().map((category) => {
          const has = game().categories.includes(category.id);
          return {
            label: category.name,
            checked: has,
            keepOpen: true,
            onSelect: () =>
              void store.setGameCategories(
                game().id,
                has
                  ? game().categories.filter((id) => id !== category.id)
                  : [...game().categories, category.id],
              ),
          };
        });

  const statusName = () =>
    store.completion().statuses.find((s) => s.id === game().completionStatus)?.name;

  const entries = (): MenuEntry[] => [
    game().installed
      ? {
          label: running() ? tr("running") : tr("play"),
          icon: iconOf(Play),
          hint: tr("enterKey"),
          disabled: running(),
          onSelect: () => void activateGame(game().id),
        }
      : {
          label: tr("install"),
          icon: iconOf(Download),
          hint: tr("enterKey"),
          onSelect: () => void activateGame(game().id),
        },
    ...playWith(),
    { label: tr("openGamePage"), onSelect: () => store.selectAndOpen(game().id) },
    { kind: "separator" },
    {
      label: game().favorite ? tr("removeFromFavorites") : tr("addToFavorites"),
      icon: iconOf(Star),
      onSelect: () => void report(updateGame(game().id, { favorite: !game().favorite })),
    },
    // Apps have no completion status.
    ...(game().kind === "app"
      ? []
      : [
          {
            kind: "submenu" as const,
            label: tr("completionStatus"),
            hint: statusName(),
            entries: statusEntries,
          },
        ]),
    { kind: "submenu", label: tr("categories"), entries: categoryEntries },
    { label: tr("edit"), icon: iconOf(Pencil), hint: "F2", onSelect: () => setStep("edit") },
    { kind: "separator" },
    {
      label: tr("refreshMetadata"),
      icon: iconOf(RefreshCw),
      onSelect: () => void report(refreshMetadata(game().id), tr("refreshingMetadata")),
    },
    {
      label: tr("combineWith"),
      icon: iconOf(Combine),
      onSelect: () => setStep("combine"),
    },
    {
      label: game().kind === "app" ? tr("markAsGame") : tr("markAsApp"),
      icon: iconOf(game().kind === "app" ? Gamepad : AppWindow),
      onSelect: () =>
        void report(updateGame(game().id, { kind: game().kind === "app" ? "game" : "app" })),
    },
    {
      label: game().hidden ? tr("unhide") : tr("hide"),
      icon: iconOf(game().hidden ? Eye : EyeOff),
      hint: tr("delKey"),
      onSelect: () => void report(updateGame(game().id, { hidden: !game().hidden })),
    },
    { kind: "separator" },
    ...(game().installDir === null
      ? []
      : [
          {
            label: tr("openInstallFolder"),
            icon: iconOf(FolderOpen),
            onSelect: () => void report(openInstallFolder(game().id)),
          },
        ]),
    ...(canUninstall()
      ? [
          {
            label: tr("uninstallEllipsis"),
            icon: iconOf(PackageMinus),
            danger: true,
            onSelect: () => setStep("uninstall"),
          },
        ]
      : []),
    {
      label: tr("removeFromLibrary"),
      icon: iconOf(Trash),
      danger: true,
      onSelect: () => setStep("remove"),
    },
  ];

  /** The menu closed: done, unless the chosen entry opened a dialog (it sets the step first). */
  function menuClosed(returnFocus: boolean) {
    queueMicrotask(() => {
      if (step() === "menu") props.onDone(returnFocus);
    });
  }

  return (
    <>
      <Show when={step() === "menu"}>
        <Menu
          anchor={props.anchor}
          label={tr("actionsLabel", { name: game().name })}
          entries={entries()}
          onClose={menuClosed}
        />
      </Show>
      <Show when={step() === "combine"}>
        <CombinePicker
          game={game()}
          anchor={props.anchor}
          inside={() => []}
          onPick={(other) => {
            props.onDone(false);
            store.openCombine([game().id, other.id]);
          }}
          onClose={(returnFocus) => props.onDone(returnFocus)}
        />
      </Show>
      <Show when={step() === "edit"}>
        <EditGameDialog game={game()} onClose={() => props.onDone(true)} />
      </Show>
      <Show when={step() === "uninstall"}>
        <ConfirmDialog
          title={tr("uninstallTitle")}
          message={tr("uninstallMessage", { name: game().name })}
          confirmLabel={tr("uninstall")}
          onConfirm={() =>
            void report(
              uninstallGame(game().id, uninstallCopy()?.id ?? null),
              tr("uninstallHanded"),
            )
          }
          onClose={() => props.onDone(true)}
        />
      </Show>
      <Show when={step() === "remove"}>
        <ConfirmDialog
          title={tr("removeTitle")}
          message={tr("removeMessage", { name: game().name })}
          confirmLabel={tr("remove")}
          onConfirm={() => void report(removeGame(game().id))}
          onClose={() => props.onDone(true)}
        />
      </Show>
    </>
  );
}
