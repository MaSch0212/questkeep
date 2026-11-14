import CalendarDays from "lucide-solid/icons/calendar-days";
import ChartColumn from "lucide-solid/icons/chart-column";
import CircleCheck from "lucide-solid/icons/circle-check";
import Clock from "lucide-solid/icons/clock";
import Cpu from "lucide-solid/icons/cpu";
import HardDrive from "lucide-solid/icons/hard-drive";
import Sparkles from "lucide-solid/icons/sparkles";
import Store from "lucide-solid/icons/store";
import Tags from "lucide-solid/icons/tags";
import Trophy from "lucide-solid/icons/trophy";
import { createMemo, createResource, createSignal, For, type JSX, Show } from "solid-js";
import type { ChartRange } from "../bindings/ChartRange";
import type { ChartSplit } from "../bindings/ChartSplit";
import type { DriveKind } from "../bindings/DriveKind";
import type { Statistics } from "../bindings/Statistics";
import { intlLocale, translator } from "../i18n";
import { statistics as dict } from "../i18n/statistics";
import * as api from "../lib/api";
import { formatPlaytime, formatSize } from "../lib/format";
import {
  busiestSlot,
  calendarWeeks,
  chartLabel,
  chartScale,
  chartTitle,
  chartTotals,
  driveShares,
  heatLevel,
  hoursText,
  memoryText,
  weekdayName,
} from "../lib/statistics";
import { store } from "../lib/store";
import { SourceIcon, sourceName } from "./SourceIcon";

const tr = translator(dict);

type KindKey = "kindNvme" | "kindSsd" | "kindHdd" | "kindRemovable" | "kindNetwork" | "kindUnknown";

const KIND_KEYS: Record<DriveKind, KindKey> = {
  nvme: "kindNvme",
  ssd: "kindSsd",
  hdd: "kindHdd",
  removable: "kindRemovable",
  network: "kindNetwork",
  unknown: "kindUnknown",
};

/** Series colors in a fixed order (theme.css); "other" is always the last, gray one. */
const SERIES = 6;
const seriesColor = (index: number, key: string) =>
  key === "other"
    ? "var(--qk-color-series-other)"
    : `var(--qk-color-series-${(index % SERIES) + 1})`;

function Card(props: {
  icon: JSX.Element;
  title: string;
  aside?: JSX.Element;
  wide?: boolean;
  children: JSX.Element;
}) {
  return (
    <section class="stats-card" classList={{ "stats-card--wide": props.wide }}>
      <header class="stats-card__head">
        <h2 class="stats-card__title">
          {props.icon}
          {props.title}
        </h2>
        <Show when={props.aside}>
          <span class="stats-card__aside">{props.aside}</span>
        </Show>
      </header>
      {props.children}
    </section>
  );
}

function Segmented<T extends string>(props: {
  label: string;
  value: T;
  options: readonly { value: T; label: string }[];
  onChange: (value: T) => void;
}) {
  return (
    <div class="segmented" role="radiogroup" aria-label={props.label}>
      <For each={props.options}>
        {(option) => (
          <button
            type="button"
            role="radio"
            class="segmented__item"
            classList={{ "segmented__item--on": props.value === option.value }}
            aria-checked={props.value === option.value}
            onClick={() => props.onChange(option.value)}
          >
            {option.label}
          </button>
        )}
      </For>
    </div>
  );
}

/** A store keeps its color everywhere on the page: its place in the user's store order. */
function storeColor(key: string): string {
  const index = store
    .providers()
    .filter((p) => p.id !== "manual")
    .findIndex((p) => p.id === key);
  return index < 0 ? "var(--qk-color-series-other)" : seriesColor(index, key);
}

