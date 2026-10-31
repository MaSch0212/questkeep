import { createSignal, onCleanup, onMount, Show } from "solid-js";
import {
  DEFAULT_SPLIT_BREAKPOINT,
  isSplitWidth,
  readCssPx,
  readSplitBreakpoint,
} from "./lib/layoutMode";
import { clampWidth } from "./lib/viewUi";
import { AddGameDialog } from "./components/AddGameDialog";
import { DuplicatesDialog } from "./components/DuplicatesDialog";
import { FirstRunDialog } from "./components/FirstRunDialog";
import { CombineDialog } from "./components/CombineDialog";
import { GameContextMenu } from "./components/GameContextMenu";
import { MultiContextMenu } from "./components/MultiContextMenu";
import { MultiSelectPanel } from "./components/MultiSelectPanel";
import { IgdbHint } from "./components/IgdbHint";
import { DetailsPanel } from "./components/DetailsPanel";
import { UpdateDialog } from "./components/UpdateDialog";
import { DetailsView } from "./components/DetailsView";
import { EmptyDetails, type PanelPin } from "./components/PanelHeader";
import { Splitter } from "./components/Splitter";
import { GameGrid } from "./components/GameGrid";
import { GameList } from "./components/GameList";
import { HomePage } from "./components/HomePage";
import { SettingsPage } from "./components/settings/SettingsPage";
import { Sidebar } from "./components/Sidebar";
import { TitleBar } from "./components/TitleBar";
import { Toasts } from "./components/Toasts";
import { FilterPills } from "./components/FilterPills";
import { TopBar } from "./components/TopBar";
import type { CommandBarHandle } from "./components/CommandBar";
import type { KeywordFilterHandle } from "./components/KeywordFilter";
import { store } from "./lib/store";

/** Grid and table views have the side panel; the details layout has its page instead. */
const isPanelView = () => store.viewMode() !== "details";

/** Fallbacks for the panel and list widths in theme.css (`--qk-details-width`, ...). */
const DEFAULT_SIZES = { panel: 380, panelMin: 320, list: 320, listMin: 240 };

/** A printable character typed without modifiers (space stays with the focused button). */
function isTypeToFilter(event: KeyboardEvent): boolean {
  return (
    event.key.length === 1 &&
    event.key !== " " &&
    event.key !== "/" &&
    !event.ctrlKey &&
    !event.altKey &&
    !event.metaKey
  );
}

/** Whether a key went to the game list (or nowhere in particular). */
function inLibrary(target: EventTarget | null): boolean {
  if (!(target instanceof Element) || target === document.body) return true;
  return (
    target.closest(".app__content") !== null && target.closest(".details, .dview__page") === null
  );
}

function isTypingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return (
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLSelectElement ||
    target.isContentEditable
  );
}

