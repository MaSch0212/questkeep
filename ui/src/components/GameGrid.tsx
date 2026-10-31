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
import { isNavKey } from "../lib/gridNav";
import {
  asGamesRow,
  asHeaderRow,
  type HeaderRow,
  layoutGroups,
  navigateSlots,
  rowSlots,
  scrollTopForRow,
  slotKey,
  visibleRowRange,
} from "../lib/grouping";
import { createSlotCursor } from "../lib/slotCursor";
import { store } from "../lib/store";
import { GameTile, tileDomId } from "./GameTile";
import { GroupHeader } from "./GroupHeader";
import { StickyGroupHeader } from "./StickyGroupHeader";
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
  headerHeight: number;
}

const DEFAULT_METRICS: Metrics = {
  minWidth: 170,
  gap: 16,
  padding: 20,
  titleHeight: 36,
  ratioHeight: 1.5,
  headerHeight: 40,
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
    headerHeight: read("--qk-group-header-height", DEFAULT_METRICS.headerHeight),
  };
}

function activateSelected() {
  const game = store.selectedGame();
  if (game) void activateGame(game.id);
}

/**
 * Cover grid virtualized by rows. Rows have a fixed height derived from the container width
 * (group headers their own), so the visible window is plain arithmetic (see `lib/grouping.ts`).
 * In a grouped view a game shows once per group it belongs to. One focusable region with
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
  const layout = createMemo(() =>
    layoutGroups(store.groups(), {
      columns: columns(),
      rowHeight: rowHeight(),
      headerHeight: metrics().headerHeight,
      padding: metrics().padding,
      headers: store.groupBy() !== "none",
      collapsed: store.collapsedGroups(),
    }),
  );
  const { cursor, place, moveTo } = createSlotCursor(layout);
  // Primitive row indices: <For> keeps each row's DOM while it stays in the window.
  const rows = createMemo(() => {
    const { start, end } = visibleRowRange(layout(), scrollTop(), height(), OVERSCAN_ROWS);
    return Array.from({ length: end - start }, (_, i) => start + i);
  });
  const activeDescendant = () => {
    const slot = layout().slots[cursor()];
    return slot ? tileDomId(slotKey(slot)) : undefined;
  };

  onMount(() => {
    const element = scroller;
    if (!element) return;
    setMetrics(readMetrics(element));
    // Coming back from another page: restore the scroll position.
    const saved = store.libraryScroll("grid");
    if (saved > 0) {
      element.scrollTop = saved;
      setScrollTop(saved);
    }
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
      store.rememberLibraryScroll("grid", element.scrollTop);
      observer.disconnect();
      element.removeEventListener("scroll", onScroll);
      cancelAnimationFrame(frame);
    });
  });

  // Keep the selected tile in view (keyboard navigation, selection after search).
  createEffect(
    on(
      place,
      () => {
        const current = layout();
        const slot = current.slots[cursor()];
        if (!slot || !scroller) return;
        const target = scrollTopForRow(
          current,
          slot.row,
          scroller.scrollTop,
          height(),
          metrics().padding,
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

  function onKeyDown(event: KeyboardEvent) {
    if (isNavKey(event.key) && !event.altKey && !event.ctrlKey && !event.metaKey) {
      event.preventDefault();
      const pageRows = Math.max(1, Math.floor(height() / rowHeight()));
      moveTo(navigateSlots(layout(), cursor(), event.key, pageRows), store.selectGame);
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

  /** Collapsing from the pinned header keeps that header at the top. */
  function toggleFromTop(row: HeaderRow) {
    store.toggleGroup(row.group.key);
    if (scroller) {
      scroller.scrollTop = row.top;
      setScrollTop(row.top);
    }
  }

  function onSelect(slot: number) {
    moveTo(slot, store.selectAndOpen);
    scroller?.focus({ preventScroll: true });
  }

  return (
    <div
      ref={(el) => (scroller = el)}
      class="grid"
      role="grid"
      tabindex="0"
      aria-label="Games"
      aria-rowcount={layout().rows.length}
      aria-colcount={columns()}
      aria-activedescendant={activeDescendant()}
      onKeyDown={onKeyDown}
    >
      <StickyGroupHeader
        layout={layout()}
        scrollTop={scrollTop()}
        variant="grid"
        onToggle={toggleFromTop}
      />
      <div class="grid__canvas" style={{ height: `${layout().height}px` }}>
        <For each={rows()}>
          {(index) => {
            const row = () => layout().rows[index];
            return (
              <Show when={row()}>
                {(current) => (
                  <div
                    role="row"
                    class="grid__row"
                    classList={{ "grid__row--header": current().kind === "header" }}
                    aria-rowindex={index + 1}
                    style={{
                      transform: `translateY(${current().top}px)`,
                      height: `${current().height}px`,
                      "--qk-columns": columns(),
                    }}
                  >
                    <Show when={asHeaderRow(current())}>
                      {(header) => (
                        <div role="rowheader" class="grid__group">
                          <GroupHeader
                            group={header().group}
                            collapsed={header().collapsed}
                            onToggle={() => store.toggleGroup(header().group.key)}
                          />
                        </div>
                      )}
                    </Show>
                    <Show when={asGamesRow(current())}>
                      {(games) => (
                        <For each={rowSlots(games())}>
                          {(slotIndex) => (
                            <Show when={layout().slots[slotIndex]}>
                              {(slot) => (
                                <GameTile
                                  game={slot().game}
                                  domKey={slotKey(slot())}
                                  selected={slot().game.id === store.selectedId()}
                                  open={
                                    store.detailsOpen() && slot().game.id === store.selectedId()
                                  }
                                  onSelect={() => onSelect(slotIndex)}
                                  onLaunch={(id) => void activateGame(id)}
                                />
                              )}
                            </Show>
                          )}
                        </For>
                      )}
                    </Show>
                  </div>
                )}
              </Show>
            );
          }}
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
