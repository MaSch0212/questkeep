import Check from "lucide-solid/icons/check";
import ChevronLeft from "lucide-solid/icons/chevron-left";
import ChevronRight from "lucide-solid/icons/chevron-right";
import Pencil from "lucide-solid/icons/pencil";
import Trash from "lucide-solid/icons/trash";
import X from "lucide-solid/icons/x";
import { createMemo, createResource, createSignal, For, Show } from "solid-js";
import type { Game } from "../bindings/Game";
import type { GameSession } from "../bindings/GameSession";
import { intlLocale, translator } from "../i18n";
import { gameSessions } from "../i18n/gameSessions";
import * as api from "../lib/api";
import { errorMessage } from "../lib/errors";
import { formatPlaytime } from "../lib/format";
import {
  fromLocalInput,
  minutesToSeconds,
  pageCount,
  secondsToMinutes,
  toLocalInput,
} from "../lib/sessions";
import { store } from "../lib/store";
import { Modal } from "./Modal";

const tr = translator(gameSessions);

/** Sessions per page: a game can have thousands, and only these are drawn. */
const PAGE_SIZE = 10;

/** A session's start as date and time to the minute. */
const sessionTime = (iso: string) =>
  new Date(iso).toLocaleString(intlLocale(), { dateStyle: "medium", timeStyle: "short" });

/**
 * Adds, corrects and removes the play sessions of a game; playtime follows. The list is paged and
 * rows are plain text until one is edited, so a game with thousands of sessions stays light.
 */
export function SessionsDialog(props: { game: Game; onClose: () => void }) {
  const [version, setVersion] = createSignal(0);
  const [sessions] = createResource(
    () => [props.game.id, version()] as const,
    ([id]) => api.gameSessions(id).catch(() => []),
  );
  const [page, setPage] = createSignal(0);
  const [editing, setEditing] = createSignal<number | null>(null);
  const [newStart, setNewStart] = createSignal(toLocalInput(new Date().toISOString()));
  const [newMinutes, setNewMinutes] = createSignal("60");

  const all = () => sessions.latest ?? [];
  const pages = createMemo(() => pageCount(all().length, PAGE_SIZE));
  // After removing the last row of the last page the page number is too high.
  const current = () => Math.min(page(), pages() - 1);
  const shown = () => all().slice(current() * PAGE_SIZE, (current() + 1) * PAGE_SIZE);
  const total = () => all().reduce((sum, s) => sum + s.durationSeconds, 0);

  /** Runs a change, reloads the list, and shows a failure as a toast. */
  async function change(work: Promise<void>) {
    try {
      await work;
    } catch (error) {
      store.pushToast("error", tr("failed", { error: errorMessage(error) }));
    }
    setVersion((v) => v + 1);
  }

  const newSeconds = () => minutesToSeconds(newMinutes());
  const canAdd = () => fromLocalInput(newStart()) !== null && newSeconds() !== null;

  return (
    <Modal
      title={tr("title", { name: props.game.name })}
      onClose={props.onClose}
      tall
      footer={
        <div class="sessions__foot">
          <Show when={pages() > 1}>
            <nav class="sessions__pager">
              <button
                type="button"
                class="icon-btn"
                aria-label={tr("previousPage")}
                data-tooltip={tr("previousPage")}
                disabled={current() === 0}
                onClick={() => setPage(current() - 1)}
              >
                <ChevronLeft class="icon" aria-hidden="true" />
              </button>
              <span>{tr("pageOf", { page: current() + 1, pages: pages() })}</span>
              <button
                type="button"
                class="icon-btn"
                aria-label={tr("nextPage")}
                data-tooltip={tr("nextPage")}
                disabled={current() >= pages() - 1}
                onClick={() => setPage(current() + 1)}
              >
                <ChevronRight class="icon" aria-hidden="true" />
              </button>
            </nav>
          </Show>
          <button type="button" class="btn" onClick={props.onClose}>
            {tr("close")}
          </button>
        </div>
      }
    >
      <p class="field__hint">{tr("intro")}</p>
      <form
        class="sessions__add"
        onSubmit={(event) => {
          event.preventDefault();
          const started = fromLocalInput(newStart());
          const seconds = newSeconds();
          if (started !== null && seconds !== null) {
            setPage(0);
            void change(api.addGameSession(props.game.id, started, seconds));
          }
        }}
      >
        <label class="field">
          <span class="field__label">{tr("started")}</span>
          <input
            class="input"
            type="datetime-local"
            value={newStart()}
            onInput={(e) => setNewStart(e.currentTarget.value)}
          />
        </label>
        <label class="field">
          <span class="field__label">{tr("minutes")}</span>
          <input
            class="input sessions__minutes"
            type="number"
            min="0"
            step="1"
            value={newMinutes()}
            onInput={(e) => setNewMinutes(e.currentTarget.value)}
          />
        </label>
        <button type="submit" class="btn btn--primary" disabled={!canAdd()}>
          {tr("add")}
        </button>
      </form>
      <Show
        when={all().length > 0}
        fallback={<p class="field__hint">{sessions.loading ? "" : tr("empty")}</p>}
      >
        <p class="sessions__summary">
          {tr("summary", { count: all().length, time: formatPlaytime(total()) })}
        </p>
        <ul class="sessions__list">
          <For each={shown()}>
            {(session) => (
              <Show
                when={editing() === session.id}
                fallback={
                  <SessionRow
                    session={session}
                    onEdit={() => setEditing(session.id)}
                    onRemove={() => void change(api.deleteGameSession(session.id))}
                  />
                }
              >
                <SessionEditor
                  session={session}
                  onSave={(started, seconds) => {
                    setEditing(null);
                    void change(api.updateGameSession(session.id, started, seconds));
                  }}
                  onCancel={() => setEditing(null)}
                />
              </Show>
            )}
          </For>
        </ul>
      </Show>
    </Modal>
  );
}

