import ArrowUpToLine from "lucide-solid/icons/arrow-up-to-line";
import ChevronDown from "lucide-solid/icons/chevron-down";
import Combine from "lucide-solid/icons/combine";
import Ellipsis from "lucide-solid/icons/ellipsis";
import EyeOff from "lucide-solid/icons/eye-off";
import Eye from "lucide-solid/icons/eye";
import ExternalLink from "lucide-solid/icons/external-link";
import FolderOpen from "lucide-solid/icons/folder-open";
import ImageDown from "lucide-solid/icons/image-down";
import PackageMinus from "lucide-solid/icons/package-minus";
import Pencil from "lucide-solid/icons/pencil";
import Play from "lucide-solid/icons/play";
import RefreshCw from "lucide-solid/icons/refresh-cw";
import Star from "lucide-solid/icons/star";
import Trash from "lucide-solid/icons/trash";
import Unlink from "lucide-solid/icons/unlink";
import {
  createEffect,
  createMemo,
  createResource,
  createSignal,
  For,
  on,
  onCleanup,
  onMount,
  Show,
} from "solid-js";
import type { Game } from "../bindings/Game";
import type { GameCopy } from "../bindings/GameCopy";
import type { ListField } from "../bindings/ListField";
import {
  installGame,
  launchGame,
  openInstallFolder,
  openLauncherPage,
  preferCopy,
  refetchArtwork,
  refreshMetadata,
  removeGame,
  separateCopy,
  uninstallGame,
  updateGame,
} from "../lib/actions";
import { copiesOf, launcherPageCopies, playCopy } from "../lib/copies";
import { chipsThatFit } from "../lib/gamePage";
import {
  formatDate,
  formatPlaytime,
  formatRelativeDate,
  formatReleaseDate,
  formatSize,
  releaseYear,
} from "../lib/format";
import {
  gameAchievements,
  gameActivity,
  gameNews,
  gamePlayerCount,
  launchProfiles,
} from "../lib/api";
import { intlLocale, translator } from "../i18n";
import { gamePage } from "../i18n/gamePage";
import { store } from "../lib/store";
import { CategoryEditor } from "./CategoryEditor";
import { CombinePicker } from "./CombinePicker";
import { CompletionPicker } from "./CompletionPicker";
import { PanelHeader, type PanelPin } from "./PanelHeader";
import { GameLinks } from "./GameLinks";
import {
  AchievementsCard,
  ActivityCard,
  Card,
  FilterChip,
  FilterValues,
  NewsCard,
  ScoresCard,
  TimeToBeatCard,
} from "./GameSections";
import { ConfirmDialog } from "./ConfirmDialog";
import { Cover } from "./Cover";
import { EditGameDialog } from "./EditGameDialog";
import { Menu, type MenuEntry } from "./Menu";
import type { PopoverAnchor } from "./Popover";
import { SourceIcon, SourceIcons, sourceName, storeNames } from "./SourceIcon";

const tr = translator(gamePage);

const DESCRIPTION_CLAMP_CHARS = 300;
const DESCRIPTION_CLAMP_LINES = 5;

function isLongText(text: string): boolean {
  return text.length > DESCRIPTION_CLAMP_CHARS || text.split("\n").length > DESCRIPTION_CLAMP_LINES;
}

/** Rows of chips a chip group shows until it is expanded. */
const CHIP_ROWS = 2;

/**
 * A labeled group of filter chips: as many as fit in two rows, then a "+N" toggle that shows
 * them all. A hidden copy of every chip gives their widths at the current layout.
 */
