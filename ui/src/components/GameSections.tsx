import { intlLocale, translator } from "../i18n";
import { gameSections } from "../i18n/gameSections";
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

const tr = translator(gameSections);

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
      title={tr("showGamesWith", { value: props.value })}
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
            title={tr("showGamesWith", { value })}
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
      aria-label={tr("ringAria", { value: props.value })}
    >
      <span aria-hidden="true">{props.value}</span>
    </div>
  );
}

/** A score's source; the edit dialog stores "Your score" in English. */
const scoreSource = (source: string) => (source === "Your score" ? tr("yourScore") : source);

export function ScoresCard(props: { game: Game }) {
  return (
    <Show when={props.game.criticScore || props.game.communityScore}>
      <Card title={tr("scores")} order={2}>
        <div class="scores">
          <Show when={props.game.criticScore}>
            {(score) => (
              <div class="score">
                <Ring value={score().value} />
                <div class="score__text">
                  <b>{tr("critics")}</b>
                  <small>{scoreSource(score().source)}</small>
                </div>
              </div>
            )}
          </Show>
          <Show when={props.game.communityScore}>
            {(score) => (
              <div class="score">
                <Ring value={score().value} />
                <div class="score__text">
                  <b>{tr("players")}</b>
                  <small>{scoreSource(score().source)}</small>
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
        <Card title={tr("timeToBeat")} order={3}>
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
                  <b>{tr("you", { hours: formatHours(props.game.playtimeSeconds) })}</b>
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
            <p class="details-card__source">{tr("from", { source: source() ?? "" })}</p>
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
          <Card title={tr("achievements")} order={4}>
            <div class="ach__summary">
              <b>{tr("achievementsOf", { unlocked: unlocked(), total: a().total })}</b>
              <span class="muted">{percent()}%</span>
            </div>
            <div
              class="progress"
              role="progressbar"
              aria-label={tr("achievementsUnlocked")}
              aria-valuenow={percent()}
              aria-valuemin={0}
              aria-valuemax={100}
            >
              <i style={{ width: `${percent()}%` }} />
            </div>
            <Show when={unlocked() > 0}>
              <ul class="ach__icons" aria-label={tr("unlockedNewest")}>
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
                      {tr("rarest", {
                        percent: (r().percent ?? 0).toLocaleString(intlLocale(), {
                          minimumFractionDigits: 1,
                          maximumFractionDigits: 1,
                        }),
                      })}
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
  new Date(`${start}T00:00:00`).toLocaleDateString(intlLocale(), {
    month: "short",
    day: "numeric",
  });

export function ActivityCard(props: { activity: GameActivity | null | undefined }) {
  const summary = () => (props.activity ? activitySummary(props.activity) : null);
  return (
    <Show when={props.activity && summary()}>
      {(s) => (
        <Card title={tr("activity")} order={5}>
          <dl class="act__sum">
            <div>
              <dt>{tr("last12Weeks")}</dt>
              <dd>{formatPlaytime(s().total)}</dd>
            </div>
            <div>
              <dt>{tr("thisWeek")}</dt>
              <dd>{formatPlaytime(s().thisWeek)}</dd>
            </div>
            <div>
              <dt>{tr("sessions")}</dt>
              <dd>{props.activity?.sessions ?? 0}</dd>
            </div>
          </dl>
          <ol class="act__bars" aria-label={tr("playtimePerWeek")}>
            <For each={props.activity?.weeks ?? []}>
              {(week, index) => (
                <li
                  class="act__bar"
                  classList={{ "act__bar--zero": week.seconds === 0 }}
                  title={tr("weekOf", {
                    date: weekLabel(week.start),
                    time: formatPlaytime(week.seconds),
                  })}
                  aria-label={tr("weekOf", {
                    date: weekLabel(week.start),
                    time: formatPlaytime(week.seconds),
                  })}
                >
                  <i style={{ height: `${s().bars[index()] ?? 0}%` }} />
                </li>
              )}
            </For>
          </ol>
          <div class="act__axis" aria-hidden="true">
            <span>{weekLabel(props.activity?.weeks[0]?.start ?? "")}</span>
            <span>{tr("thisWeekAxis")}</span>
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
          title={tr("news")}
          order={6}
          action={
            <Show when={news().allUrl}>
              {(url) => (
                <button type="button" class="link-btn" onClick={() => void openLink(url())}>
                  {tr("openAllNews")}
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