/** A labeled horizontal bar ("Rocket League ▇▇▇▇ 1,384 h"). */
function BarRow(props: {
  label: JSX.Element;
  value: number;
  max: number;
  text: string;
  color?: string;
  title?: string;
}) {
  return (
    <li class="stats-bar" data-tooltip={props.title}>
      <span class="stats-bar__label">{props.label}</span>
      <span class="stats-bar__track" aria-hidden="true">
        <span
          class="stats-bar__fill"
          style={{
            width: `${props.max > 0 ? (props.value / props.max) * 100 : 0}%`,
            background: props.color,
          }}
        />
      </span>
      <span class="stats-bar__value">{props.text}</span>
    </li>
  );
}

function PlaytimeCard(props: { history: string | null }) {
  const [range, setRange] = createSignal<ChartRange>("month");
  const [split, setSplit] = createSignal<ChartSplit>("total");
  const [chart] = createResource(
    () => [range(), split()] as const,
    ([r, s]) => api.playtimeChart(r, s),
  );
  const totals = createMemo(() => {
    const data = chart();
    return data ? chartTotals(data) : [];
  });
  const scale = createMemo(() => chartScale(totals()));
  const seriesName = (key: string, name: string | null) =>
    key === "other" ? tr("other") : key === "total" ? tr("playtime") : (name ?? sourceName(key));
  const empty = () => totals().every((t) => t === 0);

  return (
    <Card
      icon={<ChartColumn class="icon" aria-hidden="true" />}
      title={tr("playtime")}
      wide
      aside={
        <span class="stats-controls">
          <Segmented
            label={tr("split")}
            value={split()}
            onChange={setSplit}
            options={[
              { value: "total", label: tr("splitTotal") },
              { value: "store", label: tr("splitStore") },
              { value: "game", label: tr("splitGame") },
            ]}
          />
          <Segmented
            label={tr("range")}
            value={range()}
            onChange={setRange}
            options={[
              { value: "week", label: tr("rangeWeek") },
              { value: "month", label: tr("rangeMonth") },
              { value: "year", label: tr("rangeYear") },
            ]}
          />
        </span>
      }
    >
      <Show when={chart()}>
        {(data) => (
          <>
            <Show when={split() !== "total" && !empty()}>
              <ul class="stats-legend">
                <For each={data().series}>
                  {(series, i) => (
                    <li>
                      <span
                        class="stats-legend__swatch"
                        style={{
                          background:
                            split() === "store"
                              ? storeColor(series.key)
                              : seriesColor(i(), series.key),
                        }}
                      />
                      {seriesName(series.key, series.name)}
                    </li>
                  )}
                </For>
              </ul>
            </Show>
            <div class="stats-chart" classList={{ "stats-chart--empty": empty() }}>
              <div class="stats-chart__axis" aria-hidden="true">
                <span>{hoursText(scale())}</span>
                <span>{hoursText(scale() / 2)}</span>
                <span>0 h</span>
              </div>
              <ol class="stats-chart__bars">
                <For each={data().labels}>
                  {(label, i) => {
                    const total = () => totals()[i()] ?? 0;
                    const text = () =>
                      tr("bucket", {
                        label: chartTitle(label, range()),
                        time: formatPlaytime(total()),
                      });
                    return (
                      <li
                        class="stats-chart__bucket"
                        data-tooltip={total() > 0 ? text() : undefined}
                        aria-label={text()}
                      >
                        <span class="stats-chart__stack" aria-hidden="true">
                          <For each={data().series}>
                            {(series, s) => (
                              <span
                                class="stats-chart__segment"
                                style={{
                                  height: `${((series.values[i()] ?? 0) / scale()) * 100}%`,
                                  background:
                                    split() === "store"
                                      ? storeColor(series.key)
                                      : seriesColor(s(), series.key),
                                }}
                              />
                            )}
                          </For>
                        </span>
                        <span class="stats-chart__label" aria-hidden="true">
                          {chartLabel(label, range())}
                        </span>
                      </li>
                    );
                  }}
                </For>
              </ol>
              <Show when={empty()}>
                <p class="stats-chart__empty">{tr("noPlaytime")}</p>
              </Show>
            </div>
          </>
        )}
      </Show>
      <p class="stats-note">{props.history === null ? tr("historyStarts") : tr("historyNote")}</p>
    </Card>
  );
}

