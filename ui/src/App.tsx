import { createSignal, onCleanup, onMount, Show } from "solid-js";
import { DEFAULT_SPLIT_BREAKPOINT, isSplitWidth, readSplitBreakpoint } from "./lib/layoutMode";
import { AddGameDialog } from "./components/AddGameDialog";
import { DuplicatesDialog } from "./components/DuplicatesDialog";
import { DetailsPanel } from "./components/DetailsPanel";
import { DetailsView } from "./components/DetailsView";
import { GameGrid } from "./components/GameGrid";
import { GameList } from "./components/GameList";
import { SettingsPage } from "./components/settings/SettingsPage";
import { Sidebar } from "./components/Sidebar";
import { TitleBar } from "./components/TitleBar";
import { Toasts } from "./components/Toasts";
import { TopBar } from "./components/TopBar";
import type { CommandBarHandle } from "./components/CommandBar";
import type { KeywordFilterHandle } from "./components/KeywordFilter";
import { store } from "./lib/store";

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
  let keywordFilter: KeywordFilterHandle | undefined;
  let commandBar: CommandBarHandle | undefined;

  // Wide window: details sit beside the library; narrow: they overlay it as a flyout.
  const split = () => isSplitWidth(contentWidth(), breakpoint());

  function observeContent(element: HTMLElement) {
    setBreakpoint(readSplitBreakpoint(element));
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
        </div>
        <Show
          when={store.page() === "library"}
          fallback={
            <div class="app__content">
              <SettingsPage />
            </div>
          }
        >
          <div class="app__content" ref={observeContent}>
            <Show when={store.viewMode() !== "details"} fallback={<DetailsView />}>
              <Show when={store.viewMode() === "list"} fallback={<GameGrid />}>
                <GameList />
              </Show>
            </Show>
            <Show
              when={
                store.viewMode() !== "details" && store.detailsOpen()
                  ? store.selectedGame()
                  : undefined
              }
            >
              {(game) => (
                <DetailsPanel game={game()} split={split()} onClose={() => store.closeDetails()} />
              )}
            </Show>
          </div>
        </Show>
      </div>
      <Show when={adding()}>
        <AddGameDialog onClose={() => setAdding(false)} />
      </Show>
      <Show when={store.duplicatesOpen()}>
        <DuplicatesDialog onClose={() => store.setDuplicatesOpen(false)} />
      </Show>
      <Toasts />
    </div>
  );
}
