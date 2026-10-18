import { createSignal, onCleanup, onMount, Show } from "solid-js";
import { AddGameDialog } from "./components/AddGameDialog";
import { DetailsPanel } from "./components/DetailsPanel";
import { GameGrid } from "./components/GameGrid";
import { GameList } from "./components/GameList";
import { SettingsDialog } from "./components/SettingsDialog";
import { Sidebar } from "./components/Sidebar";
import { Toasts } from "./components/Toasts";
import { TopBar } from "./components/TopBar";
import { store } from "./lib/store";

type DialogKind = "add" | "settings" | null;

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
  const [dialog, setDialog] = createSignal<DialogKind>(null);
  let searchInput: HTMLInputElement | undefined;

  function onKeyDown(event: KeyboardEvent) {
    if (dialog() !== null) return;
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
      <Sidebar onAdd={() => setDialog("add")} onSettings={() => setDialog("settings")} />
      <div class="app__main">
        <TopBar inputRef={(el) => (searchInput = el)} />
        <div class="app__content">
          <Show when={store.viewMode() === "list"} fallback={<GameGrid />}>
            <GameList />
          </Show>
          <Show when={store.detailsOpen() ? store.selectedGame() : undefined}>
            {(game) => <DetailsPanel game={game()} onClose={() => store.closeDetails()} />}
          </Show>
        </div>
      </div>
      <Show when={dialog() === "add"}>
        <AddGameDialog onClose={() => setDialog(null)} />
      </Show>
      <Show when={dialog() === "settings"}>
        <SettingsDialog onClose={() => setDialog(null)} />
      </Show>
      <Toasts />
    </div>
  );
}
