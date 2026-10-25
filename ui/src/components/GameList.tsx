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
  layoutGroups,
  navigateSlots,
  scrollTopForRow,
  slotKey,
  visibleRowRange,
} from "../lib/grouping";
import { createSlotCursor } from "../lib/slotCursor";
import { store } from "../lib/store";
import { GameRow, rowDomId } from "./GameRow";
import { GameRowCompact } from "./GameRowCompact";
import { GroupHeader } from "./GroupHeader";
import { Logo } from "./Logo";

const FALLBACK_HEIGHT = 700;
const DEFAULT_ROW_HEIGHT = 36;
const DEFAULT_COMPACT_ROW_HEIGHT = 32;
const DEFAULT_HEADER_HEIGHT = 32;
const DEFAULT_PADDING = 8;
const OVERSCAN_ROWS = 6;

function readNumber(element: Element, name: string, fallback: number): number {
  return Number.parseFloat(getComputedStyle(element).getPropertyValue(name)) || fallback;
}

function activateSelected() {
  const game = store.selectedGame();
  if (game) void activateGame(game.id);
}

/**
 * List view of the library: one fixed-height row per game (and per group header when the view
 * is grouped), virtualized with the same row maths as the cover grid (`lib/grouping.ts`), so
 * 5000+ games scroll smoothly. One focusable listbox with `aria-activedescendant` for keyboard
 * navigation.
 *
 * `compact` is the narrow list of the details view: no column header, smaller rows, and selecting
 * a game shows it on the page beside the list instead of opening the details panel.
 */
export function GameList(props: { compact?: boolean }) {
  const mode = () => (props.compact ? "details" : "list");
  let scroller: HTMLDivElement | undefined;
  const [height, setHeight] = createSignal(FALLBACK_HEIGHT);
  const [scrollTop, setScrollTop] = createSignal(0);
  const [rowHeight, setRowHeight] = createSignal(
    props.compact ? DEFAULT_COMPACT_ROW_HEIGHT : DEFAULT_ROW_HEIGHT,
  );
  const [headerHeight, setHeaderHeight] = createSignal(DEFAULT_HEADER_HEIGHT);
  const [padding, setPadding] = createSignal(DEFAULT_PADDING);

  const layout = createMemo(() =>
    layoutGroups(store.groups(), {
      columns: 1,
      rowHeight: rowHeight(),
      headerHeight: headerHeight(),
      padding: padding(),
      headers: store.groupBy() !== "none",
      collapsed: store.collapsedGroups(),
    }),
  );
  const { cursor, moveTo } = createSlotCursor(layout);
  // Primitive row indices: <For> keeps each row's DOM while it stays in the window.
  const rows = createMemo(() => {
    const { start, end } = visibleRowRange(layout(), scrollTop(), height(), OVERSCAN_ROWS);
    return Array.from({ length: end - start }, (_, i) => start + i);
  });
  const activeDescendant = () => {
    const slot = layout().slots[cursor()];
    return slot ? rowDomId(slotKey(slot)) : undefined;
  };

  function scrollToCursor(from: number): number {
    const slot = layout().slots[cursor()];
    return slot ? scrollTopForRow(layout(), slot.row, from, height(), padding()) : from;
  }

  onMount(() => {
    const element = scroller;
    if (!element) return;
    setRowHeight(
      props.compact
        ? readNumber(element, "--qk-compact-row-height", DEFAULT_COMPACT_ROW_HEIGHT)
        : readNumber(element, "--qk-list-row-height", DEFAULT_ROW_HEIGHT),
    );
    setHeaderHeight(readNumber(element, "--qk-list-group-height", DEFAULT_HEADER_HEIGHT));
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
    // Coming from the grid view or another page: restore the scroll position, then bring the
    // selected game into view.
    const target = scrollToCursor(store.libraryScroll(mode()));
    if (target > 0) {
      element.scrollTop = target;
      setScrollTop(target);
    }
    onCleanup(() => {
      store.rememberLibraryScroll(mode(), element.scrollTop);
      observer.disconnect();
      element.removeEventListener("scroll", onScroll);
      cancelAnimationFrame(frame);
    });
  });

  // Keep the selected row in view (keyboard navigation, selection after search).
  createEffect(
    on(
      () => [cursor(), layout()] as const,
      () => {
        if (!scroller) return;
        const target = scrollToCursor(scroller.scrollTop);
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
      else if (!props.compact) store.openDetails();
    } else if (event.key === "F5") {
      event.preventDefault();
      activateSelected();
    }
  }

  function onSelect(slot: number) {
    moveTo(slot, props.compact ? store.selectGame : store.selectAndOpen);
    scroller?.focus({ preventScroll: true });
  }

  return (
    <div class="list" classList={{ "list--compact": props.compact === true }}>
      <Show when={!props.compact}>
        <div class="list__header" aria-hidden="true">
          <span />
          <span>Name</span>
          <span>Source</span>
          <span>Status</span>
          <span>Playtime</span>
          <span>Last played</span>
          <span class="list-row__year">Released</span>
        </div>
      </Show>
      <div
        ref={(el) => (scroller = el)}
        class="list__scroller"
        role="listbox"
        tabindex="0"
        aria-label="Games"
        aria-activedescendant={activeDescendant()}
        onKeyDown={onKeyDown}
      >
        <div class="list__canvas" style={{ height: `${layout().height}px` }}>
          <For each={rows()}>
            {(index) => {
              const row = () => layout().rows[index];
              return (
                <Show when={row()}>
                  {(current) => (
                    <div
                      class="list__slot"
                      style={{
                        transform: `translateY(${current().top}px)`,
                        height: `${current().height}px`,
                      }}
                    >
                      <Show when={asHeaderRow(current())}>
                        {(header) => (
                          <div class="list__group" role="presentation">
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
                          <Show when={layout().slots[games().first]}>
                            {(slot) => (
                              <Show
                                when={!props.compact}
                                fallback={
                                  <GameRowCompact
                                    game={slot().game}
                                    domKey={slotKey(slot())}
                                    selected={slot().game.id === store.selectedId()}
                                    onSelect={() => onSelect(games().first)}
                                  />
                                }
                              >
                                <GameRow
                                  game={slot().game}
                                  domKey={slotKey(slot())}
                                  selected={slot().game.id === store.selectedId()}
                                  open={
                                    store.detailsOpen() && slot().game.id === store.selectedId()
                                  }
                                  onSelect={() => onSelect(games().first)}
                                  onLaunch={(id) => void activateGame(id)}
                                />
                              </Show>
                            )}
                          </Show>
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
    </div>
  );
}