function ChipGroup(props: { label: string; field: ListField; values: string[]; plain?: boolean }) {
  const [all, setAll] = createSignal(false);
  const [fit, setFit] = createSignal(Number.POSITIVE_INFINITY);
  let list: HTMLUListElement | undefined;
  let measure: HTMLDivElement | undefined;

  function refit() {
    if (!list || !measure) return;
    const widths = [...measure.querySelectorAll<HTMLElement>("[data-chip]")].map(
      (chip) => chip.offsetWidth,
    );
    const more = measure.querySelector<HTMLElement>("[data-more]")?.offsetWidth ?? 0;
    const gap = Number.parseFloat(getComputedStyle(list).columnGap) || 0;
    setFit(chipsThatFit(widths, list.clientWidth, more, gap, CHIP_ROWS));
  }

  onMount(() => {
    refit();
    // The list's width, and the copy's: other values or language change what fits.
    const observer = new ResizeObserver(() => refit());
    if (list) observer.observe(list);
    if (measure) observer.observe(measure);
    onCleanup(() => observer.disconnect());
  });
  createEffect(on(() => props.values, refit, { defer: true }));

  const shown = () => (all() ? props.values : props.values.slice(0, fit()));
  const rest = () => props.values.length - shown().length;
  return (
    <Show when={props.values.length > 0}>
      <div class="chip-group">
        <h4 class="details-card__subtitle">{props.label}</h4>
        <div ref={(el) => (measure = el)} class="chips chips--measure" aria-hidden="true">
          <For each={props.values}>
            {(value) => (
              <span data-chip>
                <FilterChip field={props.field} value={value} plain={props.plain} />
              </span>
            )}
          </For>
          <span data-more class="chip chip--plain chip--link">
            +{props.values.length}
          </span>
        </div>
        <ul ref={(el) => (list = el)} class="chips" aria-label={props.label}>
          <For each={shown()}>
            {(value) => (
              <li>
                <FilterChip field={props.field} value={value} plain={props.plain} />
              </li>
            )}
          </For>
          <Show when={rest() > 0 || all()}>
            <li>
              <button
                type="button"
                class="chip chip--plain chip--link"
                aria-expanded={all()}
                onClick={() => setAll((open) => !open)}
              >
                {all() ? tr("fewer") : `+${rest()}`}
              </button>
            </li>
          </Show>
        </ul>
      </div>
    </Show>
  );
}

