import { translator } from "../i18n";
import { libraryList } from "../i18n/libraryList";
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
  scrollTopForRow,
  slotKey,
  visibleRowRange,
} from "../lib/grouping";
import { createSlotCursor } from "../lib/slotCursor";
import { followGroupJumps } from "../lib/groupJump";
import { clickSelect, contextSelect, handleGameKeys, handleSelectionKeys } from "../lib/gameKeys";
import { store } from "../lib/store";
import { GameRow, rowDomId } from "./GameRow";
import { GameRowCompact } from "./GameRowCompact";
import { GroupHeader } from "./GroupHeader";
import { StickyGroupHeader } from "./StickyGroupHeader";
import { TableHeader } from "./TableHeader";
import { columnsMinWidth, columnTemplate, shownColumns } from "../lib/columns";
import { Logo } from "./Logo";

const tr = translator(libraryList);

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
  let columnHeader: HTMLDivElement | undefined;
  const shown = createMemo(() => shownColumns(store.columns()));
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
  const { cursor, place, moveTo } = createSlotCursor(layout);
  followGroupJumps({ layout, scroller: () => scroller, setScrollTop, moveTo });
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
      // The header follows the table sideways right away; rows only need the frame's top.
      if (columnHeader) columnHeader.scrollLeft = element.scrollLeft;
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
      place,
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
    const id = activeDescendant();
    if (handleGameKeys(event, store.selectedGame(), id ? document.getElementById(id) : null))
      return;
    const pageRows = Math.max(1, Math.floor(height() / rowHeight()));
    const target = () =>
      isNavKey(event.key) ? navigateSlots(layout(), cursor(), event.key, pageRows) : cursor();
    if (handleSelectionKeys(event, layout(), moveTo, target, isNavKey(event.key))) return;
    if (isNavKey(event.key) && !event.altKey && !event.ctrlKey && !event.metaKey) {
      event.preventDefault();
      moveTo(target(), store.selectGame);
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

  /** Collapsing from the pinned header keeps that header at the top. */
  function toggleFromTop(row: HeaderRow) {
    store.toggleGroup(row.group.key);
    if (scroller) {
      scroller.scrollTop = row.top;
      setScrollTop(row.top);
    }
  }

  /** Right-click selects that game (a multi-selection stays), then opens its menu at the pointer. */
  function onContextMenu(slot: number, event: MouseEvent) {
    contextSelect(slot, layout(), moveTo);
    const game = layout().slots[slot]?.game;
    if (game) store.openContextMenu(game.id, { x: event.clientX, y: event.clientY });
  }

  function onSelect(slot: number, event: MouseEvent) {
    clickSelect(
      event,
      slot,
      layout(),
      moveTo,
      props.compact ? store.selectGame : store.selectAndOpen,
    );
    scroller?.focus({ preventScroll: true });
  }

  return (
    <div
      class="list"
      classList={{ "list--compact": props.compact === true }}
      style={
        props.compact
          ? undefined
          : {
              "--qk-list-columns": columnTemplate(shown()),
              "--qk-list-min-width": columnsMinWidth(shown()),
            }
      }
    >
      <Show when={!props.compact}>
        <TableHeader columns={shown()} ref={(el) => (columnHeader = el)} />
      </Show>
      <div
        ref={(el) => (scroller = el)}
        class="list__scroller"
        role="listbox"
        tabindex="0"
        aria-label={tr("games")}
        aria-activedescendant={activeDescendant()}
        onKeyDown={onKeyDown}
      >
        <StickyGroupHeader
          layout={layout()}
          scrollTop={scrollTop()}
          variant="list"
          onToggle={toggleFromTop}
        />
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
                                    selected={store.isSelected(slot().game.id)}
                                    onSelect={(_, event) => onSelect(games().first, event)}
                                    onContextMenu={(event) => onContextMenu(games().first, event)}
                                  />
                                }
                              >
                                <GameRow
                                  game={slot().game}
                                  columns={shown()}
                                  domKey={slotKey(slot())}
                                  selected={store.isSelected(slot().game.id)}
                                  open={
                                    store.isSelected(slot().game.id) &&
                                    (store.isMulti() || store.detailsOpen())
                                  }
                                  onSelect={(_, event) => onSelect(games().first, event)}
                                  onContextMenu={(event) => onContextMenu(games().first, event)}
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
            <p>{tr("empty")}</p>
          </div>
        </Show>
      </div>
    </div>
  );
}
