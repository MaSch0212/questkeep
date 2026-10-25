import ArrowUpToLine from "lucide-solid/icons/arrow-up-to-line";
import ChevronDown from "lucide-solid/icons/chevron-down";
import Combine from "lucide-solid/icons/combine";
import Ellipsis from "lucide-solid/icons/ellipsis";
import EyeOff from "lucide-solid/icons/eye-off";
import Eye from "lucide-solid/icons/eye";
import FolderOpen from "lucide-solid/icons/folder-open";
import ImageDown from "lucide-solid/icons/image-down";
import PackageMinus from "lucide-solid/icons/package-minus";
import Pencil from "lucide-solid/icons/pencil";
import RefreshCw from "lucide-solid/icons/refresh-cw";
import Star from "lucide-solid/icons/star";
import Trash from "lucide-solid/icons/trash";
import Unlink from "lucide-solid/icons/unlink";
import X from "lucide-solid/icons/x";
import {
  createEffect,
  createMemo,
  createSignal,
  createUniqueId,
  For,
  on,
  onCleanup,
  onMount,
  Show,
} from "solid-js";
import type { Game } from "../bindings/Game";
import type { GameCopy } from "../bindings/GameCopy";
import {
  combineGames,
  installGame,
  launchGame,
  openInstallFolder,
  preferCopy,
  refetchArtwork,
  refreshMetadata,
  removeGame,
  separateCopy,
  uninstallGame,
  updateGame,
} from "../lib/actions";
import { copiesOf, playCopy } from "../lib/copies";
import {
  formatDate,
  formatPlaytime,
  formatRelativeDate,
  formatReleaseDate,
  releaseYear,
} from "../lib/format";
import { store } from "../lib/store";
import { CategoryEditor } from "./CategoryEditor";
import { CombinePicker } from "./CombinePicker";
import { ConfirmDialog } from "./ConfirmDialog";
import { Cover } from "./Cover";
import { EditGameDialog } from "./EditGameDialog";
import { Menu, type MenuEntry } from "./Menu";
import type { PopoverAnchor } from "./Popover";
import { SourceIcon, SourceIcons, sourceName, storeNames } from "./SourceIcon";

const DESCRIPTION_CLAMP_CHARS = 300;
const DESCRIPTION_CLAMP_LINES = 5;

function isLongText(text: string): boolean {
  return text.length > DESCRIPTION_CLAMP_CHARS || text.split("\n").length > DESCRIPTION_CLAMP_LINES;
}

interface DetailsPanelProps {
  game: Game;
  /** Shown beside the library instead of overlaying it. */
  split?: boolean;
  /**
   * "panel" (default): the side panel / flyout with a close button. "page": the full-size page
   * of the details view, which is always shown, so it has no close button.
   */
  variant?: "panel" | "page";
  onClose: () => void;
}

/** Opens a popup under `button`, or closes it when it is open. */
function toggleAnchored(
  current: () => PopoverAnchor | null,
  set: (anchor: PopoverAnchor | null) => void,
  button: HTMLButtonElement | undefined,
  align: "start" | "end",
) {
  const rect = button?.getBoundingClientRect();
  if (rect) set(current() ? null : { rect, align });
}

type Confirming = "uninstall" | "remove" | null;

/**
 * A game's page: art, cover, title and the action row on top, then cards (about, details,
 * library). Favorite, hidden and categories change in place; name, cover and launch options
 * in the Edit dialog; rare actions sit in the "More" menu.
 */
