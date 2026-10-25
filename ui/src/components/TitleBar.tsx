import ChevronDown from "lucide-solid/icons/chevron-down";
import Copy from "lucide-solid/icons/copy";
import Minus from "lucide-solid/icons/minus";
import Pencil from "lucide-solid/icons/pencil";
import Plus from "lucide-solid/icons/plus";
import Power from "lucide-solid/icons/power";
import RefreshCw from "lucide-solid/icons/refresh-cw";
import Rocket from "lucide-solid/icons/rocket";
import Settings from "lucide-solid/icons/settings";
import Square from "lucide-solid/icons/square";
import X from "lucide-solid/icons/x";
import { createSignal, onCleanup, onMount, Show, type JSX } from "solid-js";
import * as api from "../lib/api";
import { addGameMenuItems, type AddGameMenuItem } from "../lib/addGameOptions";
import { errorMessage } from "../lib/errors";
import { store } from "../lib/store";
import { syncLabel } from "../lib/syncStatus";
import { CommandBar, type CommandBarHandle } from "./CommandBar";
import { Logo } from "./Logo";
import { Menu, type MenuEntry } from "./Menu";
import type { PopoverAnchor } from "./Popover";
import { sourceName, SourceIcon } from "./SourceIcon";
import { syncAll, SyncPanel, syncStore } from "./SyncPanel";

async function runAddOption(item: AddGameMenuItem) {
  try {
    await api.addGameWithOption(item.providerId, item.optionId);
    store.pushToast(
      "info",
      `Opened ${item.providerName}. The new game appears here once it is set up there.`,
    );
  } catch (error) {
    store.pushToast("error", errorMessage(error));
  }
}

async function openLauncher(id: string) {
  try {
    await api.openLauncher(id);
  } catch (error) {
    store.pushToast("error", errorMessage(error));
  }
}

const launcherEntries = (): MenuEntry[] =>
  store.launchers().length === 0
    ? [{ label: "No store launchers found", disabled: true, onSelect: () => {} }]
    : store.launchers().map((id) => ({
        label: sourceName(id),
        icon: () => <SourceIcon source={id} />,
        onSelect: () => void openLauncher(id),
      }));

const syncEntries = (): MenuEntry[] => [
  {
    label: "Sync all",
    icon: () => <RefreshCw class="icon" aria-hidden="true" />,
    disabled: store.syncState().active,
    onSelect: () => void syncAll(),
  },
  { kind: "separator" },
  ...store
    .providers()
    .filter((p) => p.canScan)
    .map((p): MenuEntry => ({
      label: p.name,
      icon: () => <SourceIcon source={p.id} />,
      onSelect: () => void syncStore(p.id),
    })),
];

type Open = "app" | "add" | "launch" | "sync" | null;

/**
 * Replaces the native window frame (`decorations: false`). Left: the Questkeep menu (every app
 * action in one place) and the Add game, Open launcher and Sync buttons; centre: the command
 * bar; right: Settings and the window buttons. Dragging the empty bar moves the window and
 * double-clicking it toggles maximize (`data-tauri-drag-region`). The window buttons are left
 * out of the tab order like native caption buttons; Win+Up/Down and Alt+F4 remain their keys.
 */
