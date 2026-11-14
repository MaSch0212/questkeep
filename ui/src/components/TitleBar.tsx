import ArrowLeft from "lucide-solid/icons/arrow-left";
import ArrowRight from "lucide-solid/icons/arrow-right";
import ChevronDown from "lucide-solid/icons/chevron-down";
import Search from "lucide-solid/icons/search";
import Combine from "lucide-solid/icons/combine";
import Copy from "lucide-solid/icons/copy";
import LayoutGrid from "lucide-solid/icons/layout-grid";
import Minus from "lucide-solid/icons/minus";
import Pencil from "lucide-solid/icons/pencil";
import Plus from "lucide-solid/icons/plus";
import Power from "lucide-solid/icons/power";
import RefreshCw from "lucide-solid/icons/refresh-cw";
import Rocket from "lucide-solid/icons/rocket";
import Settings from "lucide-solid/icons/settings";
import Download from "lucide-solid/icons/download";
import Globe from "lucide-solid/icons/globe";
import Square from "lucide-solid/icons/square";
import X from "lucide-solid/icons/x";
import { createSignal, onCleanup, onMount, Show, type JSX } from "solid-js";
import * as api from "../lib/api";
import { addGameMenuItems, type AddGameMenuItem } from "../lib/addGameOptions";
import { translator } from "../i18n";
import { shellCommandBar } from "../i18n/shellCommandBar";
import { shellTitleBar } from "../i18n/shellTitleBar";
import { readCssPx } from "../lib/layoutMode";
import { shopping } from "../i18n/shopping";
import { errorMessage } from "../lib/errors";
import { store } from "../lib/store";
import { syncLabel } from "../lib/syncStatus";
import { AppsLauncher } from "./AppsLauncher";
import { CommandBar, type CommandBarHandle } from "./CommandBar";
import { Logo } from "./Logo";
import { Menu, type MenuEntry } from "./Menu";
import type { PopoverAnchor } from "./Popover";
import { sourceName, SourceIcon } from "./SourceIcon";
import { openLink } from "./settings/openLink";
import { syncAll, SyncPanel, syncStore } from "./SyncPanel";

const tr = translator(shellTitleBar);
const trCommand = translator(shellCommandBar);
const trShop = translator(shopping);

/** "Buy from …" for each store with a website, then "Compare on …" for each price site, as
 * separated groups; all open in the user's browser. */
function shopEntries(): MenuEntry[] {
  const stores = store
    .providers()
    .filter((p) => p.shopUrl !== null)
    .map((p): MenuEntry => ({
      label: trShop("buyFrom", { store: p.name }),
      icon: () => <SourceIcon source={p.id} />,
      onSelect: () => void openLink(p.shopUrl ?? ""),
    }));
  const sites = store.priceSites().map((site): MenuEntry => ({
    label: trShop("compareOn", { site: site.name }),
    icon: () => <Globe class="icon" aria-hidden="true" />,
    onSelect: () => void openLink(site.url),
  }));
  const entries: MenuEntry[] = [];
  for (const group of [stores, sites]) {
    if (group.length > 0) entries.push({ kind: "separator" }, ...group);
  }
  return entries;
}