function SessionRow(props: { session: GameSession; onEdit: () => void; onRemove: () => void }) {
  return (
    <li
      class="sessions__row"
      role="group"
      aria-label={tr("sessionLabel", { date: sessionTime(props.session.startedAt) })}
    >
      <span class="sessions__when">{sessionTime(props.session.startedAt)}</span>
      <span class="sessions__length">{formatPlaytime(props.session.durationSeconds)}</span>
      <button
        type="button"
        class="icon-btn icon-btn--small"
        aria-label={tr("editOne")}
        data-tooltip={tr("editOne")}
        onClick={props.onEdit}
      >
        <Pencil class="icon" aria-hidden="true" />
      </button>
      <button
        type="button"
        class="icon-btn icon-btn--small"
        aria-label={tr("remove")}
        data-tooltip={tr("remove")}
        onClick={props.onRemove}
      >
        <Trash class="icon" aria-hidden="true" />
      </button>
    </li>
  );
}

/** One session's start and minutes as inputs; Enter saves, Escape cancels. */
function SessionEditor(props: {
  session: GameSession;
  onSave: (startedAt: string, seconds: number) => void;
  onCancel: () => void;
}) {
  const [start, setStart] = createSignal(toLocalInput(props.session.startedAt));
  const [minutes, setMinutes] = createSignal(secondsToMinutes(props.session.durationSeconds));
  const started = () => fromLocalInput(start());
  const seconds = () => minutesToSeconds(minutes());
  const valid = () => started() !== null && seconds() !== null;

  function save() {
    const iso = started();
    const length = seconds();
    if (iso !== null && length !== null) props.onSave(iso, length);
  }

  return (
    <li
      class="sessions__row sessions__row--editing"
      onKeyDown={(event) => {
        if (event.key === "Enter") {
          event.preventDefault();
          save();
        } else if (event.key === "Escape") {
          event.preventDefault();
          event.stopPropagation();
          props.onCancel();
        }
      }}
    >
      <input
        class="input"
        type="datetime-local"
        aria-label={tr("started")}
        value={start()}
        onInput={(e) => setStart(e.currentTarget.value)}
        data-autofocus
      />
      <input
        class="input sessions__minutes"
        type="number"
        min="0"
        step="1"
        aria-label={tr("minutes")}
        value={minutes()}
        onInput={(e) => setMinutes(e.currentTarget.value)}
      />
      <button
        type="button"
        class="icon-btn icon-btn--small"
        aria-label={tr("save")}
        data-tooltip={tr("save")}
        disabled={!valid()}
        onClick={save}
      >
        <Check class="icon" aria-hidden="true" />
      </button>
      <button
        type="button"
        class="icon-btn icon-btn--small"
        aria-label={tr("cancel")}
        data-tooltip={tr("cancel")}
        onClick={props.onCancel}
      >
        <X class="icon" aria-hidden="true" />
      </button>
    </li>
  );
}