function ActivityCards(props: { stats: Statistics }) {
  const max = createMemo(() => Math.max(0, ...props.stats.calendar.map((d) => d.seconds)));
  const hourMax = createMemo(() => Math.max(0, ...props.stats.hours.flat()));
  const busy = createMemo(() => busiestSlot(props.stats.hours));
  const days = (count: number) => tr("days", { count });
  return (
    <>
      <Card
        icon={<CalendarDays class="icon" aria-hidden="true" />}
        title={tr("calendar")}
        aside={tr("streak", {
          current: days(props.stats.currentStreak),
          longest: days(props.stats.longestStreak),
        })}
      >
        <div class="stats-calendar">
          <For each={calendarWeeks(props.stats.calendar)}>
            {(week) => (
              <div class="stats-calendar__week">
                <For each={week}>
                  {(day) => (
                    <span
                      class={`stats-heat stats-heat--${heatLevel(day.seconds, max())}`}
                      data-tooltip={tr("bucket", {
                        label: chartTitle(day.day, "month"),
                        time: day.seconds > 0 ? formatPlaytime(day.seconds) : "–",
                      })}
                    />
                  )}
                </For>
              </div>
            )}
          </For>
        </div>
      </Card>
      <Card
        icon={<Clock class="icon" aria-hidden="true" />}
        title={tr("whenYouPlay")}
        aside={
          <Show when={busy()}>
            {(slot) =>
              tr("mostly", {
                day: weekdayName(slot().day),
                from: slot().hour,
                to: (slot().hour + 3) % 24,
              })
            }
          </Show>
        }
      >
        <Show when={hourMax() > 0} fallback={<p class="stats-empty">{tr("noSessions")}</p>}>
          <div class="stats-hours">
            <For each={props.stats.hours}>
              {(row, day) => (
                <>
                  <span class="stats-hours__day">{weekdayName(day())}</span>
                  <For each={row}>
                    {(seconds, hour) => (
                      <span
                        class={`stats-heat stats-heat--${heatLevel(seconds, hourMax())}`}
                        data-tooltip={`${weekdayName(day(), "long")} ${hour()}:00 · ${formatPlaytime(seconds)}`}
                      />
                    )}
                  </For>
                </>
              )}
            </For>
            <span />
            <For each={[0, 6, 12, 18]}>
              {(hour) => <span class="stats-hours__hour">{hour}</span>}
            </For>
          </div>
        </Show>
      </Card>
    </>
  );
}