async function runAddOption(item: AddGameMenuItem) {
  try {
    await api.addGameWithOption(item.providerId, item.optionId);
    store.pushToast("info", tr("opened", { provider: item.providerName }));
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
    ? [{ label: tr("noLaunchers"), disabled: true, onSelect: () => {} }]
    : store.launchers().map((id) => ({
        label: sourceName(id),
        icon: () => <SourceIcon source={id} />,
        onSelect: () => void openLauncher(id),
      }));

const syncEntries = (): MenuEntry[] => [
  {
    label: tr("syncAll"),
    icon: () => <RefreshCw class="icon" aria-hidden="true" />,
    disabled: store.syncState().active,
    onSelect: () => void syncAll(),
  },
  { kind: "separator" },
  ...store
    .providers()
    .filter((p) => p.syncable)
    .map((p): MenuEntry => ({
      label: p.name,
      icon: () => <SourceIcon source={p.id} />,
      onSelect: () => void syncStore(p.id),
    })),
];

// Background downloads (covers, details, icons) show on the sync button too, as a sync does.
const downloading = () => store.backgroundWorkLabel() !== null;
const warningCount = () => store.accountWarnings().length;
const syncButtonLabel = () => {
  const label = store.syncBusy()
    ? syncLabel(store.syncState())
    : (store.backgroundWorkLabel() ?? tr("sync"));
  const count = warningCount();
  if (count === 0) return label;
  return tr("signedOut", { label, count });
};

type Open = "app" | "add" | "launch" | "apps" | "sync" | null;

// The mouse's back and forward buttons, and Alt+Left/Right, move through the history.
function onMouseUp(event: MouseEvent) {
  if (event.button !== 3 && event.button !== 4) return;
  event.preventDefault();
  void (event.button === 3 ? store.goBack() : store.goForward());
}
function onMouseDown(event: MouseEvent) {
  if (event.button === 3 || event.button === 4) event.preventDefault();
}
function onKeyDown(event: KeyboardEvent) {
  if (!event.altKey || event.ctrlKey || event.shiftKey || event.metaKey) return;
  if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
  event.preventDefault();
  void (event.key === "ArrowLeft" ? store.goBack() : store.goForward());
}

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
  // A narrow window has no room for the command bar: a button opens it as a popup instead.
  const [compact, setCompact] = createSignal(false);
  const [floating, setFloating] = createSignal(false);
  let bar: CommandBarHandle | undefined;
  let center: HTMLDivElement | undefined;
  let searchButton: HTMLButtonElement | undefined;

  props.commandRef?.({
    focus: () => {
      if (compact()) setFloating(true);
      else bar?.focus();
    },
  });

  const syncMaximized = () =>
    void Promise.resolve(api.isWindowMaximized())
      .then((value) => setMaximized(value === true))
      .catch(() => {});

  onMount(() => {
    syncMaximized();

    if (center) {
      const element = center;
      const min = readCssPx(element, "--qk-command-min-width", 200);
      const observer = new ResizeObserver(() => {
        // Zero: not laid out (yet), e.g. in tests.
        const width = element.clientWidth;
        const narrow = width > 0 && width < min;
        setCompact(narrow);
        if (!narrow) setFloating(false);
      });
      observer.observe(element);
      onCleanup(() => observer.disconnect());
    }

    window.addEventListener("mouseup", onMouseUp);
    window.addEventListener("mousedown", onMouseDown);
    window.addEventListener("keydown", onKeyDown);
    onCleanup(() => {
      window.removeEventListener("mouseup", onMouseUp);
      window.removeEventListener("mousedown", onMouseDown);
      window.removeEventListener("keydown", onKeyDown);
    });

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
        label: tr("addGameEllipsis"),
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
      ...shopEntries(),
    ];
  };

  const appEntries = (): MenuEntry[] => [
    {
      label: tr("editViews"),
      icon: () => <Pencil class="icon" aria-hidden="true" />,
      onSelect: () => {
        store.showLibrary();
        store.setEditingViews(true);
      },
    },
    { kind: "separator" },
    {
      kind: "submenu",
      label: tr("addGame"),
      icon: () => <Plus class="icon" aria-hidden="true" />,
      entries: addEntries,
    },
    {
      kind: "submenu",
      label: tr("openLauncher"),
      icon: () => <Rocket class="icon" aria-hidden="true" />,
      entries: launcherEntries,
    },
    {
      kind: "submenu",
      label: tr("sync"),
      icon: () => <RefreshCw class="icon" aria-hidden="true" />,
      entries: syncEntries,
    },
    {
      label: tr("severalStores"),
      icon: () => <Combine class="icon" aria-hidden="true" />,
      onSelect: () => store.setDuplicatesOpen(true),
    },
    { kind: "separator" },
    {
      label: tr("settings"),
      icon: () => <Settings class="icon" aria-hidden="true" />,
      onSelect: () => store.showSettings(),
    },
    {
      label: tr("exit"),
      icon: () => <Power class="icon" aria-hidden="true" />,
      onSelect: () => void api.exitApp(),
    },
  ];

  const toolButton = (
    which: "add" | "launch" | "apps" | "sync",
    label: string,
    icon: () => JSX.Element,
  ) => (
    <button
      ref={(el) => (buttons[which] = el)}
      type="button"
      class="titlebar__tool"
      classList={{ "titlebar__tool--open": open() === which }}
      aria-label={label}
      data-tooltip={label}
      aria-haspopup={which === "sync" || which === "apps" ? "dialog" : "menu"}
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
        <div class="titlebar__lead" data-tauri-drag-region>
          <button
            ref={(el) => (buttons.app = el)}
            type="button"
            class="titlebar__brand"
            classList={{ "titlebar__brand--open": open() === "app" }}
            aria-label={tr("questkeepMenu")}
            data-tooltip={tr("questkeepMenu")}
            aria-haspopup="menu"
            aria-expanded={open() === "app"}
            onClick={() => toggle("app")}
          >
            <Logo class="logo titlebar__logo" />
            <span class="titlebar__wordmark">Questkeep</span>
            <ChevronDown class="icon titlebar__brand-arrow" aria-hidden="true" />
          </button>
          <button
            type="button"
            class="titlebar__tool"
            aria-label={tr("back")}
            data-tooltip={tr("backHint")}
            disabled={!store.canGoBack()}
            onClick={() => void store.goBack()}
          >
            <ArrowLeft class="icon" aria-hidden="true" />
          </button>
          <button
            type="button"
            class="titlebar__tool"
            aria-label={tr("forward")}
            data-tooltip={tr("forwardHint")}
            disabled={!store.canGoForward()}
            onClick={() => void store.goForward()}
          >
            <ArrowRight class="icon" aria-hidden="true" />
          </button>
          <span class="titlebar__separator titlebar__separator--start" aria-hidden="true" />
        </div>
        {toolButton("add", tr("addGame"), () => (
          <Plus class="icon" aria-hidden="true" />
        ))}
        {toolButton("launch", tr("openLauncher"), () => (
          <Rocket class="icon" aria-hidden="true" />
        ))}
        {toolButton("apps", tr("apps"), () => (
          <LayoutGrid class="icon" aria-hidden="true" />
        ))}
        <span class="titlebar__sync">
          {toolButton("sync", syncButtonLabel(), () => (
            <RefreshCw
              class="icon"
              classList={{ "icon--spin": store.syncBusy() || downloading() }}
              aria-hidden="true"
            />
          ))}
          <Show when={store.syncBusy() || downloading()}>
            <span class="titlebar__sync-dot" aria-hidden="true" />
          </Show>
          <Show when={warningCount() > 0}>
            <span class="titlebar__sync-warn" aria-hidden="true" />
          </Show>
        </span>
      </div>
      <div class="titlebar__center" data-tauri-drag-region ref={(el) => (center = el)}>
        <Show when={compact()} fallback={<CommandBar ref={(handle) => (bar = handle)} />}>
          <button
            ref={(el) => (searchButton = el)}
            type="button"
            class="titlebar__tool"
            classList={{ "titlebar__tool--open": floating() }}
            aria-label={trCommand("findGame")}
            data-tooltip={`${trCommand("findGame")} (Ctrl+K)`}
            aria-expanded={floating()}
            onPointerDown={(event) => {
              // The open popup would close on this press and the click reopen it.
              if (floating()) event.stopPropagation();
            }}
            onClick={() => setFloating((value) => !value)}
          >
            <Search class="icon" aria-hidden="true" />
          </button>
          <Show when={floating()}>
            <div class="titlebar__command-popup">
              <CommandBar
                autoFocus
                onDismiss={() => {
                  setFloating(false);
                  searchButton?.focus();
                }}
              />
            </div>
          </Show>
        </Show>
      </div>
      <div class="titlebar__end" data-tauri-drag-region>
        <Show when={store.update()}>
          {(update) => (
            <button
              type="button"
              class="titlebar__update"
              aria-label={tr("updateTo", { version: update().version })}
              data-tooltip={tr("updateAvailable", { version: update().version })}
              onClick={() => store.setUpdateOpen(true)}
            >
              <Download class="icon" aria-hidden="true" /> {tr("update")}
            </button>
          )}
        </Show>
        <button
          type="button"
          class="titlebar__tool"
          classList={{ "titlebar__tool--open": store.page() === "settings" }}
          aria-label={tr("settings")}
          data-tooltip={tr("settings")}
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
            aria-label={tr("minimize")}
            data-tooltip={tr("minimize")}
            onClick={() => void api.minimizeWindow()}
          >
            <Minus class="titlebar__icon" aria-hidden="true" />
          </button>
          <button
            type="button"
            class="titlebar__button"
            tabIndex={-1}
            aria-label={maximized() ? tr("restore") : tr("maximize")}
            data-tooltip={maximized() ? tr("restore") : tr("maximize")}
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
            aria-label={tr("close")}
            data-tooltip={tr("close")}
            onClick={() => void api.closeWindow()}
          >
            <X class="titlebar__icon" aria-hidden="true" />
          </button>
        </div>
      </div>
      <Show when={open() !== null && open() !== "sync" && open() !== "apps" && anchor()}>
        {(at) => (
          <Menu
            anchor={at()}
            label={
              open() === "app" ? "Questkeep" : open() === "add" ? tr("addGame") : tr("openLauncher")
            }
            inside={() => [buttons[open() ?? "app"]]}
            onClose={close}
            entries={
              open() === "app" ? appEntries() : open() === "add" ? addEntries() : launcherEntries()
            }
          />
        )}
      </Show>
      <Show when={open() === "apps" && anchor()}>
        {(at) => <AppsLauncher anchor={at()} onClose={close} inside={() => [buttons.apps]} />}
      </Show>
      <Show when={open() === "sync" && anchor()}>
        {(at) => <SyncPanel anchor={at()} onClose={close} inside={() => [buttons.sync]} />}
      </Show>
    </header>
  );
}
