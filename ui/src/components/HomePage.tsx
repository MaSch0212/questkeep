import { createMemo, createSignal, For, onCleanup, onMount, Show } from "solid-js";
import type { ShelfGame } from "../bindings/ShelfGame";
import { activateGame } from "../lib/actions";
import { formatPlaytime } from "../lib/format";
import { dayLabel, hoursLabel, relativeDay } from "../lib/startPage";
import { store } from "../lib/store";
import { GameTile, tileDomId } from "./GameTile";
import { SourceIcon, sourceName } from "./SourceIcon";

const data = () => store.startData();
const px = (value: string) => Number.parseFloat(value) || 0;
const days = () => data()?.playtime ?? [];
const stores = () => data()?.stores ?? [];

/**
 * The start page: shelves of recently played and recently added games and a band of stats.
 * Opening a game (Enter) shows it in a view, like the command bar does.
 */
export function HomePage() {
  const [coverWidth, setCoverWidth] = createSignal(160);
  let shelves: HTMLDivElement | undefined;

  onMount(() => {
    if (!shelves || typeof ResizeObserver === "undefined") return;
    const element = shelves;
    const measure = () => {
      // Each shelf gets half the height (sizing from the container, not the shelves, which shrink
      // to their cards); the cover fills what is left below the heading and the card's strip.
      const shelf = element.querySelector<HTMLElement>(".shelf");
      const scroller = shelf?.querySelector<HTMLElement>(".shelf__scroller");
      if (!shelf || !scroller) return;
      const style = getComputedStyle(element);
      const strip = Number.parseFloat(style.getPropertyValue("--qk-tile-strip-height")) || 36;
      const ratio = Number.parseFloat(style.getPropertyValue("--qk-tile-ratio-height")) || 1.5;
      // Between these limits the cover size follows the window height.
      const min = Number.parseFloat(style.getPropertyValue("--qk-shelf-cover-min-width")) || 90;
      const maxHeight =
        Number.parseFloat(style.getPropertyValue("--qk-shelf-cover-max-height")) || 300;
      const scrollerStyle = getComputedStyle(scroller);
      const chrome =
        shelf.offsetHeight -
        scroller.offsetHeight +
        px(scrollerStyle.paddingTop) +
        px(scrollerStyle.paddingBottom) +
        px(scrollerStyle.borderTopWidth) +
        px(scrollerStyle.borderBottomWidth);
      const perShelf = (element.clientHeight - px(style.rowGap)) / 2;
      const room = perShelf - chrome - strip;
      setCoverWidth(Math.round(Math.min(maxHeight / ratio, Math.max(min, room / ratio))));
    };
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    measure();
    onCleanup(() => observer.disconnect());
  });

  return (
    <section class="home" aria-label="Start page">
      <div
        class="home__shelves"
        ref={(el) => (shelves = el)}
        style={{ "--qk-shelf-cover-width": `${coverWidth()}px` }}
      >
        <Shelf
          title="Recently played"
          games={data()?.recentlyPlayed ?? []}
          empty="Nothing played yet"
        />
        <Shelf title="Recently added" games={data()?.recentlyAdded ?? []} empty="No games yet" />
      </div>
      <div class="home__stats">
        <PlaytimePanel />
        <StoresPanel />
      </div>
    </section>
  );
}

