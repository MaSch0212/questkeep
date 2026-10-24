import { createSignal, onCleanup, onMount, Show } from "solid-js";
import { DEFAULT_SPLIT_BREAKPOINT, isSplitWidth, readSplitBreakpoint } from "./lib/layoutMode";
import { AddGameDialog } from "./components/AddGameDialog";
import { DetailsPanel } from "./components/DetailsPanel";
import { GameGrid } from "./components/GameGrid";
import { GameList } from "./components/GameList";
import { SettingsPage } from "./components/settings/SettingsPage";
import { Sidebar } from "./components/Sidebar";
import { TitleBar } from "./components/TitleBar";
import { Toasts } from "./components/Toasts";
import { TopBar } from "./components/TopBar";
import { store } from "./lib/store";

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
  let searchInput: HTMLInputElement | undefined;

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
    if (adding() || store.page() !== "library") return;
    const wantsSearch =
      (event.ctrlKey && event.key.toLowerCase() === "f") ||
      (event.key === "/" && !event.ctrlKey && !event.metaKey && !isTypingTarget(event.target));
    if (wantsSearch) {
      event.preventDefault();
      searchInput?.focus();
      searchInput?.select();
    } else if (event.key === "Escape" && store.detailsOpen()) {
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
      <TitleBar />
      <Sidebar onAdd={() => setAdding(true)} />
      <div class="app__main">
        {/* Stays mounted (just hidden) on other pages so the search box keeps its text. */}
        <div hidden={store.page() !== "library"}>
          <TopBar inputRef={(el) => (searchInput = el)} />
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
            <Show when={store.viewMode() === "list"} fallback={<GameGrid />}>
              <GameList />
            </Show>
            <Show when={store.detailsOpen() ? store.selectedGame() : undefined}>
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
      <Toasts />
    </div>
  );
}