export function DetailsPanel(props: DetailsPanelProps) {
  const [error, setError] = createSignal<string | null>(null);
  const [editing, setEditing] = createSignal(false);
  const [confirming, setConfirming] = createSignal<Confirming>(null);
  const [descriptionOpen, setDescriptionOpen] = createSignal(false);
  const [heroFailed, setHeroFailed] = createSignal(false);

  const [storeMenu, setStoreMenu] = createSignal<PopoverAnchor | null>(null);
  const [moreMenu, setMoreMenu] = createSignal<PopoverAnchor | null>(null);
  const [copyMenu, setCopyMenu] = createSignal<{ copy: GameCopy; anchor: PopoverAnchor } | null>(
    null,
  );
  const [combining, setCombining] = createSignal<PopoverAnchor | null>(null);
  let storeMenuButton: HTMLButtonElement | undefined;
  let moreButton: HTMLButtonElement | undefined;
  const copyButtons = new Map<string, HTMLButtonElement>();

  const aboutId = createUniqueId();
  const detailsId = createUniqueId();
  const libraryId = createUniqueId();

  const running = () => store.isRunning(props.game.id);
  const copies = () => copiesOf(props.game);
  const combined = () => copies().length > 1;
  // Uninstall acts on the copy Play starts.
  const uninstallCopy = () => {
    const copy = playCopy(props.game);
    return copy?.installed ? copy : undefined;
  };
  const canUninstall = () => {
    const copy = uninstallCopy();
    return (
      copy !== undefined &&
      copy.sourceGameId !== null &&
      store.providers().some((p) => p.id === copy.source && p.canUninstall)
    );
  };
  const isPage = () => props.variant === "page";
  const heroUrl = () => (heroFailed() ? null : store.backgroundUrl(props.game));
  const released = () => formatReleaseDate(props.game.releaseDate);
  const description = () => props.game.description?.trim() ?? "";
  const subtitle = () =>
    [props.game.developers[0], releaseYear(props.game.releaseDate)]
      .filter((part) => part !== undefined && part !== null && part !== "")
      .join(" · ");

  // A memo, so list refreshes (a new game object with the same id, every few seconds while covers
  // download) don't count as switching games and close open dialogs and menus.
  const gameId = createMemo(() => props.game.id);
  createEffect(
    on(
      gameId,
      () => {
        setError(null);
        setEditing(false);
        setConfirming(null);
        setMoreMenu(null);
        setCopyMenu(null);
        setCombining(null);
        setDescriptionOpen(false);
        setHeroFailed(false);
      },
      { defer: true },
    ),
  );

  function onKey(event: KeyboardEvent) {
    if (event.key === "F2" && !editing()) {
      event.preventDefault();
      setEditing(true);
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

  /** Play or install with one store's copy (the store menu next to Play). */
  const storeEntries = (): MenuEntry[] => [
    { kind: "heading", label: "Stores" },
    ...copies().map((copy): MenuEntry => {
      const name = sourceName(copy.source);
      return copy.installed
        ? {
            label: `Play with ${name}`,
            icon: () => <SourceIcon source={copy.source} />,
            hint: copy.id === playCopy(props.game)?.id ? "Default" : undefined,
            disabled: running(),
            onSelect: () => void run(() => launchGame(props.game.id, copy.id)),
          }
        : {
            label: `Install with ${name}`,
            icon: () => <SourceIcon source={copy.source} />,
            onSelect: () => void install(copy.id),
          };
    }),
  ];

  const moreEntries = (): MenuEntry[] => [
    {
      label: "Refresh metadata",
      icon: () => <RefreshCw class="icon" aria-hidden="true" />,
      onSelect: () => void refreshGameMetadata(),
    },
    {
      label: "Refetch artwork",
      icon: () => <ImageDown class="icon" aria-hidden="true" />,
      onSelect: () => void refetchGameArtwork(),
    },
    {
      label: "Combine with another game…",
      icon: () => <Combine class="icon" aria-hidden="true" />,
      // The menu closes first; open the picker where the menu button is.
      onSelect: () =>
        queueMicrotask(() => toggleAnchored(combining, setCombining, moreButton, "end")),
    },
    ...(props.game.installDir === null
      ? []
      : [
          {
            label: "Open install folder",
            icon: () => <FolderOpen class="icon" aria-hidden="true" />,
            onSelect: () => void run(() => openInstallFolder(props.game.id)),
          },
        ]),
    { kind: "separator" },
    ...(canUninstall()
      ? [
          {
            label: "Uninstall…",
            icon: () => <PackageMinus class="icon" aria-hidden="true" />,
            danger: true,
            onSelect: () => setConfirming("uninstall"),
          },
        ]
      : []),
    {
      label: "Remove from library…",
      icon: () => <Trash class="icon" aria-hidden="true" />,
      danger: true,
      onSelect: () => setConfirming("remove"),
    },
  ];

  const copyEntries = (copy: GameCopy): MenuEntry[] => [
    ...(copy.id === copies()[0]?.id
      ? []
      : [
          {
            label: "Play this store first",
            icon: () => <ArrowUpToLine class="icon" aria-hidden="true" />,
            onSelect: () => void run(() => preferCopy(copy.id)),
          },
        ]),
    {
      label: "Separate",
      hint: "Its own game again",
      icon: () => <Unlink class="icon" aria-hidden="true" />,
      onSelect: () => void run(() => separateCopy(copy.id)),
    },
  ];

  async function install(copyId: string | null = null) {
    setError(null);
    const failure = await installGame(props.game.id, copyId);
    if (failure) setError(failure);
    else store.pushToast("info", "Install handed to the store client");
  }

  async function uninstall() {
    setError(null);
    const failure = await uninstallGame(props.game.id, uninstallCopy()?.id ?? null);
    if (failure) setError(failure);
    else store.pushToast("info", "Uninstall handed to the store client");
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

  const copyState = (copy: GameCopy, index: number) =>
    [
      copy.installed ? "Installed" : "Not installed",
      combined() && index === 0 ? "preferred" : null,
      combined() && copy.playtimeSeconds > 0 ? formatPlaytime(copy.playtimeSeconds) : null,
    ]
      .filter((part) => part !== null)
      .join(" · ");

  return (
    <aside
      class="details"
      classList={{
        "details--split": props.split === true,
        "details--page": isPage(),
      }}
      aria-label="Game details"
    >
      <Show when={!isPage()}>
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
      </Show>

      <div class="details__scroll">
        <Show when={heroUrl()}>
          {(src) => (
            <div class="details__hero">
              <img src={src()} alt="" draggable={false} onError={() => setHeroFailed(true)} />
            </div>
          )}
        </Show>

        <div class="details__head" classList={{ "details__head--overlap": heroUrl() !== null }}>
          <div class="details__cover">
            <Cover game={props.game} />
          </div>
          <div class="details__intro">
            <h2 class="details__title">{props.game.name}</h2>
            <Show when={subtitle()}>
              <p class="details__subtitle">{subtitle()}</p>
            </Show>
            <div class="details__actions">
              <div class="play-split">
                <Show
                  when={props.game.installed}
                  fallback={
                    <button
                      type="button"
                      class="btn btn--primary btn--play"
                      onClick={() => void install()}
                    >
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
                <Show when={combined()}>
                  <button
                    ref={(el) => (storeMenuButton = el)}
                    type="button"
                    class="btn btn--primary play-split__more"
                    aria-label="Choose a store"
                    title="Play or install with another store"
                    aria-haspopup="menu"
                    aria-expanded={storeMenu() !== null}
                    onClick={() => toggleAnchored(storeMenu, setStoreMenu, storeMenuButton, "end")}
                  >
                    <ChevronDown class="icon" aria-hidden="true" />
                  </button>
                </Show>
              </div>

              <dl class="details__stats">
                <div class="details__stat">
                  <dt>Last played</dt>
                  <dd>{formatRelativeDate(props.game.lastPlayed)}</dd>
                </div>
                <div class="details__stat">
                  <dt>Playtime</dt>
                  <dd>{formatPlaytime(props.game.playtimeSeconds)}</dd>
                </div>
                <div class="details__stat">
                  <dt>{combined() ? "Stores" : "Store"}</dt>
                  <dd class="details__stores">
                    <SourceIcons game={props.game} />
                    <span>{storeNames(props.game)}</span>
                  </dd>
                </div>
              </dl>

              <div class="details__tools">
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
                <button
                  type="button"
                  class="btn"
                  aria-haspopup="dialog"
                  title="Edit name, cover and launch options (F2)"
                  onClick={() => setEditing(true)}
                >
                  <Pencil class="icon" aria-hidden="true" /> Edit
                </button>
                <button
                  ref={(el) => (moreButton = el)}
                  type="button"
                  class="toggle-btn"
                  aria-label="More actions"
                  title="More actions"
                  aria-haspopup="menu"
                  aria-expanded={moreMenu() !== null}
                  onClick={() => toggleAnchored(moreMenu, setMoreMenu, moreButton, "end")}
                >
                  <Ellipsis class="icon" aria-hidden="true" />
                </button>
              </div>
            </div>
          </div>
        </div>

        <Show when={error()}>
          <p class="error-text details__error" role="alert">
            {error()}
          </p>
        </Show>

        <div class="details__cards">
          <Show when={description() !== "" || props.game.genres.length > 0}>
            <section class="details-card details-card--about" aria-labelledby={aboutId}>
              <h3 id={aboutId} class="details-card__title">
                About
              </h3>
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
            </section>
          </Show>

          <section class="details-card" aria-labelledby={detailsId}>
            <h3 id={detailsId} class="details-card__title">
              Details
            </h3>
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
              <dt>Date added</dt>
              <dd>{formatDate(props.game.dateAdded)}</dd>
              <dt>Play count</dt>
              <dd>{props.game.playCount}</dd>
            </dl>
          </section>

          <section class="details-card" aria-labelledby={libraryId}>
            <h3 id={libraryId} class="details-card__title">
              Library
            </h3>
            <h4 class="details-card__subtitle">Categories</h4>
            <CategoryEditor game={props.game} />
            <h4 class="details-card__subtitle">{combined() ? "Stores" : "Store"}</h4>
            <ul class="copies" aria-label="Stores">
              <For each={copies()}>
                {(copy, index) => (
                  <li class="copies__row">
                    <SourceIcon source={copy.source} />
                    <span class="copies__name">{sourceName(copy.source)}</span>
                    <span class="copies__state">{copyState(copy, index())}</span>
                    <Show when={combined()}>
                      <button
                        ref={(el) => copyButtons.set(copy.id, el)}
                        type="button"
                        class="icon-btn icon-btn--small"
                        aria-label={`${sourceName(copy.source)} actions`}
                        title="Store actions"
                        aria-haspopup="menu"
                        aria-expanded={copyMenu()?.copy.id === copy.id}
                        onClick={(event) => {
                          const rect = event.currentTarget.getBoundingClientRect();
                          setCopyMenu(
                            copyMenu()?.copy.id === copy.id
                              ? null
                              : { copy, anchor: { rect, align: "end" } },
                          );
                        }}
                      >
                        <Ellipsis class="icon" aria-hidden="true" />
                      </button>
                    </Show>
                  </li>
                )}
              </For>
            </ul>
            <Show when={props.game.installDir}>
              {(dir) => (
                <p class="details__path" title={dir()}>
                  {dir()}
                </p>
              )}
            </Show>
          </section>
        </div>
      </div>

      <Show when={storeMenu()}>
        {(anchor) => (
          <Menu
            anchor={anchor()}
            label="Stores"
            inside={() => [storeMenuButton]}
            onClose={(returnFocus) => {
              setStoreMenu(null);
              if (returnFocus) storeMenuButton?.focus();
            }}
            entries={storeEntries()}
          />
        )}
      </Show>
      <Show when={moreMenu()}>
        {(anchor) => (
          <Menu
            anchor={anchor()}
            label="More actions"
            inside={() => [moreButton]}
            onClose={(returnFocus) => {
              setMoreMenu(null);
              if (returnFocus) moreButton?.focus();
            }}
            entries={moreEntries()}
          />
        )}
      </Show>
      <Show when={copyMenu()}>
        {(open) => (
          <Menu
            anchor={open().anchor}
            label={`${sourceName(open().copy.source)} actions`}
            inside={() => [copyButtons.get(open().copy.id)]}
            onClose={(returnFocus) => {
              const button = copyButtons.get(open().copy.id);
              setCopyMenu(null);
              if (returnFocus) button?.focus();
            }}
            entries={copyEntries(open().copy)}
          />
        )}
      </Show>
      <Show when={combining()}>
        {(anchor) => (
          <CombinePicker
            game={props.game}
            anchor={anchor()}
            inside={() => [moreButton]}
            onPick={(other) => {
              setCombining(null);
              void run(() => combineGames(props.game.id, [other.id]));
            }}
            onClose={(returnFocus) => {
              setCombining(null);
              if (returnFocus) moreButton?.focus();
            }}
          />
        )}
      </Show>

      <Show when={editing()}>
        <EditGameDialog game={props.game} onClose={() => setEditing(false)} />
      </Show>
      <Show when={confirming() === "uninstall"}>
        <ConfirmDialog
          title="Uninstall game"
          message={
            <>
              Uninstall “{props.game.name}”
              <Show when={combined() && uninstallCopy()}>
                {(copy) => <> from {sourceName(copy().source)}</>}
              </Show>
              ? The store will ask to confirm.
            </>
          }
          confirmLabel="Uninstall"
          onConfirm={() => void uninstall()}
          onClose={() => setConfirming(null)}
        />
      </Show>
      <Show when={confirming() === "remove"}>
        <ConfirmDialog
          title="Remove game"
          message={<>Remove “{props.game.name}” from your library?</>}
          confirmLabel="Remove"
          onConfirm={() => void run(() => removeGame(props.game.id))}
          onClose={() => setConfirming(null)}
        />
      </Show>
    </aside>
  );
}