function DrivesCard(props: { stats: Statistics }) {
  const hdd = () =>
    props.stats.drives.filter((d) => d.drive.kind === "hdd").reduce((n, d) => n + d.games, 0);
  return (
    <Card
      icon={<HardDrive class="icon" aria-hidden="true" />}
      title={tr("drives")}
      wide
      aside={
        <ul class="stats-legend">
          <li>
            <span class="stats-legend__swatch stats-drive__games" />
            {tr("legendGames")}
          </li>
          <li>
            <span class="stats-legend__swatch stats-drive__other" />
            {tr("legendOther")}
          </li>
          <li>
            <span class="stats-legend__swatch stats-drive__free" />
            {tr("legendFree")}
          </li>
        </ul>
      }
    >
      <ul class="stats-drives">
        <For each={props.stats.drives}>
          {(usage) => {
            const shares = () => driveShares(usage);
            return (
              <li class="stats-drive">
                <span class="stats-drive__name">
                  <b>{usage.drive.mount.replace(/\\$/, "")}</b>
                  <Show when={usage.drive.label}>{(label) => <span>{label()}</span>}</Show>
                  <span
                    class="stats-chip"
                    classList={{ "stats-chip--warn": usage.drive.kind === "hdd" }}
                  >
                    {tr(KIND_KEYS[usage.drive.kind])}
                  </span>
                </span>
                <span class="stats-drive__track" aria-hidden="true">
                  <span class="stats-drive__games" style={{ width: `${shares().games * 100}%` }} />
                  <span class="stats-drive__other" style={{ width: `${shares().other * 100}%` }} />
                </span>
                <span class="stats-drive__text">
                  <Show when={usage.games > 0}>
                    <span>
                      {tr("driveGames", {
                        count: usage.games,
                        size: usage.gamesBytes > 0 ? formatSize(usage.gamesBytes) : "",
                      })}
                    </span>
                  </Show>
                  <span>
                    {tr("driveFree", {
                      free: formatSize(usage.drive.freeBytes),
                      total: formatSize(usage.drive.totalBytes),
                    })}
                  </span>
                </span>
              </li>
            );
          }}
        </For>
      </ul>
      <Show when={hdd() > 0}>
        <p class="stats-hint stats-hint--warn">{tr("hddHint", { count: hdd() })}</p>
      </Show>
      <Show when={props.stats.cleanup.count > 0}>
        <p class="stats-hint">
          {tr("cleanup", {
            size: formatSize(props.stats.cleanup.bytes),
            count: props.stats.cleanup.count,
            names: props.stats.cleanup.largest
              .map((g) => `${g.name} (${formatSize(g.bytes)})`)
              .join(", "),
          })}
        </p>
      </Show>
    </Card>
  );
}

function LibraryCard(props: { stats: Statistics }) {
  const total = () => props.stats.completion.reduce((n, c) => n + c.count, 0);
  const statusName = (status: string | null) =>
    status === null ? tr("noStatus") : store.valueName("completionStatus", status);
  const decadeMax = () => Math.max(1, ...props.stats.decades.map((d) => d.count));
  return (
    <Card icon={<CircleCheck class="icon" aria-hidden="true" />} title={tr("completion")}>
      <span class="stats-split" aria-hidden="true">
        <For each={props.stats.completion}>
          {(item, i) => (
            <span
              style={{
                width: `${(item.count / Math.max(1, total())) * 100}%`,
                background: seriesColor(i(), item.status ?? "other"),
              }}
            />
          )}
        </For>
      </span>
      <ul class="stats-legend">
        <For each={props.stats.completion}>
          {(item, i) => (
            <li>
              <span
                class="stats-legend__swatch"
                style={{ background: seriesColor(i(), item.status ?? "other") }}
              />
              {statusName(item.status)} <b>{item.count}</b>
            </li>
          )}
        </For>
      </ul>
      <h3 class="stats-card__subtitle">{tr("releaseYears")}</h3>
      <ol class="stats-columns">
        <For each={props.stats.decades}>
          {(decade) => (
            <li data-tooltip={`${decade.decade}–${decade.decade + 9}: ${decade.count}`}>
              <span class="stats-columns__value">{decade.count}</span>
              <span
                class="stats-columns__bar"
                style={{ height: `${(decade.count / decadeMax()) * 100}%` }}
              />
              <span class="stats-columns__label">{`’${String(decade.decade).slice(2)}`}</span>
            </li>
          )}
        </For>
      </ol>
    </Card>
  );
}

/** A `YYYY-MM-DD` date in the UI language ("Jul 22, 2026"). */
function longDate(iso: string): string {
  return new Date(`${iso}T00:00:00`).toLocaleDateString(intlLocale(), { dateStyle: "medium" });
}

