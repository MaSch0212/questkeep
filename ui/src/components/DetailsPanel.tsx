import EyeOff from "lucide-solid/icons/eye-off";
import Eye from "lucide-solid/icons/eye";
import ImageDown from "lucide-solid/icons/image-down";
import PackageMinus from "lucide-solid/icons/package-minus";
import Pencil from "lucide-solid/icons/pencil";
import Star from "lucide-solid/icons/star";
import X from "lucide-solid/icons/x";
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
import type { Game } from "../bindings/Game";
import {
  changeCover,
  installGame,
  launchGame,
  refetchArtwork,
  refreshMetadata,
  removeCover,
  removeGame,
  uninstallGame,
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
  /** Shown beside the library instead of overlaying it. */
  split?: boolean;
  onClose: () => void;
}

export function DetailsPanel(props: DetailsPanelProps) {
  const [error, setError] = createSignal<string | null>(null);
  const [editingName, setEditingName] = createSignal(false);
  const [draft, setDraft] = createSignal("");
  const [editingLaunch, setEditingLaunch] = createSignal(false);
  const [confirmingRemove, setConfirmingRemove] = createSignal(false);
  const [confirmingUninstall, setConfirmingUninstall] = createSignal(false);
  const [descriptionOpen, setDescriptionOpen] = createSignal(false);
  const [heroFailed, setHeroFailed] = createSignal(false);
  const [nameInput, setNameInput] = createSignal<HTMLInputElement>();
  const [uninstallButton, setUninstallButton] = createSignal<HTMLButtonElement>();
  const [cancelUninstallButton, setCancelUninstallButton] = createSignal<HTMLButtonElement>();

  const running = () => store.isRunning(props.game.id);
  const canUninstall = () =>
    props.game.installed &&
    props.game.sourceGameId !== null &&
    store.providers().some((p) => p.id === props.game.source && p.canUninstall);
  const isManual = () => props.game.source === "manual";
  const heroUrl = () => (heroFailed() ? null : store.backgroundUrl(props.game));
  const released = () => formatReleaseDate(props.game.releaseDate);
  const description = () => props.game.description?.trim() ?? "";

  // A memo, so list refreshes (a new game object with the same id, every few seconds while covers
  // download) don't count as switching games and close open editors and confirmations.
  const gameId = createMemo(() => props.game.id);
  createEffect(
    on(
      gameId,
      () => {
        setError(null);
        setEditingName(false);
        setEditingLaunch(false);
        setConfirmingRemove(false);
        setConfirmingUninstall(false);
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

  function askUninstall() {
    setConfirmingUninstall(true);
    queueMicrotask(() => cancelUninstallButton()?.focus());
  }

  function cancelUninstall() {
    setConfirmingUninstall(false);
    queueMicrotask(() => uninstallButton()?.focus());
  }

  async function uninstall() {
    setError(null);
    setConfirmingUninstall(false);
    const failure = await uninstallGame(props.game.id);
    if (failure) setError(failure);
    else store.pushToast("info", "Uninstall handed to the store client");
    queueMicrotask(() => uninstallButton()?.focus());
  }

  /** Store games ask their store (then IGDB); others only IGDB, which the core may refuse. */
  async function refreshGameMetadata() {
    setError(null);
    const failure = await refreshMetadata(props.game.id);
    if (failure) setError(failure);
    else store.pushToast("info", "Refreshing metadata…");
  }

  async function refetchGameArtwork() {
    const failure = await refetchArtwork(props.game.id);
    if (failure) store.pushToast("error", failure);
    else store.pushToast("info", "Fetching artwork…");
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
    <aside
      class="details"
      classList={{ "details--split": props.split === true }}
      aria-label="Game details"
    >
      <div class="details__header">
        <button
          type="button"
          class="icon-btn"
          aria-label="Close details"
          title="Close (Esc)"
          onClick={props.onClose}
        >
          <X class="icon" aria-hidden="true" />
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
                  <Pencil class="icon" aria-hidden="true" />
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
          <button
            type="button"
            class="toggle-btn"
            classList={{ "toggle-btn--on": props.game.favorite }}
            aria-pressed={props.game.favorite}
            aria-label="Favorite"
            title={props.game.favorite ? "Remove from favorites" : "Add to favorites"}
            onClick={() =>
              void run(() => updateGame(props.game.id, { favorite: !props.game.favorite }))
            }
          >
            <Star class="icon icon--filled-when-on" aria-hidden="true" />
          </button>
          <button
            type="button"
            class="toggle-btn"
            classList={{ "toggle-btn--on": props.game.hidden }}
            aria-pressed={props.game.hidden}
            aria-label="Hidden"
            title={props.game.hidden ? "Show in library" : "Hide from library"}
            onClick={() =>
              void run(() => updateGame(props.game.id, { hidden: !props.game.hidden }))
            }
          >
            <Show when={props.game.hidden} fallback={<Eye class="icon" aria-hidden="true" />}>
              <EyeOff class="icon" aria-hidden="true" />
            </Show>
          </button>
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

        <div class="details__row">
          <button type="button" class="btn" onClick={() => void refreshGameMetadata()}>
            Refresh metadata
          </button>
          <button type="button" class="btn" onClick={() => void refetchGameArtwork()}>
            <ImageDown class="icon" aria-hidden="true" /> Refetch artwork
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
          <Show when={canUninstall()}>
            <Show
              when={confirmingUninstall()}
              fallback={
                <button
                  type="button"
                  class="btn btn--danger"
                  ref={setUninstallButton}
                  onClick={askUninstall}
                >
                  <PackageMinus class="icon" aria-hidden="true" /> Uninstall
                </button>
              }
            >
              <div
                role="group"
                aria-label="Confirm uninstall"
                onKeyDown={(event) => {
                  if (event.key === "Escape") {
                    event.preventDefault();
                    event.stopPropagation();
                    cancelUninstall();
                  }
                }}
              >
                <p>Uninstall “{props.game.name}”? The store will ask to confirm.</p>
                <div class="details__row">
                  <button
                    type="button"
                    class="btn btn--danger-solid"
                    onClick={() => void uninstall()}
                  >
                    Uninstall
                  </button>
                  <button
                    type="button"
                    class="btn"
                    ref={setCancelUninstallButton}
                    onClick={cancelUninstall}
                  >
                    Cancel
                  </button>
                </div>
              </div>
            </Show>
          </Show>
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