/** A Details row whose values open Explore; hidden without values. */
type FactKey = Parameters<typeof tr>[0];
function Fact(props: { label: FactKey; plural?: FactKey; field: ListField; values: string[] }) {
  return (
    <Show when={props.values.length > 0}>
      <dt>{props.values.length > 1 && props.plural ? tr(props.plural) : tr(props.label)}</dt>
      <dd>
        <FilterValues field={props.field} values={props.values} />
      </dd>
    </Show>
  );
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
  /** Panel only: docked, floating by choice, or floating because the window is narrow. */
  pin?: PanelPin;
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

/** A Play menu entry's icon: its store, or a play symbol for an executable profile. */
const icon = (source: string | null) => () =>
  source ? <SourceIcon source={source} /> : <Play class="icon" aria-hidden="true" />;

/**
 * A game's page: art, cover, title and the action row on top, then cards (about, details,
 * library). Favorite, hidden and categories change in place; name, cover and launch options
 * in the Edit dialog; rare actions sit in the "More" menu.
 */
export function DetailsPanel(props: DetailsPanelProps) {
  const [error, setError] = createSignal<string | null>(null);
  const [editing, setEditing] = createSignal<"general" | "launch" | null>(null);
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
  const live = <T,>(load: (id: string) => Promise<T>) =>
    createResource(
      () => props.game.id,
      (id) =>
        Promise.resolve()
          .then(() => load(id))
          .catch(() => null),
    )[0];
  const players = live(gamePlayerCount);
  const news = live(gameNews);
  const achievements = live(gameAchievements);
  // Reloaded when a session ends (playtime changes).
  const [activity] = createResource(
    () => [props.game.id, props.game.playtimeSeconds] as const,
    ([id]) =>
      Promise.resolve()
        .then(() => gameActivity(id))
        .catch(() => null),
  );
  // Launch profiles for the Play menu; loaded again after the Edit dialog closes.
  const [profilesVersion, setProfilesVersion] = createSignal(0);
  const [profiles] = createResource(
    () => [props.game.id, profilesVersion()] as const,
    ([id]) =>
      Promise.resolve()
        .then(() => launchProfiles(id))
        .catch(() => null),
  );
  const playProfile = () => {
    const list = profiles();
    return list?.profiles.find((p) => p.id === list.playId) ?? null;
  };
  // The default profile's store copy may not be installed: Play offers to install it then.
  const playable = () => props.game.installed && (playProfile()?.installed ?? true);
  const hasChips = () =>
    props.game.genres.length + props.game.features.length + props.game.tags.length > 0;
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
        setEditing(null);
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
      setEditing("general");
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

  /** Play with a launch profile, or install a store's copy (the menu next to Play). */
  const storeEntries = (): MenuEntry[] => {
    const list = profiles();
    const played: MenuEntry[] = list
      ? list.profiles
          .filter((profile) => profile.installed)
          .map((profile) => ({
            label: profile.name,
            icon: icon(profile.source),
            hint: profile.id === list.playId ? tr("default") : undefined,
            disabled: running(),
            onSelect: () => void run(() => launchGame(props.game.id, null, profile.id)),
          }))
      : copies()
          .filter((copy) => copy.installed)
          .map((copy) => ({
            label: tr("playWith", { store: sourceName(copy.source) }),
            icon: icon(copy.source),
            hint: copy.id === playCopy(props.game)?.id ? tr("default") : undefined,
            disabled: running(),
            onSelect: () => void run(() => launchGame(props.game.id, copy.id)),
          }));
    const installs: MenuEntry[] = copies()
      .filter((copy) => !copy.installed)
      .map((copy) => ({
        label: tr("installWith", { store: sourceName(copy.source) }),
        icon: icon(copy.source),
        onSelect: () => void install(copy.id),
      }));
    return [
      { kind: "heading", label: tr("play") },
      ...played,
      ...(installs.length > 0 ? [{ kind: "separator" } as const, ...installs] : []),
      { kind: "separator" },
      { label: tr("editProfiles"), onSelect: () => setEditing("launch") },
    ];
  };

  const moreEntries = (): MenuEntry[] => [
    {
      label: tr("refreshMetadata"),
      icon: () => <RefreshCw class="icon" aria-hidden="true" />,
      onSelect: () => void refreshGameMetadata(),
    },
    {
      label: tr("refetchArtwork"),
      icon: () => <ImageDown class="icon" aria-hidden="true" />,
      onSelect: () => void refetchGameArtwork(),
    },
    {
      label: tr("combine"),
      icon: () => <Combine class="icon" aria-hidden="true" />,
      // The menu closes first; open the picker where the menu button is.
      onSelect: () =>
        queueMicrotask(() => toggleAnchored(combining, setCombining, moreButton, "end")),
    },
    ...(props.game.installDir === null
      ? []
      : [
          {
            label: tr("openInstallFolder"),
            icon: () => <FolderOpen class="icon" aria-hidden="true" />,
            onSelect: () => void run(() => openInstallFolder(props.game.id)),
          },
        ]),
    ...launcherPageCopies(props.game, store.providers()).map(showInLauncher),
    { kind: "separator" },
    ...(canUninstall()
      ? [
          {
            label: tr("uninstall"),
            icon: () => <PackageMinus class="icon" aria-hidden="true" />,
            danger: true,
            onSelect: () => setConfirming("uninstall"),
          },
        ]
      : []),
    {
      label: tr("removeFromLibrary"),
      icon: () => <Trash class="icon" aria-hidden="true" />,
      danger: true,
      onSelect: () => setConfirming("remove"),
    },
  ];

  const showInLauncher = (copy: GameCopy): MenuEntry => ({
    label: tr("showIn", { store: sourceName(copy.source) }),
    icon: () => <ExternalLink class="icon" aria-hidden="true" />,
    onSelect: () => void run(() => openLauncherPage(props.game.id, copy.id)),
  });

  const copyEntries = (copy: GameCopy): MenuEntry[] => [
    ...(copy.id === copies()[0]?.id
      ? []
      : [
          {
            label: tr("playThisFirst"),
            icon: () => <ArrowUpToLine class="icon" aria-hidden="true" />,
            onSelect: () => void run(() => preferCopy(copy.id)),
          },
        ]),
    {
      label: tr("separate"),
      hint: tr("separateHint"),
      icon: () => <Unlink class="icon" aria-hidden="true" />,
      onSelect: () => void run(() => separateCopy(copy.id)),
    },
    ...launcherPageCopies(props.game, store.providers())
      .filter((c) => c.id === copy.id)
      .map(showInLauncher),
  ];

  async function install(copyId: string | null = null) {
    setError(null);
    const failure = await installGame(props.game.id, copyId);
    if (failure) setError(failure);
    else store.pushToast("info", tr("installHanded"));
  }

  async function uninstall() {
    setError(null);
    const failure = await uninstallGame(props.game.id, uninstallCopy()?.id ?? null);
    if (failure) setError(failure);
    else store.pushToast("info", tr("uninstallHanded"));
  }

  /** Store games ask their store (then IGDB); others only IGDB, which the core may refuse. */
  async function refreshGameMetadata() {
    setError(null);
    const failure = await refreshMetadata(props.game.id);
    if (failure) setError(failure);
    else store.pushToast("info", tr("refreshing"));
  }

  async function refetchGameArtwork() {
    const failure = await refetchArtwork(props.game.id);
    if (failure) store.pushToast("error", failure);
    else store.pushToast("info", tr("fetchingArtwork"));
  }

  const copyState = (copy: GameCopy, index: number) =>
    [
      copy.installed ? tr("installed") : tr("notInstalled"),
      combined() && index === 0 ? tr("preferred") : null,
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
      aria-label={tr("gameDetails")}
    >
      <Show when={!isPage()}>
        <PanelHeader pin={props.pin ?? null} onClose={props.onClose} overlay={heroUrl() !== null} />
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
                  when={playable()}
                  fallback={
                    <button
                      type="button"
                      class="btn btn--primary btn--play"
                      onClick={() =>
                        void install(props.game.installed ? (playProfile()?.copyId ?? null) : null)
                      }
                    >
                      {tr("install")}
                    </button>
                  }
                >
                  <button
                    type="button"
                    class="btn btn--primary btn--play"
                    disabled={running()}
                    onClick={() => void run(() => launchGame(props.game.id))}
                  >
                    {running() ? tr("running") : tr("play")}
                  </button>
                </Show>
                <Show when={combined() || (profiles()?.profiles.length ?? 0) > 1}>
                  <button
                    ref={(el) => (storeMenuButton = el)}
                    type="button"
                    class="btn btn--primary play-split__more"
                    aria-label={tr("chooseHowToPlay")}
                    title={tr("playWithOther")}
                    aria-haspopup="menu"
                    aria-expanded={storeMenu() !== null}
                    onClick={() => toggleAnchored(storeMenu, setStoreMenu, storeMenuButton, "end")}
                  >
                    <ChevronDown class="icon" aria-hidden="true" />
                  </button>
                </Show>
              </div>

              <dl class="details__stats">
                <Show when={players()}>
                  {(count) => (
                    <div class="details__stat details__stat--players">
                      <dt>{tr("playingNow")}</dt>
                      <dd>
                        <span class="players-dot" aria-hidden="true" />
                        {count().toLocaleString(intlLocale())}
                      </dd>
                    </div>
                  )}
                </Show>
                <div class="details__stat">
                  <dt>{tr("lastPlayed")}</dt>
                  <dd>{formatRelativeDate(props.game.lastPlayed)}</dd>
                </div>
                <div class="details__stat">
                  <dt>{tr("playtime")}</dt>
                  <dd>{formatPlaytime(props.game.playtimeSeconds)}</dd>
                </div>
                <Show when={props.game.kind === "game"}>
                  <div class="details__stat">
                    <dt>{tr("status")}</dt>
                    <dd>
                      <CompletionPicker game={props.game} />
                    </dd>
                  </div>
                </Show>
                <div class="details__stat details__stat--stores">
                  <dt>{combined() ? tr("stores") : tr("store")}</dt>
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
                  aria-label={tr("favorite")}
                  title={props.game.favorite ? tr("removeFavorite") : tr("addFavorite")}
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
                  aria-label={tr("hidden")}
                  title={props.game.hidden ? tr("showInLibrary") : tr("hideFromLibrary")}
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
                  title={tr("editTitle")}
                  onClick={() => setEditing("general")}
                >
                  <Pencil class="icon" aria-hidden="true" /> {tr("edit")}
                </button>
                <button
                  ref={(el) => (moreButton = el)}
                  type="button"
                  class="toggle-btn"
                  aria-label={tr("moreActions")}
                  title={tr("moreActions")}
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

        <GameLinks links={props.game.links} />

        <Show when={error()}>
          <p class="error-text details__error" role="alert">
            {error()}
          </p>
        </Show>

        <div class="details__cards">
          <div class="details__col details__col--wide">
            <Show when={description() !== "" || hasChips()}>
              <Card title={tr("about")} order={1} class="details-card--about">
                <Show when={description() !== ""}>
                  <div class="description">
                    <p
                      class="description__text"
                      classList={{
                        "description__text--clamped":
                          isLongText(description()) && !descriptionOpen(),
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
                        {descriptionOpen() ? tr("showLess") : tr("showMore")}
                      </button>
                    </Show>
                  </div>
                </Show>
                <ChipGroup label={tr("genres")} field="genre" values={props.game.genres} />
                <ChipGroup label={tr("features")} field="feature" values={props.game.features} />
                <ChipGroup label={tr("tags")} field="tag" values={props.game.tags} plain />
              </Card>
            </Show>
            <NewsCard news={news()} />
            <Card title={tr("details")} order={7}>
              <dl class="facts">
                <Show when={released()}>
                  {(text) => (
                    <>
                      <dt>{tr("released")}</dt>
                      <dd>{text()}</dd>
                    </>
                  )}
                </Show>
                <Fact
                  label="developer"
                  plural="developers"
                  field="developer"
                  values={props.game.developers}
                />
                <Fact
                  label="publisher"
                  plural="publishers"
                  field="publisher"
                  values={props.game.publishers}
                />
                <Fact label="series" field="series" values={props.game.series} />
                <Fact
                  label="platform"
                  plural="platforms"
                  field="platform"
                  values={props.game.platforms}
                />
                <Fact
                  label="gameMode"
                  plural="gameModes"
                  field="gameMode"
                  values={props.game.gameModes}
                />
                <Fact
                  label="perspective"
                  plural="perspectives"
                  field="perspective"
                  values={props.game.perspectives}
                />
                <Fact label="theme" plural="themes" field="theme" values={props.game.themes} />
                <Fact
                  label="ageRating"
                  plural="ageRatings"
                  field="ageRating"
                  values={props.game.ageRatings}
                />
                <Show when={props.game.installSize}>
                  {(size) => (
                    <>
                      <dt>{tr("installSize")}</dt>
                      <dd>{formatSize(size())}</dd>
                    </>
                  )}
                </Show>
                <dt>{tr("dateAdded")}</dt>
                <dd>{formatDate(props.game.dateAdded)}</dd>
                <dt>{tr("playCount")}</dt>
                <dd>{props.game.playCount}</dd>
              </dl>
            </Card>
          </div>
          <div class="details__col details__col--side">
            <ScoresCard game={props.game} />
            <TimeToBeatCard game={props.game} />
            <AchievementsCard achievements={achievements()} />
            <ActivityCard activity={activity()} />
            <Card title={tr("library")} order={8}>
              <h4 class="details-card__subtitle">{tr("categories")}</h4>
              <CategoryEditor game={props.game} />
              <h4 class="details-card__subtitle">{combined() ? tr("stores") : tr("store")}</h4>
              <ul class="copies" aria-label={tr("stores")}>
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
                          aria-label={tr("actionsOf", { name: sourceName(copy.source) })}
                          title={tr("storeActions")}
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
            </Card>
          </div>
        </div>
      </div>

      <Show when={storeMenu()}>
        {(anchor) => (
          <Menu
            anchor={anchor()}
            label={tr("stores")}
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
            label={tr("moreActions")}
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
            label={tr("actionsOf", { name: sourceName(open().copy.source) })}
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
              store.openCombine([props.game.id, other.id]);
            }}
            onClose={(returnFocus) => {
              setCombining(null);
              if (returnFocus) moreButton?.focus();
            }}
          />
        )}
      </Show>

      <Show when={editing()}>
        {(section) => (
          <EditGameDialog
            game={props.game}
            section={section()}
            onClose={() => {
              setEditing(null);
              setProfilesVersion((v) => v + 1);
            }}
          />
        )}
      </Show>
      <Show when={confirming() === "uninstall"}>
        <ConfirmDialog
          title={tr("uninstallTitle")}
          message={
            combined() && uninstallCopy()
              ? tr("uninstallMessageFrom", {
                  name: props.game.name,
                  store: sourceName(uninstallCopy()?.source ?? ""),
                })
              : tr("uninstallMessage", { name: props.game.name })
          }
          confirmLabel={tr("uninstallLabel")}
          onConfirm={() => void uninstall()}
          onClose={() => setConfirming(null)}
        />
      </Show>
      <Show when={confirming() === "remove"}>
        <ConfirmDialog
          title={tr("removeTitle")}
          message={tr("removeMessage", { name: props.game.name })}
          confirmLabel={tr("removeLabel")}
          onConfirm={() => void run(() => removeGame(props.game.id))}
          onClose={() => setConfirming(null)}
        />
      </Show>
    </aside>
  );
}