function SystemCard(props: { stats: Statistics }) {
  const sys = () => props.stats.system;
  const facts = () => props.stats.facts;
  return (
    <Card icon={<Cpu class="icon" aria-hidden="true" />} title={tr("thisPc")}>
      <dl class="stats-specs">
        <Show when={sys().cpu}>
          {(cpu) => (
            <>
              <dt>{tr("processor")}</dt>
              <dd>
                {cpu().name}
                <span>{tr("coresThreads", { cores: cpu().cores, threads: cpu().threads })}</span>
              </dd>
            </>
          )}
        </Show>
        <Show when={sys().gpus.length > 0}>
          <dt>{tr("graphics")}</dt>
          <dd>
            <For each={sys().gpus}>
              {(gpu) => (
                <div class="stats-specs__item">
                  {gpu.name}
                  <Show when={gpu.memoryBytes}>{(bytes) => ` · ${formatSize(bytes())}`}</Show>
                  <Show when={gpu.driverVersion}>
                    {(version) => (
                      <span>
                        {gpu.driverDate
                          ? tr("driver", { version: version(), date: longDate(gpu.driverDate) })
                          : version()}
                      </span>
                    )}
                  </Show>
                </div>
              )}
            </For>
          </dd>
        </Show>
        <dt>{tr("memory")}</dt>
        <dd>{memoryText(sys().memoryBytes)}</dd>
        <Show when={sys().displays.length > 0}>
          <dt>{tr("displays")}</dt>
          <dd>
            <For each={sys().displays}>
              {(display) => (
                <div class="stats-specs__item">
                  {display.name}
                  <span>
                    {`${display.width} × ${display.height} · ${Math.round(display.refreshHz)} Hz`}
                    <Show when={display.hdr}> · HDR</Show>
                    <Show when={display.primary && sys().displays.length > 1}>
                      {` · ${tr("primary")}`}
                    </Show>
                  </span>
                </div>
              )}
            </For>
          </dd>
        </Show>
        <dt>{tr("system")}</dt>
        <dd>{sys().os}</dd>
        <dt>{tr("uptime")}</dt>
        <dd>
          {sys().uptimeSeconds < 86_400
            ? formatPlaytime(sys().uptimeSeconds)
            : tr("uptimeValue", {
                days: Math.floor(sys().uptimeSeconds / 86_400),
                hours: Math.floor((sys().uptimeSeconds % 86_400) / 3600),
              })}
        </dd>
      </dl>
      <h3 class="stats-card__subtitle">
        <Sparkles class="icon" aria-hidden="true" />
        {tr("funFacts")}
      </h3>
      <ul class="stats-facts">
        <Show when={facts().longestSession}>
          {(s) => (
            <li>{tr("longestSession", { name: s().name, time: formatPlaytime(s().seconds) })}</li>
          )}
        </Show>
        <Show when={facts().averageSessionSeconds}>
          {(s) => <li>{tr("averageSession", { time: formatPlaytime(s()) })}</li>}
        </Show>
        <Show when={facts().backlogGames > 0}>
          <li>
            {tr("backlog", {
              count: facts().backlogGames,
              time: hoursText(facts().backlogSeconds),
            })}
          </li>
        </Show>
        <Show when={facts().topShare}>
          {(share) => <li>{tr("topShare", { percent: Math.round(share() * 100) })}</li>}
        </Show>
      </ul>
    </Card>
  );
}