function Shelf(props: { title: string; games: ShelfGame[]; empty: string }) {
  const [selected, setSelected] = createSignal<string | null>(null);
  let scroller: HTMLDivElement | undefined;
  const headingId = `shelf-${props.title.toLowerCase().replace(/\s+/g, "-")}`;
  const domKey = (id: string) => `${headingId}-${id}`;

  const selectedGame = () => props.games.find((s) => s.game.id === selected())?.game;

  function select(id: string) {
    setSelected(id);
    document.getElementById(tileDomId(domKey(id)))?.scrollIntoView?.({ inline: "nearest" });
  }

  function onKeyDown(event: KeyboardEvent) {
    const ids = props.games.map((s) => s.game.id);
    if (ids.length === 0) return;
    const index = ids.indexOf(selected() ?? "");
    const step: Record<string, number> = { ArrowLeft: -1, ArrowRight: 1 };
    if (event.key in step) {
      event.preventDefault();
      const next =
        index < 0 ? 0 : Math.min(ids.length - 1, Math.max(0, index + (step[event.key] ?? 0)));
      select(ids[next] ?? ids[0] ?? "");
    } else if (event.key === "Home" || event.key === "End") {
      event.preventDefault();
      select((event.key === "Home" ? ids[0] : ids.at(-1)) ?? "");
    } else if (event.key === "Enter" || event.key === "F5") {
      const game = selectedGame();
      if (!game) return;
      event.preventDefault();
      if (event.key === "F5" || event.ctrlKey || event.metaKey) void activateGame(game.id);
      else store.openGamePage(game);
    }
  }

  // The mouse wheel scrolls the shelf sideways.
  function onWheel(event: WheelEvent) {
    if (!scroller || event.deltaX !== 0 || event.deltaY === 0) return;
    if (scroller.scrollWidth <= scroller.clientWidth) return;
    event.preventDefault();
    scroller.scrollLeft += event.deltaY;
  }

  return (
    <div class="shelf">
      <h2 class="shelf__title" id={headingId}>
        {props.title}
      </h2>
      <div
        class="shelf__scroller"
        ref={(el) => (scroller = el)}
        role="grid"
        tabindex="0"
        aria-labelledby={headingId}
        aria-activedescendant={selected() ? tileDomId(domKey(selected() ?? "")) : undefined}
        onKeyDown={onKeyDown}
        onWheel={onWheel}
        onFocus={() => {
          if (selected() === null && props.games[0]) setSelected(props.games[0].game.id);
        }}
      >
        <Show when={props.games.length > 0} fallback={<p class="shelf__empty">{props.empty}</p>}>
          <div class="shelf__row" role="row">
            <For each={props.games}>
              {(item) => (
                <GameTile
                  game={item.game}
                  domKey={domKey(item.game.id)}
                  selected={selected() === item.game.id}
                  date={relativeDay(item.date)}
                  onSelect={(id) => {
                    setSelected(id);
                    scroller?.focus({ preventScroll: true });
                  }}
                  onLaunch={(id) => void activateGame(id)}
                />
              )}
            </For>
          </div>
        </Show>
      </div>
    </div>
  );
}

function PlaytimePanel() {
  const total = () => days().reduce((sum, d) => sum + d.seconds, 0);
  // At least an hour, so a few minutes don't fill the chart.
  const max = () => Math.max(3600, ...days().map((d) => d.seconds));
  const average = () => (days().length > 0 ? total() / days().length : 0);

  return (
    <section class="home-panel" aria-labelledby="home-playtime">
      <header class="home-panel__head">
        <h2 class="home-panel__title" id="home-playtime">
          Playtime, last {days().length || 7} days
        </h2>
        <span class="home-panel__sub">
          {(total() / 3600).toFixed(1)} h · avg {(average() / 3600).toFixed(1)} h/day
        </span>
      </header>
      <ol class="playtime-chart">
        <For each={days()}>
          {(day, i) => {
            const label = () => dayLabel(day.date);
            return (
              <li
                class="playtime-chart__day"
                classList={{ "playtime-chart__day--today": i() === days().length - 1 }}
                title={day.seconds > 0 ? formatPlaytime(day.seconds) : "Not played"}
                aria-label={`${label()}: ${day.seconds > 0 ? formatPlaytime(day.seconds) : "not played"}`}
              >
                <span class="playtime-chart__value" aria-hidden="true">
                  {hoursLabel(day.seconds)}
                </span>
                <span class="playtime-chart__track" aria-hidden="true">
                  <span
                    class="playtime-chart__bar"
                    style={{ height: `${Math.max(2, (day.seconds / max()) * 100)}%` }}
                  />
                </span>
                <span class="playtime-chart__label" aria-hidden="true">
                  {label()}
                </span>
              </li>
            );
          }}
        </For>
      </ol>
    </section>
  );
}

function StoresPanel() {
  const installed = createMemo(() => stores().reduce((sum, s) => sum + s.installed, 0));

  return (
    <section class="home-panel" aria-labelledby="home-stores">
      <header class="home-panel__head">
        <h2 class="home-panel__title" id="home-stores">
          Installed by store
        </h2>
        <span class="home-panel__sub">{installed()} installed</span>
      </header>
      <Show when={stores().length > 0} fallback={<p class="shelf__empty">No store games yet</p>}>
        <ul class="store-stats">
          <For each={stores()}>
            {(entry) => {
              const name = () => sourceName(entry.source);
              const share = () => (entry.owned > 0 ? entry.installed / entry.owned : 0);
              return (
                <li
                  class="store-stats__row"
                  title={`${Math.round(share() * 100)} % of your ${name()} games are installed`}
                >
                  <SourceIcon source={entry.source} />
                  <span class="store-stats__name">{name()}</span>
                  <span class="store-stats__track" aria-hidden="true">
                    <span class="store-stats__fill" style={{ width: `${share() * 100}%` }} />
                  </span>
                  <span class="store-stats__count">
                    <b>{entry.installed}</b> / {entry.owned}
                  </span>
                </li>
              );
            }}
          </For>
        </ul>
      </Show>
    </section>
  );
}
