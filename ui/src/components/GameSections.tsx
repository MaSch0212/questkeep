import { For, Show, type JSX } from "solid-js";
import type { Achievements } from "../bindings/Achievements";
import type { Game } from "../bindings/Game";
import type { GameActivity } from "../bindings/GameActivity";
import type { GameNews } from "../bindings/GameNews";
import type { ListField } from "../bindings/ListField";
import { formatDate, formatPlaytime } from "../lib/format";
import { activitySummary, exploreFor, formatHours, rarestUnlocked, ttbBar } from "../lib/gamePage";
import { store } from "../lib/store";
import { openLink } from "./settings/openLink";

let sectionSeq = 0;

/** A titled card of the game page. `order` is its place in the narrow, one-column layout. */
export function Card(props: {
  title: string;
  order: number;
  class?: string;
  action?: JSX.Element;
  children: JSX.Element;
}) {
  const id = `game-section-${++sectionSeq}`;
  return (
    <section
      class={`details-card ${props.class ?? ""}`}
      style={{ "--narrow-order": props.order }}
      aria-labelledby={id}
    >
      <h3 id={id} class="details-card__title">
        {props.title}
        <Show when={props.action}>
          <span class="details-card__action">{props.action}</span>
        </Show>
      </h3>
      {props.children}
    </section>
  );
}

/** A value that opens Explore filtered by it. */
export function FilterChip(props: { field: ListField; value: string; plain?: boolean }) {
  return (
    <button
      type="button"
      class="chip chip--link"
      classList={{ "chip--plain": props.plain }}
      title={`Show games with “${props.value}”`}
      onClick={() => store.openExplore(exploreFor(props.field, props.value))}
    >
      {props.value}
    </button>
  );
}

/** Values of a list field as filter links, comma separated (Details card). */
export function FilterValues(props: { field: ListField; values: string[] }) {
  return (
    <For each={props.values}>
      {(value, index) => (
        <>
          <Show when={index() > 0}>, </Show>
          <button
            type="button"
            class="link-value"
            title={`Show games with “${value}”`}
            onClick={() => store.openExplore(exploreFor(props.field, value))}
          >
            {value}
          </button>
        </>
      )}
    </For>
  );
}

/** A 0-100 score as a ring. */
function Ring(props: { value: number }) {
  return (
    <div
      class="score-ring"
      style={{ "--score": props.value }}
      role="img"
      aria-label={`${props.value} of 100`}
    >
      <span aria-hidden="true">{props.value}</span>
    </div>
  );
}

export function ScoresCard(props: { game: Game }) {
  return (
    <Show when={props.game.criticScore || props.game.communityScore}>
      <Card title="Scores" order={2}>
        <div class="scores">
          <Show when={props.game.criticScore}>
            {(score) => (
              <div class="score">
                <Ring value={score().value} />
                <div class="score__text">
                  <b>Critics</b>
                  <small>{score().source}</small>
                </div>
              </div>
            )}
          </Show>
          <Show when={props.game.communityScore}>
            {(score) => (
              <div class="score">
                <Ring value={score().value} />
                <div class="score__text">
                  <b>Players</b>
                  <small>{score().source}</small>
                </div>
              </div>
            )}
          </Show>
        </div>
      </Card>
    </Show>
  );
}

const TTB_SOURCES: Record<string, string> = { igdb: "IGDB", hltb: "HowLongToBeat" };

export function TimeToBeatCard(props: { game: Game }) {
  const bar = () => ttbBar(props.game.timeToBeat, props.game.playtimeSeconds);
  const source = () => {
    const id = props.game.fieldSources.timeToBeat;
    return id ? (TTB_SOURCES[id] ?? null) : null;
  };
  return (
    <Show when={bar()}>
      {(b) => (
        <Card title="Time to beat" order={3}>
          <div class="ttb-bar" aria-hidden="true">
            <For each={b().segments}>
              {(segment) => (
                <div
                  class={`ttb-bar__seg ttb-bar__seg--${segment.key}`}
                  style={{ left: `${segment.left}%`, width: `${segment.width}%` }}
                />
              )}
            </For>
            <Show when={b().you}>
              {(you) => (
                <div
                  class="ttb-bar__you"
                  classList={{ "ttb-bar__you--end": b().youAtEnd }}
                  style={{ left: `${you()}%` }}
                >
                  <b>You · {formatHours(props.game.playtimeSeconds)}</b>
                </div>
              )}
            </Show>
          </div>
          <dl class="ttb">
            <For each={b().segments}>
              {(segment) => (
                <div class={`ttb__item ttb__item--${segment.key}`}>
                  <dt>{segment.label}</dt>
                  <dd>{formatHours(segment.seconds)}</dd>
                </div>
              )}
            </For>
          </dl>
          <Show when={source()}>
            <p class="details-card__source">From {source()}</p>
          </Show>
        </Card>
      )}
    </Show>
  );
}

