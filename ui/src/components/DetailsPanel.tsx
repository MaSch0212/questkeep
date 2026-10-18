import { createEffect, createSignal, For, on, onCleanup, onMount, Show } from "solid-js";
import type { Game } from "../bindings/Game";
import {
  changeCover,
  installGame,
  launchGame,
  refreshMetadata,
  removeCover,
  removeGame,
  updateGame,
} from "../lib/actions";
import { formatDate, formatPlaytime, formatRelativeDate, formatReleaseDate } from "../lib/format";
import { store } from "../lib/store";
import { Cover } from "./Cover";
import { LaunchOptionsForm } from "./LaunchOptionsForm";

const DESCRIPTION_CLAMP_CHARS = 300;
const DESCRIPTION_CLAMP_LINES = 5;

function isLongText(text: string): boolean {
  return text.length > DESCRIPTION_CLAMP_CHARS || text.split("\n").length > DESCRIPTION_CLAMP_LINES;
}

interface DetailsPanelProps {
  game: Game;
  onClose: () => void;
}

export function DetailsPanel(props: DetailsPanelProps) {
  const [error, setError] = createSignal<string | null>(null);
  const [editingName, setEditingName] = createSignal(false);
  const [draft, setDraft] = createSignal("");
  const [editingLaunch, setEditingLaunch] = createSignal(false);
  const [confirmingRemove, setConfirmingRemove] = createSignal(false);
  const [descriptionOpen, setDescriptionOpen] = createSignal(false);
  const [heroFailed, setHeroFailed] = createSignal(false);
  const [nameInput, setNameInput] = createSignal<HTMLInputElement>();

  const running = () => store.isRunning(props.game.id);
  const isManual = () => props.game.source === "manual";
  const isStoreGame = () => props.game.sourceGameId !== null;
  const heroUrl = () => (heroFailed() ? null : store.backgroundUrl(props.game));
  const released = () => formatReleaseDate(props.game.releaseDate);
  const description = () => props.game.description?.trim() ?? "";

  createEffect(
    on(
      () => props.game.id,
      () => {
        setError(null);
        setEditingName(false);
        setEditingLaunch(false);
        setConfirmingRemove(false);
        setDescriptionOpen(false);
        setHeroFailed(false);
      },
      { defer: true },
    ),
  );

  function startRename() {
    setDraft(props.game.name);
    setEditingName(true);
    queueMicrotask(() => {
      nameInput()?.focus();
      nameInput()?.select();
    });
  }

  function onKey(event: KeyboardEvent) {
    if (event.key === "F2" && !editingName()) {
      event.preventDefault();
      startRename();
    }
  }

  onMount(() => {
    window.addEventListener("keydown", onKey);
    onCleanup(() => window.removeEventListener("keydown", onKey));
  });

  async function run(work: () => Promise<string | null>) {
    setError(null);
    const failure = await work();
    if (failure) setError(failure);
  }

  async function install() {
    setError(null);
    const failure = await installGame(props.game.id);
    if (failure) setError(failure);
    else store.pushToast("info", "Install handed to the store client");
  }

  async function refreshStoreMetadata() {
    setError(null);
    const failure = await refreshMetadata(props.game.id);
    if (failure) setError(failure);
    else store.pushToast("info", "Refreshing metadata…");
  }

  async function commitName() {
    const name = draft().trim();
    if (name === "" || name === props.game.name) {
      setEditingName(false);
      return;
    }
    const failure = await updateGame(props.game.id, { name });
    if (failure) setError(failure);
    else setEditingName(false);
  }

  return (
    <aside class="details" aria-label="Game details">
      <div class="details__header">
        <button
          type="button"
          class="icon-btn"
          aria-label="Close details"
          title="Close (Esc)"
          onClick={props.onClose}
        >
          ×
        </button>
      </div>

      <div class="details__scroll">
        <Show when={heroUrl()}>
          {(src) => (
            <div class="details__hero">
              <img src={src()} alt="" draggable={false} onError={() => setHeroFailed(true)} />
            </div>
          )}
        </Show>
        <div class="details__cover" classList={{ "details__cover--overlap": heroUrl() !== null }}>
          <Cover game={props.game} />
        </div>

        <div class="details__name">
          <Show
            when={editingName()}
            fallback={
              <>
                <h2 class="details__title">{props.game.name}</h2>
                <button
                  type="button"
                  class="icon-btn"
                  aria-label="Rename game"
                  title="Rename (F2)"
                  onClick={startRename}
                >
                  ✎
                </button>
              </>
            }
          >
            <input
              ref={setNameInput}
              class="input"
              aria-label="Game name"
              value={draft()}
              onInput={(event) => setDraft(event.currentTarget.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  void commitName();
                } else if (event.key === "Escape") {
                  event.preventDefault();
                  event.stopPropagation();
                  setEditingName(false);
                }
              }}
            />
          </Show>
        </div>

        <Show
          when={props.game.installed}
          fallback={
            <button type="button" class="btn btn--primary btn--play" onClick={() => void install()}>
              Install
            </button>
          }
        >
          <button
            type="button"
            class="btn btn--primary btn--play"
            disabled={running()}
            onClick={() => void run(() => launchGame(props.game.id))}
          >
            {running() ? "Running…" : "Play"}
          </button>
        </Show>
        <Show when={error()}>
          <p class="error-text" role="alert">
            {error()}
          </p>
        </Show>

        <Show when={description() !== ""}>
          <div class="description">
            <p
              class="description__text"
              classList={{
                "description__text--clamped": isLongText(description()) && !descriptionOpen(),
              }}
            >
              {description()}
            </p>
            <Show when={isLongText(description())}>
              <button
                type="button"
                class="link-btn"
                aria-expanded={descriptionOpen()}
                onClick={() => setDescriptionOpen((open) => !open)}
              >
                {descriptionOpen() ? "Show less" : "Show more"}
              </button>
            </Show>
          </div>
        </Show>

        <Show when={props.game.genres.length > 0}>
          <ul class="chips" aria-label="Genres">
            <For each={props.game.genres}>{(genre) => <li class="chip">{genre}</li>}</For>
          </ul>
        </Show>

        <dl class="facts">
          <Show when={released()}>
            {(text) => (
              <>
                <dt>Released</dt>
                <dd>{text()}</dd>
              </>
            )}
          </Show>
          <Show when={props.game.developers.length > 0}>
            <dt>{props.game.developers.length === 1 ? "Developer" : "Developers"}</dt>
            <dd>{props.game.developers.join(", ")}</dd>
          </Show>
          <Show when={props.game.publishers.length > 0}>
            <dt>{props.game.publishers.length === 1 ? "Publisher" : "Publishers"}</dt>
            <dd>{props.game.publishers.join(", ")}</dd>
          </Show>
          <dt>Source</dt>
          <dd>
            {store.providers().find((p) => p.id === props.game.source)?.name ?? props.game.source}
          </dd>
          <dt>Install directory</dt>
          <dd class="facts__path">{props.game.installDir ?? "Unknown"}</dd>
          <dt>Playtime</dt>
          <dd>{formatPlaytime(props.game.playtimeSeconds)}</dd>
          <dt>Last played</dt>
          <dd>{formatRelativeDate(props.game.lastPlayed)}</dd>
          <dt>Date added</dt>
          <dd>{formatDate(props.game.dateAdded)}</dd>
          <dt>Play count</dt>
          <dd>{props.game.playCount}</dd>
        </dl>

        <div class="details__toggles">
          <label class="check">
            <input
              type="checkbox"
              checked={props.game.favorite}
              onChange={(event) =>
                void run(() => updateGame(props.game.id, { favorite: event.currentTarget.checked }))
              }
            />
            <span>Favorite</span>
          </label>
          <label class="check">
            <input
              type="checkbox"
              checked={props.game.hidden}
              onChange={(event) =>
                void run(() => updateGame(props.game.id, { hidden: event.currentTarget.checked }))
              }
            />
            <span>Hidden</span>
          </label>
        </div>

        <div class="details__row">
          <button
            type="button"
            class="btn"
            onClick={() => void run(() => changeCover(props.game.id))}
          >
            Change cover…
          </button>
          <button
            type="button"
            class="btn"
            disabled={props.game.coverPath === null}
            onClick={() => void run(() => removeCover(props.game.id))}
          >
            Remove cover
          </button>
        </div>

        <Show when={isStoreGame()}>
          <button type="button" class="btn" onClick={() => void refreshStoreMetadata()}>
            Refresh metadata
          </button>
        </Show>

        <Show when={isManual()}>
          <Show
            when={editingLaunch()}
            fallback={
              <button type="button" class="btn" onClick={() => setEditingLaunch(true)}>
                Edit launch options
              </button>
            }
          >
            <LaunchOptionsForm game={props.game} onDone={() => setEditingLaunch(false)} />
          </Show>
        </Show>

        <div class="details__danger">
          <Show
            when={confirmingRemove()}
            fallback={
              <button
                type="button"
                class="btn btn--danger"
                onClick={() => setConfirmingRemove(true)}
              >
                Remove from library
              </button>
            }
          >
            <p>Remove “{props.game.name}” from your library?</p>
            <div class="details__row">
              <button
                type="button"
                class="btn btn--danger-solid"
                onClick={() => void run(() => removeGame(props.game.id))}
              >
                Confirm remove
              </button>
              <button type="button" class="btn" onClick={() => setConfirmingRemove(false)}>
                Cancel
              </button>
            </div>
          </Show>
        </div>
      </div>
    </aside>
  );
}
