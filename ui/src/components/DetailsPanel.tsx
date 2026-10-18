import { createEffect, createSignal, on, onCleanup, onMount, Show } from "solid-js";
import type { Game } from "../bindings/Game";
import { changeCover, launchGame, removeCover, removeGame, updateGame } from "../lib/actions";
import { formatDate, formatPlaytime, formatRelativeDate } from "../lib/format";
import { store } from "../lib/store";
import { Cover } from "./Cover";
import { LaunchOptionsForm } from "./LaunchOptionsForm";

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
  const [nameInput, setNameInput] = createSignal<HTMLInputElement>();

  const running = () => store.isRunning(props.game.id);
  const isManual = () => props.game.source === "manual";

  createEffect(
    on(
      () => props.game.id,
      () => {
        setError(null);
        setEditingName(false);
        setEditingLaunch(false);
        setConfirmingRemove(false);
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
        <div class="details__cover">
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

        <button
          type="button"
          class="btn btn--primary btn--play"
          disabled={running()}
          onClick={() => void run(() => launchGame(props.game.id))}
        >
          {running() ? "Running…" : "Play"}
        </button>
        <Show when={error()}>
          <p class="error-text" role="alert">
            {error()}
          </p>
        </Show>

        <dl class="facts">
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