export function TitleBar(props: { onAdd: () => void; commandRef?: (h: CommandBarHandle) => void }) {
  const [maximized, setMaximized] = createSignal(false);
  const [focused, setFocused] = createSignal(true);
  const [open, setOpen] = createSignal<Open>(null);
  const [anchor, setAnchor] = createSignal<PopoverAnchor | null>(null);
  const buttons: Partial<Record<Exclude<Open, null>, HTMLButtonElement>> = {};

  const syncMaximized = () =>
    void Promise.resolve(api.isWindowMaximized())
      .then((value) => setMaximized(value === true))
      .catch(() => {});

  onMount(() => {
    syncMaximized();

    const unlisteners = [
      api.onWindowResized(syncMaximized),
      api.onWindowFocusChanged((value) => setFocused(value)),
    ];
    onCleanup(() => {
      for (const unlisten of unlisteners) {
        void Promise.resolve(unlisten)
          .then((fn) => fn?.())
          .catch(() => {});
      }
    });
  });

  function toggle(which: Exclude<Open, null>) {
    if (open() === which) {
      setOpen(null);
      return;
    }
    const rect = buttons[which]?.getBoundingClientRect();
    if (!rect) return;
    if (which === "launch" || which === "app") void store.loadLaunchers();
    setAnchor({ rect, align: "start" });
    setOpen(which);
  }

  function close(returnFocus: boolean) {
    const which = open();
    setOpen(null);
    if (returnFocus && which) buttons[which]?.focus();
  }

  const addEntries = (): MenuEntry[] => {
    const options = addGameMenuItems(store.providers());
    return [
      {
        label: "Add a game…",
        icon: () => <Plus class="icon" aria-hidden="true" />,
        onSelect: props.onAdd,
      },
      ...(options.length > 0 ? [{ kind: "separator" } as const] : []),
      ...options.map((item): MenuEntry => ({
        label: item.label,
        hint: item.providerName,
        icon: () => <SourceIcon source={item.providerId} />,
        onSelect: () => void runAddOption(item),
      })),
    ];
  };

  const appEntries = (): MenuEntry[] => [
    {
      label: "Edit views",
      icon: () => <Pencil class="icon" aria-hidden="true" />,
      onSelect: () => {
        store.showLibrary();
        store.setEditingViews(true);
      },
    },
    { kind: "separator" },
    {
      kind: "submenu",
      label: "Add game",
      icon: () => <Plus class="icon" aria-hidden="true" />,
      entries: addEntries,
    },
    {
      kind: "submenu",
      label: "Open launcher",
      icon: () => <Rocket class="icon" aria-hidden="true" />,
      entries: launcherEntries,
    },
    {
      kind: "submenu",
      label: "Sync",
      icon: () => <RefreshCw class="icon" aria-hidden="true" />,
      entries: syncEntries,
    },
    { kind: "separator" },
    {
      label: "Settings",
      icon: () => <Settings class="icon" aria-hidden="true" />,
      onSelect: () => store.showSettings(),
    },
    {
      label: "Exit Questkeep",
      icon: () => <Power class="icon" aria-hidden="true" />,
      onSelect: () => void api.exitApp(),
    },
  ];

  const toolButton = (which: "add" | "launch" | "sync", label: string, icon: () => JSX.Element) => (
    <button
      ref={(el) => (buttons[which] = el)}
      type="button"
      class="titlebar__tool"
      classList={{ "titlebar__tool--open": open() === which }}
      aria-label={label}
      title={label}
      aria-haspopup={which === "sync" ? "dialog" : "menu"}
      aria-expanded={open() === which}
      onClick={() => toggle(which)}
    >
      {icon()}
    </button>
  );

  return (
    <header
      class="titlebar"
      classList={{ "titlebar--inactive": !focused() }}
      data-tauri-drag-region
    >
      <div class="titlebar__start" data-tauri-drag-region>
        <button
          ref={(el) => (buttons.app = el)}
          type="button"
          class="titlebar__brand"
          classList={{ "titlebar__brand--open": open() === "app" }}
          aria-label="Questkeep menu"
          title="Questkeep menu"
          aria-haspopup="menu"
          aria-expanded={open() === "app"}
          onClick={() => toggle("app")}
        >
          <Logo class="logo titlebar__logo" />
          <span class="titlebar__wordmark">Questkeep</span>
          <ChevronDown class="icon titlebar__brand-arrow" aria-hidden="true" />
        </button>
        {toolButton("add", "Add game", () => (
          <Plus class="icon" aria-hidden="true" />
        ))}
        {toolButton("launch", "Open launcher", () => (
          <Rocket class="icon" aria-hidden="true" />
        ))}
        <span class="titlebar__sync">
          {toolButton("sync", store.syncBusy() ? syncLabel(store.syncState()) : "Sync", () => (
            <RefreshCw
              class="icon"
              classList={{ "icon--spin": store.syncBusy() }}
              aria-hidden="true"
            />
          ))}
          <Show when={store.syncBusy()}>
            <span class="titlebar__sync-dot" aria-hidden="true" />
          </Show>
        </span>
      </div>
      <div class="titlebar__center" data-tauri-drag-region>
        <CommandBar ref={props.commandRef} />
      </div>
      <div class="titlebar__end">
        <button
          type="button"
          class="titlebar__tool"
          classList={{ "titlebar__tool--open": store.page() === "settings" }}
          aria-label="Settings"
          title="Settings"
          aria-current={store.page() === "settings" ? "page" : undefined}
          onClick={() => store.showSettings()}
        >
          <Settings class="icon" aria-hidden="true" />
        </button>
        <span class="titlebar__separator" aria-hidden="true" />
        <div class="titlebar__controls">
          <button
            type="button"
            class="titlebar__button"
            tabIndex={-1}
            aria-label="Minimize"
            title="Minimize"
            onClick={() => void api.minimizeWindow()}
          >
            <Minus class="titlebar__icon" aria-hidden="true" />
          </button>
          <button
            type="button"
            class="titlebar__button"
            tabIndex={-1}
            aria-label={maximized() ? "Restore" : "Maximize"}
            title={maximized() ? "Restore" : "Maximize"}
            onClick={() => void api.toggleMaximizeWindow()}
          >
            <Show
              when={maximized()}
              fallback={<Square class="titlebar__icon titlebar__icon--small" aria-hidden="true" />}
            >
              <Copy
                class="titlebar__icon titlebar__icon--small titlebar__icon--mirrored"
                aria-hidden="true"
              />
            </Show>
          </button>
          <button
            type="button"
            class="titlebar__button titlebar__button--close"
            tabIndex={-1}
            aria-label="Close"
            title="Close"
            onClick={() => void api.closeWindow()}
          >
            <X class="titlebar__icon" aria-hidden="true" />
          </button>
        </div>
      </div>
      <Show when={open() !== null && open() !== "sync" && anchor()}>
        {(at) => (
          <Menu
            anchor={at()}
            label={open() === "app" ? "Questkeep" : open() === "add" ? "Add game" : "Open launcher"}
            inside={() => [buttons[open() ?? "app"]]}
            onClose={close}
            entries={
              open() === "app" ? appEntries() : open() === "add" ? addEntries() : launcherEntries()
            }
          />
        )}
      </Show>
      <Show when={open() === "sync" && anchor()}>
        {(at) => <SyncPanel anchor={at()} onClose={close} inside={() => [buttons.sync]} />}
      </Show>
    </header>
  );
}
