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
import { launchWithToast } from "../lib/actions";
import { isNavKey, nextIndex } from "../lib/gridNav";
import { store } from "../lib/store";
import { rowRange, rowTop, scrollTopForRow, visibleRows, type RowLayout } from "../lib/virtualRows";
import { GameTile, tileDomId } from "./GameTile";
import { Logo } from "./Logo";

const FALLBACK_WIDTH = 1000;
const FALLBACK_HEIGHT = 700;
const OVERSCAN_ROWS = 3;

interface Metrics {
  minWidth: number;
  gap: number;
  padding: number;
  titleHeight: number;
  ratioHeight: number;
}

const DEFAULT_METRICS: Metrics = {
  minWidth: 170,
  gap: 16,
  padding: 20,
  titleHeight: 36,
  ratioHeight: 1.5,
};

function readMetrics(element: Element): Metrics {
  const style = getComputedStyle(element);
  const read = (name: string, fallback: number) =>
    Number.parseFloat(style.getPropertyValue(name)) || fallback;
  return {
    minWidth: read("--qk-tile-min-width", DEFAULT_METRICS.minWidth),
    gap: read("--qk-grid-gap", DEFAULT_METRICS.gap),
    padding: read("--qk-grid-padding", DEFAULT_METRICS.padding),
    titleHeight: read("--qk-tile-title-height", DEFAULT_METRICS.titleHeight),
    ratioHeight: read("--qk-tile-ratio-height", DEFAULT_METRICS.ratioHeight),
  };
}

function activeDescendant(): string | undefined {
  const game = store.selectedGame();
  return game && store.selectedIndex() >= 0 ? tileDomId(game.id) : undefined;
}

function launchSelected() {
  const game = store.selectedGame();
  if (game) void launchWithToast(game.id);
}

/**
 * Cover grid virtualized by rows. Rows have a fixed height derived from the container width, so
 * the visible window is plain arithmetic (see `lib/virtualRows.ts`). One focusable region with
 * `aria-activedescendant` for keyboard navigation.
 */
export function GameGrid() {
  let scroller: HTMLDivElement | undefined;
  const [size, setSize] = createSignal({ width: 0, height: 0 });
  const [scrollTop, setScrollTop] = createSignal(0);
  const [metrics, setMetrics] = createSignal(DEFAULT_METRICS);

  const width = () => size().width || FALLBACK_WIDTH;
  const height = () => size().height || FALLBACK_HEIGHT;
  const columns = createMemo(() => {
    const { minWidth, gap, padding } = metrics();
    const usable = width() - padding * 2;
    return Math.max(1, Math.floor((usable + gap) / (minWidth + gap)));
  });
  const rowHeight = createMemo(() => {
    const m = metrics();
    const usable = width() - m.padding * 2;
    const tileWidth = (usable - m.gap * (columns() - 1)) / columns();
    return Math.round(tileWidth * m.ratioHeight + m.titleHeight + m.gap);
  });
  const layout = createMemo<RowLayout>(() => ({
    rowHeight: rowHeight(),
    rowCount: Math.ceil(store.games().length / columns()),
    paddingTop: metrics().padding,
  }));
  const totalHeight = () => layout().rowCount * layout().rowHeight + metrics().padding * 2;
  // Primitive row indices: <For> keeps each row's DOM while it stays in the window.
  const rows = createMemo(() =>
    rowRange(visibleRows(scrollTop(), height(), layout(), OVERSCAN_ROWS)),
  );

  onMount(() => {
    const element = scroller;
    if (!element) return;
    setMetrics(readMetrics(element));
    const observer = new ResizeObserver((entries) => {
      const rect = entries[0]?.contentRect;
      if (rect) setSize({ width: rect.width, height: rect.height });
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
    onCleanup(() => {
      observer.disconnect();
      element.removeEventListener("scroll", onScroll);
      cancelAnimationFrame(frame);
    });
  });

  // Keep the selected tile in view (keyboard navigation, selection after search).
  createEffect(
    on(
      () => [store.selectedIndex(), columns(), rowHeight()] as const,
      ([index, cols]) => {
        if (index < 0 || !scroller) return;
        const target = scrollTopForRow(
          Math.floor(index / cols),
          scroller.scrollTop,
          height(),
          layout(),
        );
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

  const rowGames = (row: number) => {
    const cols = columns();
    return store.games().slice(row * cols, (row + 1) * cols);
  };

  function onKeyDown(event: KeyboardEvent) {
    if (isNavKey(event.key) && !event.altKey && !event.ctrlKey && !event.metaKey) {
      event.preventDefault();
      const games = store.games();
      const pageRows = Math.max(1, Math.floor(height() / rowHeight()));
      const next = nextIndex(store.selectedIndex(), event.key, columns(), games.length, pageRows);
      const game = games[next];
      if (game) store.selectGame(game.id);
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      if (event.ctrlKey || event.metaKey) launchSelected();
      else store.openDetails();
    } else if (event.key === "F5") {
      event.preventDefault();
      launchSelected();
    }
  }

  function onSelect(id: string) {
    store.selectAndOpen(id);
    scroller?.focus({ preventScroll: true });
  }

  return (
    <div
      ref={(el) => (scroller = el)}
      class="grid"
      role="grid"
      tabindex="0"
      aria-label="Games"
      aria-rowcount={layout().rowCount}
      aria-colcount={columns()}
      aria-activedescendant={activeDescendant()}
      onKeyDown={onKeyDown}
    >
      <div class="grid__canvas" style={{ height: `${totalHeight()}px` }}>
        <For each={rows()}>
          {(row) => (
            <div
              role="row"
              class="grid__row"
              aria-rowindex={row + 1}
              style={{
                transform: `translateY(${rowTop(row, layout())}px)`,
                height: `${rowHeight()}px`,
                "--qk-columns": columns(),
              }}
            >
              <For each={rowGames(row)}>
                {(game) => (
                  <GameTile
                    game={game}
                    selected={game.id === store.selectedId()}
                    onSelect={onSelect}
                    onLaunch={(id) => void launchWithToast(id)}
                  />
                )}
              </For>
            </div>
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
  );
}