const shortDate = (unix: number | null) =>
  unix === null ? "" : formatDate(new Date(unix * 1000).toISOString());

export function AchievementsCard(props: { achievements: Achievements | null | undefined }) {
  return (
    <Show when={props.achievements}>
      {(a) => {
        const unlocked = () => a().unlocked.length;
        const percent = () => Math.round((unlocked() / a().total) * 100);
        const rarest = () => rarestUnlocked(a());
        return (
          <Card title="Achievements" order={4}>
            <div class="ach__summary">
              <b>
                {unlocked()} of {a().total}
              </b>
              <span class="muted">{percent()}%</span>
            </div>
            <div
              class="progress"
              role="progressbar"
              aria-label="Achievements unlocked"
              aria-valuenow={percent()}
              aria-valuemin={0}
              aria-valuemax={100}
            >
              <i style={{ width: `${percent()}%` }} />
            </div>
            <Show when={unlocked() > 0}>
              <ul class="ach__icons" aria-label="Unlocked achievements, newest first">
                <For each={a().unlocked.slice(0, 16)}>
                  {(item) => (
                    <li
                      class="ach__icon"
                      title={`${item.name}${item.description ? ` – ${item.description}` : ""}`}
                    >
                      <Show when={item.icon} fallback={<span aria-hidden="true">★</span>}>
                        {(src) => <img src={src()} alt={item.name} loading="lazy" />}
                      </Show>
                    </li>
                  )}
                </For>
              </ul>
            </Show>
            <Show when={rarest()}>
              {(r) => (
                <div class="ach__rare">
                  <Show when={r().icon}>{(src) => <img src={src()} alt="" />}</Show>
                  <div>
                    <b>{r().name}</b>
                    <small>
                      Rarest you have · {r().percent?.toFixed(1)}% of players
                      <Show when={r().unlockedAt}> · {shortDate(r().unlockedAt)}</Show>
                    </small>
                  </div>
                </div>
              )}
            </Show>
          </Card>
        );
      }}
    </Show>
  );
}

const weekLabel = (start: string) =>
  new Date(`${start}T00:00:00`).toLocaleDateString(undefined, { month: "short", day: "numeric" });

export function ActivityCard(props: { activity: GameActivity | null | undefined }) {
  const summary = () => (props.activity ? activitySummary(props.activity) : null);
  return (
    <Show when={props.activity && summary()}>
      {(s) => (
        <Card title="Your activity" order={5}>
          <dl class="act__sum">
            <div>
              <dt>Last 12 weeks</dt>
              <dd>{formatPlaytime(s().total)}</dd>
            </div>
            <div>
              <dt>This week</dt>
              <dd>{formatPlaytime(s().thisWeek)}</dd>
            </div>
            <div>
              <dt>Sessions</dt>
              <dd>{props.activity?.sessions ?? 0}</dd>
            </div>
          </dl>
          <ol class="act__bars" aria-label="Playtime per week">
            <For each={props.activity?.weeks ?? []}>
              {(week, index) => (
                <li
                  class="act__bar"
                  classList={{ "act__bar--zero": week.seconds === 0 }}
                  title={`Week of ${weekLabel(week.start)}: ${formatPlaytime(week.seconds)}`}
                  aria-label={`Week of ${weekLabel(week.start)}: ${formatPlaytime(week.seconds)}`}
                >
                  <i style={{ height: `${s().bars[index()] ?? 0}%` }} />
                </li>
              )}
            </For>
          </ol>
          <div class="act__axis" aria-hidden="true">
            <span>{weekLabel(props.activity?.weeks[0]?.start ?? "")}</span>
            <span>this week</span>
          </div>
        </Card>
      )}
    </Show>
  );
}

export function NewsCard(props: { news: GameNews | null | undefined }) {
  return (
    <Show when={props.news && props.news.items.length > 0 && props.news}>
      {(news) => (
        <Card
          title="News"
          order={6}
          action={
            <Show when={news().allUrl}>
              {(url) => (
                <button type="button" class="link-btn" onClick={() => void openLink(url())}>
                  Open all news ↗
                </button>
              )}
            </Show>
          }
        >
          <ul class="news">
            <For each={news().items}>
              {(item) => (
                <li>
                  <a
                    class="news__item"
                    href={item.url}
                    onClick={(event) => {
                      event.preventDefault();
                      void openLink(item.url);
                    }}
                  >
                    <Show when={item.image}>
                      {(src) => <img class="news__thumb" src={src()} alt="" loading="lazy" />}
                    </Show>
                    <div class="news__text">
                      <h4>{item.title}</h4>
                      <small>
                        {shortDate(item.date)}
                        <Show when={item.date !== null && item.source}> · </Show>
                        {item.source}
                      </small>
                      <Show when={item.snippet}>
                        <p>{item.snippet}</p>
                      </Show>
                    </div>
                  </a>
                </li>
              )}
            </For>
          </ul>
        </Card>
      )}
    </Show>
  );
}