export function App() {
  const [adding, setAdding] = createSignal(false);
  const [contentWidth, setContentWidth] = createSignal(0);
  const [breakpoint, setBreakpoint] = createSignal(DEFAULT_SPLIT_BREAKPOINT);
  const [sizes, setSizes] = createSignal(DEFAULT_SIZES);
  let keywordFilter: KeywordFilterHandle | undefined;
  let commandBar: CommandBarHandle | undefined;

  // Wide window: the panel can dock beside the library; narrow: it overlays it as a flyout.
  const split = () => isSplitWidth(contentWidth(), breakpoint());
  const docked = () => isPanelView() && split() && !store.panelFloating();
  /** Dock/unpin only makes sense where the window is wide enough to dock. */
  const pin = (): PanelPin => (docked() ? "docked" : split() ? "floating" : null);
  const panelWidth = () =>
    clampWidth(store.panelWidth() ?? sizes().panel, sizes().panelMin, contentWidth());
  const listWidth = () =>
    clampWidth(store.listWidth() ?? sizes().list, sizes().listMin, contentWidth());

  function observeContent(element: HTMLElement) {
    setBreakpoint(readSplitBreakpoint(element));
    setSizes({
      panel: readCssPx(element, "--qk-details-width", DEFAULT_SIZES.panel),
      panelMin: readCssPx(element, "--qk-details-min-width", DEFAULT_SIZES.panelMin),
      list: readCssPx(element, "--qk-dview-list-width", DEFAULT_SIZES.list),
      listMin: readCssPx(element, "--qk-dview-list-min-width", DEFAULT_SIZES.listMin),
    });
    const observer = new ResizeObserver((entries) => {
      const rect = entries[0]?.contentRect;
      if (rect) setContentWidth(rect.width);
    });
    observer.observe(element);
    onCleanup(() => observer.disconnect());
  }

  function onKeyDown(event: KeyboardEvent) {
    if (adding()) return;
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
      event.preventDefault();
      commandBar?.focus();
      return;
    }
    if (store.page() !== "library") return;
    const typing = isTypingTarget(event.target);
    const wantsFilter =
      (event.ctrlKey && event.key.toLowerCase() === "f") ||
      (event.key === "/" && !event.ctrlKey && !event.metaKey && !typing);
    if (wantsFilter) {
      event.preventDefault();
      keywordFilter?.open();
    } else if (
      isTypeToFilter(event) &&
      !typing &&
      !store.editingViews() &&
      inLibrary(event.target)
    ) {
      // Typing over the games starts the filter with that character.
      event.preventDefault();
      keywordFilter?.open(event.key);
    } else if (event.key === "Escape" && store.viewMode() !== "details" && store.detailsOpen()) {
      store.closeDetails();
    }
  }

  onMount(() => {
    let cleanup: (() => void) | undefined;
    let disposed = false;
    void store.init().then((fn) => {
      if (disposed) fn();
      else cleanup = fn;
    });

    window.addEventListener("keydown", onKeyDown);

    onCleanup(() => {
      disposed = true;
      cleanup?.();
      window.removeEventListener("keydown", onKeyDown);
    });
  });

  return (
    <div class="app">
      <TitleBar onAdd={() => setAdding(true)} commandRef={(handle) => (commandBar = handle)} />
      <Sidebar />
      <div class="app__main">
        {/* Stays mounted (just hidden) on other pages so the filter box keeps its text. */}
        <div hidden={store.page() !== "library"}>
          <TopBar keywordRef={(handle) => (keywordFilter = handle)} />
          <FilterPills />
          <Show when={store.setup()?.showIgdbHint}>
            <IgdbHint />
          </Show>
        </div>
        <Show
          when={store.page() === "library"}
          fallback={
            <div class="app__content">
              <Show when={store.page() === "home"} fallback={<SettingsPage />}>
                <HomePage />
              </Show>
            </div>
          }
        >
          <div
            class="app__content"
            ref={observeContent}
            style={{ "--qk-panel-width": `${panelWidth()}px` }}
          >
            <Show
              when={isPanelView()}
              fallback={
                <DetailsView
                  listWidth={listWidth()}
                  minListWidth={sizes().listMin}
                  maxListWidth={Math.max(sizes().listMin, Math.floor(contentWidth() / 2))}
                />
              }
            >
              <Show when={store.viewMode() === "list"} fallback={<GameGrid />}>
                <GameList />
              </Show>
            </Show>
            <Show when={docked()}>
              <Splitter
                label="Details panel width"
                value={panelWidth()}
                min={sizes().panelMin}
                max={Math.max(sizes().panelMin, Math.floor(contentWidth() / 2))}
                grows="left"
                onChange={(width) => store.setPanelWidth(width)}
              />
              <Show
                when={store.isMulti()}
                fallback={
                  <Show when={store.selectedGame()} fallback={<EmptyDetails />}>
                    {(game) => (
                      <DetailsPanel
                        game={game()}
                        split
                        pin="docked"
                        onClose={() => store.closeDetails()}
                      />
                    )}
                  </Show>
                }
              >
                <MultiSelectPanel
                  games={store.selectedGames()}
                  split
                  pin="docked"
                  onClose={() => store.clearSelection()}
                />
              </Show>
            </Show>
            <Show when={isPanelView() && !docked() && store.isMulti()}>
              <MultiSelectPanel
                games={store.selectedGames()}
                pin={pin()}
                onClose={() => store.clearSelection()}
              />
            </Show>
            <Show
              when={
                isPanelView() && !docked() && store.detailsOpen() && !store.isMulti()
                  ? store.selectedGame()
                  : undefined
              }
            >
              {(game) => (
                <DetailsPanel game={game()} pin={pin()} onClose={() => store.closeDetails()} />
              )}
            </Show>
          </div>
        </Show>
      </div>
      <Show when={adding()}>
        <AddGameDialog onClose={() => setAdding(false)} />
      </Show>
      <Show when={store.updateOpen() && store.update()}>
        {(update) => <UpdateDialog update={update()} onClose={() => store.setUpdateOpen(false)} />}
      </Show>
      <Show when={store.duplicatesOpen()}>
        <DuplicatesDialog onClose={() => store.setDuplicatesOpen(false)} />
      </Show>
      <Show when={store.contextMenu()}>
        {(open) => (
          <Show
            when={open().games.length > 1}
            fallback={
              <GameContextMenu
                game={open().game}
                anchor={open().anchor}
                onDone={(returnFocus) => store.closeContextMenu(returnFocus)}
              />
            }
          >
            <MultiContextMenu
              games={open().games}
              anchor={open().anchor}
              onDone={(returnFocus) => store.closeContextMenu(returnFocus)}
            />
          </Show>
        )}
      </Show>
      <Show when={store.combining()}>
        {(ids) => <CombineDialog ids={[...ids()]} onClose={() => store.closeCombine()} />}
      </Show>
      <Show when={store.setup()?.showSetup}>
        <FirstRunDialog />
      </Show>
      <Toasts />
    </div>
  );
}