/** The statistics page (below Home in the sidebar): playtime, library, drives and this PC. */
export function StatisticsPage() {
  const [stats] = createResource(() => api.statistics());

  return (
    <section class="settings statistics" aria-label={tr("title")}>
      <header class="topbar">
        <h1 class="topbar__title">{tr("title")}</h1>
      </header>
      <div class="statistics__scroll">
        <Show when={stats()} fallback={<p class="muted statistics__loading">{tr("loading")}</p>}>
          {(s) => {
            const delta = () => s().thisWeekSeconds - s().lastWeekSeconds;
            const topMax = () => s().topGames[0]?.seconds ?? 0;
            const storeMax = () => s().stores[0]?.seconds ?? 0;
            const genreMax = () => s().genres[0]?.seconds ?? 0;
            const drivesWithGames = () => s().drives.filter((d) => d.games > 0).length;
            return (
              <div class="statistics__grid">
                <div class="stats-kpis">
                  <div class="stats-kpi">
                    <span class="stats-kpi__label">{tr("totalPlaytime")}</span>
                    <span class="stats-kpi__value">{hoursText(s().totalSeconds)}</span>
                    <span class="stats-kpi__sub">
                      {tr("daysOfPlaytime", {
                        days: Math.round(s().totalSeconds / 86_400).toLocaleString(intlLocale()),
                      })}
                    </span>
                  </div>
                  <div class="stats-kpi">
                    <span class="stats-kpi__label">{tr("thisWeek")}</span>
                    <span class="stats-kpi__value">{hoursText(s().thisWeekSeconds)}</span>
                    <span class="stats-kpi__sub" classList={{ "stats-kpi__sub--up": delta() > 0 }}>
                      {delta() === 0
                        ? tr("sameAsLastWeek")
                        : tr(delta() > 0 ? "moreThanLastWeek" : "lessThanLastWeek", {
                            time: hoursText(Math.abs(delta())),
                          })}
                    </span>
                  </div>
                  <div class="stats-kpi">
                    <span class="stats-kpi__label">{tr("games")}</span>
                    <span class="stats-kpi__value">{s().games.toLocaleString(intlLocale())}</span>
                    <span class="stats-kpi__sub">
                      {tr("gamesSub", { installed: s().installed, never: s().neverPlayed })}
                    </span>
                  </div>
                  <div class="stats-kpi">
                    <span class="stats-kpi__label">{tr("installedSize")}</span>
                    <span class="stats-kpi__value">{formatSize(s().installedBytes)}</span>
                    <span class="stats-kpi__sub">
                      {tr("onDrives", { count: drivesWithGames() })}
                    </span>
                  </div>
                </div>
                <PlaytimeCard history={s().historySince} />
                <ActivityCards stats={s()} />
                <Card
                  icon={<Trophy class="icon" aria-hidden="true" />}
                  title={tr("mostPlayed")}
                  aside={tr("allTime")}
                >
                  <Show
                    when={s().topGames.length > 0}
                    fallback={<p class="stats-empty">{tr("noneYet")}</p>}
                  >
                    <ol class="stats-bars">
                      <For each={s().topGames}>
                        {(game) => (
                          <BarRow
                            label={game.name}
                            title={game.name}
                            value={game.seconds}
                            max={topMax()}
                            text={hoursText(game.seconds)}
                          />
                        )}
                      </For>
                    </ol>
                  </Show>
                </Card>
                <Card icon={<Store class="icon" aria-hidden="true" />} title={tr("byStore")}>
                  <ul class="stats-bars">
                    <For each={s().stores}>
                      {(entry) => (
                        <BarRow
                          label={
                            <>
                              <SourceIcon source={entry.key} />
                              {sourceName(entry.key)}
                            </>
                          }
                          value={entry.seconds}
                          max={storeMax()}
                          text={hoursText(entry.seconds)}
                          color={storeColor(entry.key)}
                        />
                      )}
                    </For>
                  </ul>
                  <h3 class="stats-card__subtitle">
                    <Tags class="icon" aria-hidden="true" />
                    {tr("topGenres")}
                  </h3>
                  <ul class="stats-bars">
                    <For each={s().genres}>
                      {(genre) => (
                        <BarRow
                          label={genre.key}
                          value={genre.seconds}
                          max={genreMax()}
                          text={hoursText(genre.seconds)}
                        />
                      )}
                    </For>
                  </ul>
                </Card>
                <DrivesCard stats={s()} />
                <LibraryCard stats={s()} />
                <SystemCard stats={s()} />
              </div>
            );
          }}
        </Show>
      </div>
    </section>
  );
}
