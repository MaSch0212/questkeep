import {
  createEffect,
  createMemo,
  createSignal,
  For,
  on,
  onCleanup,
  onMount,
  Show,
} from "solid-js";
import { activateGame } from "../lib/actions";
import { isNavKey, nextIndex } from "../lib/gridNav";
import { store } from "../lib/store";
import { rowRange, rowTop, scrollTopForRow, visibleRows, type RowLayout } from "../lib/virtualRows";
import { GameRow, rowDomId } from "./GameRow";
import { Logo } from "./Logo";

const FALLBACK_HEIGHT = 700;
const DEFAULT_ROW_HEIGHT = 52;
const DEFAULT_PADDING = 8;
const OVERSCAN_ROWS = 6;

function readNumber(element: Element, name: string, fallback: number): number {
  return Number.parseFloat(getComputedStyle(element).getPropertyValue(name)) || fallback;
}

function activeDescendant(): string | undefined {
  const game = store.selectedGame();
  return game && store.selectedIndex() >= 0 ? rowDomId(game.id) : undefined;
}

function activateSelected() {
  const game = store.selectedGame();
  if (game) void activateGame(game.id);
}

/**
 * List view of the library: one fixed-height row per game, virtualized with the same row maths
 * as the cover grid (`lib/virtualRows.ts`), so 5000+ games scroll smoothly. One focusable listbox
 * with `aria-activedescendant` for keyboard navigation.
 */
export function GameList() {
  let scroller: HTMLDivElement | undefined;
  const [height, setHeight] = createSignal(FALLBACK_HEIGHT);
  const [scrollTop, setScrollTop] = createSignal(0);
  const [rowHeight, setRowHeight] = createSignal(DEFAULT_ROW_HEIGHT);
  const [padding, setPadding] = createSignal(DEFAULT_PADDING);

  const layout = createMemo<RowLayout>(() => ({
    rowHeight: rowHeight(),
    rowCount: store.games().length,
    paddingTop: padding(),
  }));
  const totalHeight = () => layout().rowCount * rowHeight() + padding() * 2;
  // Primitive row indices: <For> keeps each row's DOM while it stays in the window.
  const rows = createMemo(() =>
    rowRange(visibleRows(scrollTop(), height(), layout(), OVERSCAN_ROWS)),
  );

  onMount(() => {
    const element = scroller;
    if (!element) return;
    setRowHeight(readNumber(element, "--qk-list-row-height", DEFAULT_ROW_HEIGHT));
    setPadding(readNumber(element, "--qk-list-padding", DEFAULT_PADDING));
    const observer = new ResizeObserver((entries) => {
      const rect = entries[0]?.contentRect;
      if (rect) setHeight(rect.height || FALLBACK_HEIGHT);
    });
    observer.observe(element);
    let frame = 0;
    const onScroll = () => {
      if (frame) return;
      frame = requestAnimationFrame(() => {
        frame = 0;
        setScrollTop(element.scrollTop);
      });
    };
    element.addEventListener("scroll", onScroll, { passive: true });
    // Coming from the grid view: bring the selected game into view.
    if (store.selectedIndex() >= 0) {
      const target = scrollTopForRow(store.selectedIndex(), 0, height(), layout());
      element.scrollTop = target;
      setScrollTop(target);
    }
    onCleanup(() => {
      observer.disconnect();
      element.removeEventListener("scroll", onScroll);
      cancelAnimationFrame(frame);
    });
  });

  // Keep the selected row in view (keyboard navigation, selection after search).
  createEffect(
    on(
      () => store.selectedIndex(),
      (index) => {
        if (index < 0 || !scroller) return;
        const target = scrollTopForRow(index, scroller.scrollTop, height(), layout());
        if (target !== scroller.scrollTop) {
          scroller.scrollTop = target;
          setScrollTop(target);
        }
      },
      { defer: true },
    ),
  );

  // A new result list starts at the top (unless the selection pulls the view elsewhere).
  createEffect(
    on(
      () => store.query(),
      () => {
        if (scroller && store.selectedIndex() < 0) {
          scroller.scrollTop = 0;
          setScrollTop(0);
        }
      },
      { defer: true },
    ),
  );

  function onKeyDown(event: KeyboardEvent) {
    if (isNavKey(event.key) && !event.altKey && !event.ctrlKey && !event.metaKey) {
      event.preventDefault();
      const games = store.games();
      const pageRows = Math.max(1, Math.floor(height() / rowHeight()));
      const game = games[nextIndex(store.selectedIndex(), event.key, 1, games.length, pageRows)];
      if (game) store.selectGame(game.id);
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      if (event.ctrlKey || event.metaKey) activateSelected();
      else store.openDetails();
    } else if (event.key === "F5") {
      event.preventDefault();
      activateSelected();
    }
  }

  function onSelect(id: string) {
    store.selectAndOpen(id);
    scroller?.focus({ preventScroll: true });
  }

  return (
    <div class="list">
      <div class="list__header" aria-hidden="true">
        <span />
        <span>Name</span>
        <span>Source</span>
        <span>Status</span>
        <span>Playtime</span>
        <span>Last played</span>
        <span class="list-row__year">Released</span>
      </div>
      <div
        ref={(el) => (scroller = el)}
        class="list__scroller"
        role="listbox"
        tabindex="0"
        aria-label="Games"
        aria-activedescendant={activeDescendant()}
        onKeyDown={onKeyDown}
      >
        <div class="list__canvas" style={{ height: `${totalHeight()}px` }}>
          <For each={rows()}>
            {(index) => (
              <Show when={store.games()[index]}>
                {(game) => (
                  <div
                    class="list__slot"
                    style={{
                      transform: `translateY(${rowTop(index, layout())}px)`,
                      height: `${rowHeight()}px`,
                    }}
                  >
                    <GameRow
                      game={game()}
                      selected={game().id === store.selectedId()}
                      onSelect={onSelect}
                      onLaunch={(id) => void activateGame(id)}
                    />
                  </div>
                )}
              </Show>
            )}
          </For>
        </div>
        <Show when={store.games().length === 0 && !store.loading()}>
          <div class="grid__empty">
            <Logo class="logo logo--muted" />
            <p>No games to show. Add a game, or scan a store from Settings.</p>
          </div>
        </Show>
      </div>
    </div>
  );
}
