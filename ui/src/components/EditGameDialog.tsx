import {
  createResource,
  createSignal,
  For,
  Match,
  onMount,
  Show,
  Switch,
  type JSX,
} from "solid-js";
import { Portal } from "solid-js/web";
import type { Game } from "../bindings/Game";
import type { GameKind } from "../bindings/GameKind";
import type { GameEditInfo } from "../bindings/GameEditInfo";
import type { GameLink } from "../bindings/GameLink";
import type { GameMetadata } from "../bindings/GameMetadata";
import type { ImageSlot } from "../bindings/ImageSlot";
import type { LaunchProfiles } from "../bindings/LaunchProfiles";
import type { MetadataField } from "../bindings/MetadataField";
import { refreshMetadata, updateGame } from "../lib/actions";
import * as api from "../lib/api";
import {
  buildEdits,
  copyField,
  editedCount,
  gigabytesText,
  hoursText,
  LIST_FIELDS,
  parseGigabytes,
  parseHours,
  parseScore,
  SECTION_FIELDS,
  shortUrl,
  sourceLabel,
  sourceName,
  startValues,
} from "../lib/editGame";
import { errorMessage } from "../lib/errors";
import { pickGameImage } from "../lib/pickers";
import { store } from "../lib/store";
import { ChipInput } from "./ChipInput";
import { Cover } from "./Cover";
import { EditLaunchSection } from "./EditLaunchSection";
import { ImagePicker, imageSrc } from "./ImagePicker";
import { Modal } from "./Modal";
import { sourceName as storeName } from "./SourceIcon";

type Section = "general" | "details" | "links" | "launch" | "tracking" | "images";

const KINDS: readonly (readonly [GameKind, string])[] = [
  ["game", "Game"],
  ["app", "App"],
];

const SECTIONS: { id: Section; label: string }[] = [
  { id: "general", label: "General" },
  { id: "details", label: "Details" },
  { id: "links", label: "Links" },
  { id: "launch", label: "Launch" },
  { id: "tracking", label: "Playtime tracking" },
  { id: "images", label: "Images" },
];

const RELEASE_DATE = /^\d{4}(-\d{2}(-\d{2})?)?$/;

const nameOf = (id: string) => storeName(id);

/** The shown address of a game's image, fresh after it changed. */
function urlOf(game: Game, slot: ImageSlot): string | null {
  if (slot === "cover") return store.coverUrl(game);
  return slot === "icon" ? store.iconUrl(game) : store.backgroundUrl(game);
}

/** The id of a metadata field's input, for its label. */
const fieldId = (field: MetadataField) => `edit-field-${field}`;

/** The label row of a metadata field: its name and where the value comes from. */
function FieldLabel(props: {
  label: string;
  field: MetadataField;
  info: GameEditInfo;
  edited: boolean;
  onReset: () => void;
}) {
  const source = () => props.info.automaticSources[props.field];
  // The source note sits next to the <label>, not inside it: it can hold a button.
  return (
    <span class="field__label edit-field__label">
      <label for={fieldId(props.field)}>{props.label}</label>
      <Show
        when={props.edited}
        fallback={<span class="edit-field__source">{sourceLabel(source(), nameOf)}</span>}
      >
        <span class="edit-field__source edit-field__source--user">
          edited by you ·{" "}
          <button type="button" class="link-btn" onClick={() => props.onReset()}>
            {source() === undefined ? "Clear" : `Use ${sourceName(source(), nameOf)} value`}
          </button>
        </span>
      </Show>
    </span>
  );
}

/** One image row of the Images section. */
function ImageRow(props: {
  game: Game;
  slot: ImageSlot;
  title: string;
  hint: string;
  path: string | null;
  user: boolean;
  onChoose: () => void;
  onPick: () => void;
  onRemove: () => void;
}) {
  return (
    <div class={`image-row image-row--${props.slot}`}>
      <div class="image-row__frame">
        <Show
          when={props.slot === "cover"}
          fallback={
            <Show when={props.path} fallback={<span class="muted">None</span>}>
              {(path) => (
                <img
                  class="image-row__img"
                  src={urlOf(props.game, props.slot) ?? imageSrc(path())}
                  alt=""
                />
              )}
            </Show>
          }
        >
          <div class="image-row__cover">
            <Cover game={props.game} />
          </div>
        </Show>
      </div>
      <div class="image-row__info">
        <b>{props.title}</b>
        <span class="field__hint">
          {props.hint} {props.user ? "Chosen by you." : ""}
        </span>
        <div class="image-row__buttons">
          <button type="button" class="btn" onClick={() => props.onChoose()}>
            Choose file…
          </button>
          <button type="button" class="btn" onClick={() => props.onPick()}>
            Pick image…
          </button>
          <button
            type="button"
            class="btn btn--ghost"
            disabled={props.slot === "icon" ? !props.user : props.path === null}
            onClick={() => props.onRemove()}
          >
            {props.slot === "icon" ? "Reset" : "Remove"}
          </button>
        </div>
      </div>
    </div>
  );
}

/**
 * The Edit game dialog: every metadata field (edits are kept through refreshes), links, launch
 * profiles, playtime tracking and images, in sections listed on the left. Metadata, name,
 * "You" fields and tracking are drafts until Save; images and profiles apply at once.
 */
export function EditGameDialog(props: {
  game: Game;
  onClose: () => void;
  /** Section to show first. */
  section?: Section;
}) {
  const game = () => props.game;
  const id = props.game.id;
  const [section, setSection] = createSignal<Section>(props.section ?? "general");
  const [info] = createResource(() => api.gameEditInfo(id));
  const [profiles, setProfiles] = createSignal<LaunchProfiles>();
  onMount(() => {
    Promise.resolve()
      .then(() => api.launchProfiles(id))
      .then(setProfiles)
      .catch(() => undefined);
  });

  // Drafts.
  const [name, setName] = createSignal(props.game.name);
  const [sortingName, setSortingName] = createSignal<string | null>(null);
  const [values, setValues] = createSignal<GameMetadata | null>(null);
  const [edited, setEdited] = createSignal<ReadonlySet<MetadataField>>(new Set());
  const [ownLinks, setOwnLinks] = createSignal<GameLink[]>([]);
  const [hidden, setHidden] = createSignal<string[]>([]);
  const [completion, setCompletion] = createSignal(props.game.completionStatus);
  const [kind, setKind] = createSignal(props.game.kind);
  const [score, setScore] = createSignal<number | null>(props.game.userScore);
  const [notes, setNotes] = createSignal(props.game.notes ?? "");
  const [categoryNames, setCategoryNames] = createSignal(
    props.game.categories
      .map((cid) => store.categories().find((c) => c.id === cid)?.name)
      .filter((n): n is string => n !== undefined),
  );
  const [detectLaunches, setDetectLaunches] = createSignal(props.game.detectLaunches);
  const [ignored, setIgnored] = createSignal(props.game.ignoredProcesses.join("\n"));
  // Number fields as typed.
  const [numbers, setNumbers] = createSignal<Record<string, string>>({});
  const [newLink, setNewLink] = createSignal({ name: "", url: "" });
  const [editingLink, setEditingLink] = createSignal<number | null>(null);

  const [picker, setPicker] = createSignal<ImageSlot | null>(null);
  const [error, setError] = createSignal<string | null>(null);
  const [saving, setSaving] = createSignal(false);

  // Start the drafts once the edit info arrives.
  const ready = () => {
    const loaded = info();
    if (loaded && values() === null) {
      const start = startValues(loaded);
      setValues(start);
      setEdited(new Set(loaded.edits.edited));
      setOwnLinks(loaded.edits.meta.links);
      setHidden(loaded.edits.hiddenLinks);
      setSortingName(loaded.sortingName);
      setNumbers({
        criticScore: start.criticScore ? String(start.criticScore.value) : "",
        communityScore: start.communityScore ? String(start.communityScore.value) : "",
        ttbMain: hoursText(start.timeToBeat?.main),
        ttbExtra: hoursText(start.timeToBeat?.extra),
        ttbComplete: hoursText(start.timeToBeat?.complete),
        installSize: gigabytesText(start.installSize),
      });
      for (const f of LIST_FIELDS) void store.loadValues(f.values);
    }
    return loaded !== undefined && values() !== null;
  };

  function setField<K extends MetadataField>(field: K, value: GameMetadata[K]) {
    setValues((current) => (current ? { ...current, [field]: value } : current));
    setEdited((current) => new Set([...current, field]));
  }

  const setNumber = (key: string, text: string) => setNumbers((n) => ({ ...n, [key]: text }));

  function resetField(field: MetadataField) {
    const loaded = info();
    const current = values();
    if (!loaded || !current) return;
    const next = { ...current };
    copyField(field, loaded.automatic, next);
    setValues(next);
    setEdited((all) => new Set([...all].filter((f) => f !== field)));
    const auto = loaded.automatic;
    const set = setNumber;
    if (field === "criticScore")
      set("criticScore", auto.criticScore ? String(auto.criticScore.value) : "");
    if (field === "communityScore")
      set("communityScore", auto.communityScore ? String(auto.communityScore.value) : "");
    if (field === "installSize") set("installSize", gigabytesText(auto.installSize));
    if (field === "timeToBeat") {
      set("ttbMain", hoursText(auto.timeToBeat?.main));
      set("ttbExtra", hoursText(auto.timeToBeat?.extra));
      set("ttbComplete", hoursText(auto.timeToBeat?.complete));
    }
  }

  const isEdited = (field: MetadataField) => edited().has(field);
  const label = (text: string, field: MetadataField) => (
    <FieldLabel
      label={text}
      field={field}
      info={info() as GameEditInfo}
      edited={isEdited(field)}
      onReset={() => resetField(field)}
    />
  );

  /** Reads the number fields into the draft; returns an error message for bad input. */
  function readNumbers(): string | null {
    const n = numbers();
    const current = values();
    if (!current) return null;
    const scoreOf = (key: "criticScore" | "communityScore", title: string) => {
      if (!isEdited(key)) return null;
      const value = parseScore(n[key] ?? "");
      if (value === undefined) return `${title} must be a whole number from 0 to 100.`;
      const old = current[key];
      setField(key, value === null ? null : { value, source: old?.source ?? "Your score" });
      return null;
    };
    const failed =
      scoreOf("criticScore", "Critic score") ?? scoreOf("communityScore", "Community score");
    if (failed) return failed;
    if (isEdited("timeToBeat")) {
      const main = parseHours(n.ttbMain ?? "");
      const extra = parseHours(n.ttbExtra ?? "");
      const complete = parseHours(n.ttbComplete ?? "");
      if (main === undefined || extra === undefined || complete === undefined) {
        return "Enter time to beat in hours, e.g. 28 or 28.5.";
      }
      const empty = main === null && extra === null && complete === null;
      setField("timeToBeat", empty ? null : { main, extra, complete });
    }
    if (isEdited("installSize")) {
      const size = parseGigabytes(n.installSize ?? "");
      if (size === undefined) return "Enter the install size in GB, e.g. 9.4.";
      setField("installSize", size);
    }
    const date = current.releaseDate?.trim() ?? "";
    if (isEdited("releaseDate") && date !== "" && !RELEASE_DATE.test(date)) {
      return "Enter the release date as YYYY-MM-DD, YYYY-MM or YYYY.";
    }
    return null;
  }

  /** Changes plain game fields; throws the error message. */
  async function patch(update: Parameters<typeof updateGame>[1]) {
    const failed = await updateGame(id, update);
    if (failed) throw new Error(failed);
  }

  async function save() {
    const trimmed = name().trim();
    if (trimmed === "") {
      setSection("general");
      setError("Enter a name for the game.");
      return;
    }
    const failedInput = readNumbers();
    if (failedInput) {
      setError(failedInput);
      return;
    }
    setError(null);
    setSaving(true);
    try {
      const current = values();
      const steps: (() => Promise<unknown>)[] = [];
      if (trimmed !== props.game.name) steps.push(() => patch({ name: trimmed }));
      if (kind() !== props.game.kind) steps.push(() => patch({ kind: kind() }));
      if (
        detectLaunches() !== props.game.detectLaunches ||
        ignoredList().join("\n") !== props.game.ignoredProcesses.join("\n")
      ) {
        steps.push(() =>
          patch({ detectLaunches: detectLaunches(), ignoredProcesses: ignoredList() }),
        );
      }
      if (current) {
        const date = current.releaseDate?.trim() ?? "";
        const meta = { ...current, releaseDate: date === "" ? null : date };
        steps.push(() =>
          api.saveGameEdits(id, {
            edits: buildEdits(meta, edited(), ownLinks(), hidden()),
            sortingName: sortingName(),
            userScore: score(),
            notes: notes(),
          }),
        );
      }
      if (kind() === "game" && completion() !== props.game.completionStatus) {
        steps.push(() => store.setCompletionStatus([id], completion()));
      }
      const wanted = await categoryIds();
      if (wanted.toSorted().join() !== props.game.categories.toSorted().join()) {
        steps.push(() => store.setGameCategories(id, wanted));
      }
      // In order: a failing step stops the rest.
      await steps.reduce<Promise<unknown>>(
        (previous, step) => previous.then(step),
        Promise.resolve(),
      );
      store.upsertGame(await api.getGame(id));
      props.onClose();
    } catch (failure) {
      setError(errorMessage(failure));
    } finally {
      setSaving(false);
    }
  }

  const ignoredList = () =>
    ignored()
      .split("\n")
      .map((line) => line.trim())
      .filter((line) => line !== "");

  /** The chosen categories as ids; new names become new categories. */
  async function categoryIds(): Promise<string[]> {
    const found = await Promise.all(
      categoryNames().map(async (categoryName) => {
        const key = categoryName.toLocaleLowerCase();
        const known = store.categories().find((c) => c.name.toLocaleLowerCase() === key);
        return known ?? (await store.createCategory(categoryName));
      }),
    );
    return found.flatMap((category) => (category ? [category.id] : []));
  }

  async function imageAction(work: () => Promise<Game>) {
    setError(null);
    try {
      store.upsertGame(await work());
    } catch (failure) {
      setError(errorMessage(failure));
    }
  }

  async function chooseFile(slot: ImageSlot) {
    const path = await pickGameImage(slot === "icon");
    if (path !== null) await imageAction(() => api.setUserImage(id, slot, path));
  }

  const pathOf = (slot: ImageSlot) =>
    ({ cover: game().coverPath, background: game().backgroundPath, icon: game().iconPath })[slot];
  const userImage = (slot: ImageSlot) => {
    const loaded = info();
    if (!loaded) return false;
    return loaded.userImages[slot] || userChanged().has(slot);
  };
  const [userChanged, setUserChanged] = createSignal<ReadonlySet<ImageSlot>>(new Set());
  const markUser = (slot: ImageSlot, on: boolean) =>
    setUserChanged((all) => {
      const next = new Set(all);
      if (on) next.add(slot);
      else next.delete(slot);
      return next;
    });

  const sectionCount = (s: Section): string | null => {
    if (s === "general" || s === "details") {
      const count = editedCount(SECTION_FIELDS[s], edited());
      return count > 0 ? `${count} edited` : null;
    }
    if (s === "launch") {
      const count = profiles()?.profiles.length ?? 0;
      return count > 1 ? String(count) : null;
    }
    return null;
  };

  const numberInput = (
    key: string,
    labelText: JSX.Element,
    field: MetadataField,
    hint?: string,
  ) => (
    <div class="field">
      {labelText}
      <input
        id={fieldId(field)}
        class="input"
        classList={{ "input--edited": isEdited(field) }}
        inputmode="decimal"
        value={numbers()[key] ?? ""}
        placeholder={hint}
        onInput={(event) => {
          const text = event.currentTarget.value;
          setNumbers((n) => ({ ...n, [key]: text }));
          setEdited((all) => new Set([...all, field]));
        }}
      />
    </div>
  );

  const links = () => {
    const loaded = info();
    if (!loaded) return [];
    return loaded.links.filter((l) => l.source !== "user");
  };
  const isHidden = (url: string) => hidden().includes(url);

  function addLink(event: SubmitEvent) {
    event.preventDefault();
    event.stopPropagation();
    const link = newLink();
    if (link.url.trim() === "") return;
    const index = editingLink();
    if (index === null) setOwnLinks((all) => [...all, { name: link.name, url: link.url }]);
    else setOwnLinks((all) => all.map((l, i) => (i === index ? { ...link } : l)));
    setNewLink({ name: "", url: "" });
    setEditingLink(null);
  }

  return (
    <Modal title="Edit game" wide onClose={props.onClose}>
      <div class="edit-game">
        <div class="edit-game__main">
          <nav class="edit-game__nav" aria-label="Sections">
            <For each={SECTIONS}>
              {(entry) => (
                <button
                  type="button"
                  class="edit-game__tab"
                  aria-current={section() === entry.id ? "page" : undefined}
                  onClick={() => setSection(entry.id)}
                >
                  {entry.label}
                  <Show when={sectionCount(entry.id)}>
                    <small>{sectionCount(entry.id)}</small>
                  </Show>
                </button>
              )}
            </For>
          </nav>
          <div class="edit-game__pane">
            <Switch>
              <Match when={section() === "launch"}>
                <EditLaunchSection game={game()} profiles={profiles()} onChange={setProfiles} />
              </Match>
              <Match when={section() === "tracking"}>
                <div class="edit-section">
                  <label class="check">
                    <input
                      type="checkbox"
                      checked={detectLaunches()}
                      onChange={(event) => setDetectLaunches(event.currentTarget.checked)}
                    />
                    <span>Record playtime when started outside Questkeep</span>
                  </label>
                  <label class="field">
                    <span class="field__label">Ignored programs</span>
                    <textarea
                      class="input input--multiline"
                      rows={4}
                      placeholder={"Launcher.exe\n*patcher*"}
                      value={ignored()}
                      aria-describedby="ignored-programs-hint"
                      onInput={(event) => setIgnored(event.currentTarget.value)}
                    />
                  </label>
                  <p class="field__hint" id="ignored-programs-hint">
                    One per line. While only these run, the game doesn't count as playing. Use * as
                    a wildcard.
                  </p>
                </div>
              </Match>
              <Match when={section() === "images"}>
                <div class="edit-section">
                  <ImageRow
                    game={game()}
                    slot="cover"
                    title="Cover"
                    hint="Grid view and game page."
                    path={game().coverPath}
                    user={userImage("cover")}
                    onChoose={() => void chooseFile("cover").then(() => markUser("cover", true))}
                    onPick={() => setPicker("cover")}
                    onRemove={() =>
                      void imageAction(() => api.removeUserImage(id, "cover")).then(() =>
                        markUser("cover", true),
                      )
                    }
                  />
                  <ImageRow
                    game={game()}
                    slot="background"
                    title="Background"
                    hint="Behind the game page header."
                    path={game().backgroundPath}
                    user={userImage("background")}
                    onChoose={() =>
                      void chooseFile("background").then(() => markUser("background", true))
                    }
                    onPick={() => setPicker("background")}
                    onRemove={() =>
                      void imageAction(() => api.removeUserImage(id, "background")).then(() =>
                        markUser("background", true),
                      )
                    }
                  />
                  <ImageRow
                    game={game()}
                    slot="icon"
                    title="Icon"
                    hint="List view, tray and command bar."
                    path={game().iconPath}
                    user={userImage("icon") && game().iconPath !== null}
                    onChoose={() => void chooseFile("icon").then(() => markUser("icon", true))}
                    onPick={() => setPicker("icon")}
                    onRemove={() =>
                      void imageAction(() => api.removeUserImage(id, "icon")).then(() =>
                        markUser("icon", false),
                      )
                    }
                  />
                  <p class="field__hint">
                    Files: .png, .jpg, .webp; icons also .ico, .exe or .dll. "Pick image…" shows
                    images from the stores, IGDB and SteamGridDB (each when available) and a web
                    search. Your images stay through metadata refreshes; Remove keeps the image
                    empty, Reset makes the icon again from the store or the executable.
                  </p>
                </div>
              </Match>
              <Match when={!ready()}>
                <p class="muted">Loading…</p>
              </Match>
              <Match when={section() === "general"}>
                <div class="edit-section">
                  <div class="edit-grid">
                    <label class="field field--full">
                      <span class="field__label">Name</span>
                      <input
                        class="input"
                        data-autofocus
                        value={name()}
                        onInput={(event) => setName(event.currentTarget.value)}
                      />
                    </label>
                    <label class="field">
                      <span class="field__label edit-field__label">
                        Sorting name
                        <span class="edit-field__source">
                          {sortingName() ? "your own" : "automatic"}
                        </span>
                      </span>
                      <input
                        class="input"
                        value={sortingName() ?? ""}
                        placeholder={info()?.automaticSortingName}
                        onInput={(event) => setSortingName(event.currentTarget.value || null)}
                      />
                    </label>
                    <div class="field">
                      {label("Release date", "releaseDate")}
                      <input
                        id={fieldId("releaseDate")}
                        class="input"
                        classList={{ "input--edited": isEdited("releaseDate") }}
                        value={values()?.releaseDate ?? ""}
                        placeholder="YYYY-MM-DD"
                        onInput={(event) => setField("releaseDate", event.currentTarget.value)}
                      />
                    </div>
                    <div class="field">
                      <span class="field__label" id="edit-kind">
                        Type
                      </span>
                      <div class="segmented" role="radiogroup" aria-labelledby="edit-kind">
                        <For each={KINDS}>
                          {([value, text]) => (
                            <button
                              type="button"
                              role="radio"
                              class="segmented__option"
                              aria-checked={kind() === value}
                              onClick={() => setKind(value)}
                            >
                              {text}
                            </button>
                          )}
                        </For>
                      </div>
                    </div>
                    <div class="field field--full">
                      {label("Description", "description")}
                      <textarea
                        id={fieldId("description")}
                        class="input input--multiline"
                        classList={{ "input--edited": isEdited("description") }}
                        rows={6}
                        value={values()?.description ?? ""}
                        onInput={(event) =>
                          setField("description", event.currentTarget.value || null)
                        }
                      />
                    </div>
                  </div>
                  <h3 class="edit-section__title">You</h3>
                  <div class="edit-grid">
                    <Show when={kind() === "game"}>
                      <div class="field field--full">
                        <span class="field__label" id="edit-completion">
                          Completion status
                        </span>
                        <div class="segmented" role="radiogroup" aria-labelledby="edit-completion">
                          <For each={store.completion().statuses}>
                            {(status) => (
                              <button
                                type="button"
                                role="radio"
                                class="segmented__option"
                                aria-checked={completion() === status.id}
                                onClick={() =>
                                  setCompletion(completion() === status.id ? null : status.id)
                                }
                              >
                                {status.name}
                              </button>
                            )}
                          </For>
                        </div>
                      </div>
                    </Show>
                    <div class="field">
                      <span class="field__label edit-field__label" id="edit-score">
                        Your score
                        <span class="edit-field__source">
                          {score() === null ? "none" : `${score()} / 100`}
                        </span>
                      </span>
                      <div class="edit-score">
                        <input
                          type="range"
                          min="0"
                          max="100"
                          step="1"
                          aria-labelledby="edit-score"
                          value={score() ?? 50}
                          classList={{ "edit-score--empty": score() === null }}
                          onInput={(event) => setScore(Number(event.currentTarget.value))}
                        />
                        <button
                          type="button"
                          class="btn btn--sm btn--ghost"
                          disabled={score() === null}
                          onClick={() => setScore(null)}
                        >
                          Clear
                        </button>
                      </div>
                    </div>
                    <div class="field">
                      <span class="field__label">Categories</span>
                      <ChipInput
                        label="Categories"
                        values={categoryNames()}
                        suggestions={store.categories().map((c) => c.name)}
                        onChange={setCategoryNames}
                      />
                    </div>
                    <label class="field field--full">
                      <span class="field__label">Notes</span>
                      <textarea
                        class="input input--multiline"
                        rows={3}
                        placeholder="Anything you want to remember about this game."
                        value={notes()}
                        onInput={(event) => setNotes(event.currentTarget.value)}
                      />
                    </label>
                  </div>
                </div>
              </Match>
              <Match when={section() === "details"}>
                <div class="edit-section">
                  <p class="field__hint">
                    Each field shows where its value comes from. Change a field to keep your own
                    value through every metadata refresh; "Use … value" goes back to the automatic
                    one.
                  </p>
                  <div class="edit-grid">
                    <For each={LIST_FIELDS}>
                      {(entry) => (
                        <div class="field" classList={{ "field--full": entry.wide === true }}>
                          {label(entry.label, entry.field)}
                          <ChipInput
                            label={entry.label}
                            id={fieldId(entry.field)}
                            values={values()?.[entry.field] ?? []}
                            edited={isEdited(entry.field)}
                            suggestions={(store.valueLists()[entry.values] ?? []).map(
                              (v) => v.name,
                            )}
                            onChange={(list) => setField(entry.field, list)}
                          />
                        </div>
                      )}
                    </For>
                  </div>
                  <h3 class="edit-section__title">Numbers</h3>
                  <div class="edit-grid">
                    {numberInput(
                      "criticScore",
                      label("Critic score", "criticScore"),
                      "criticScore",
                      "0–100",
                    )}
                    {numberInput(
                      "communityScore",
                      label("Community score", "communityScore"),
                      "communityScore",
                      "0–100",
                    )}
                    {numberInput(
                      "ttbMain",
                      label("Time to beat: main story (hours)", "timeToBeat"),
                      "timeToBeat",
                    )}
                    <div class="field">
                      <span class="field__label">Main + extras / completionist (hours)</span>
                      <span class="field__row">
                        <input
                          class="input"
                          classList={{ "input--edited": isEdited("timeToBeat") }}
                          inputmode="decimal"
                          aria-label="Main + extras (hours)"
                          value={numbers().ttbExtra ?? ""}
                          onInput={(event) => {
                            const text = event.currentTarget.value;
                            setNumbers((n) => ({ ...n, ttbExtra: text }));
                            setEdited((all) => new Set([...all, "timeToBeat"]));
                          }}
                        />
                        <input
                          class="input"
                          classList={{ "input--edited": isEdited("timeToBeat") }}
                          inputmode="decimal"
                          aria-label="Completionist (hours)"
                          value={numbers().ttbComplete ?? ""}
                          onInput={(event) => {
                            const text = event.currentTarget.value;
                            setNumbers((n) => ({ ...n, ttbComplete: text }));
                            setEdited((all) => new Set([...all, "timeToBeat"]));
                          }}
                        />
                      </span>
                    </div>
                    {numberInput(
                      "installSize",
                      label("Install size (GB)", "installSize"),
                      "installSize",
                    )}
                  </div>
                </div>
              </Match>
              <Match when={section() === "links"}>
                <div class="edit-section">
                  <p class="field__hint">
                    Links from all sources are shown on the game page. Hide the ones you don't need,
                    or add your own.
                  </p>
                  <ul class="edit-links">
                    <For each={ownLinks()}>
                      {(link, index) => (
                        <li class="edit-link">
                          <div class="edit-link__text">
                            <span>{link.name || shortUrl(link.url)}</span>
                            <small class="muted">{shortUrl(link.url)} · added by you</small>
                          </div>
                          <button
                            type="button"
                            class="btn btn--sm btn--ghost"
                            onClick={() => {
                              setEditingLink(index());
                              setNewLink({ name: link.name, url: link.url });
                            }}
                          >
                            Edit
                          </button>
                          <button
                            type="button"
                            class="btn btn--sm btn--ghost"
                            onClick={() =>
                              setOwnLinks((all) => all.filter((_, i) => i !== index()))
                            }
                          >
                            Remove
                          </button>
                        </li>
                      )}
                    </For>
                    <For each={links()}>
                      {(entry) => (
                        <li
                          class="edit-link"
                          classList={{ "edit-link--hidden": isHidden(entry.link.url) }}
                        >
                          <div class="edit-link__text">
                            <span>
                              {entry.link.name}
                              <Show when={isHidden(entry.link.url)}>
                                <small class="muted"> (hidden)</small>
                              </Show>
                            </span>
                            <small class="muted">
                              {shortUrl(entry.link.url)} · {sourceName(entry.source, nameOf)}
                            </small>
                          </div>
                          <button
                            type="button"
                            class="btn btn--sm btn--ghost"
                            onClick={() =>
                              setHidden((all) =>
                                isHidden(entry.link.url)
                                  ? all.filter((u) => u !== entry.link.url)
                                  : [...all, entry.link.url],
                              )
                            }
                          >
                            {isHidden(entry.link.url) ? "Show" : "Hide"}
                          </button>
                        </li>
                      )}
                    </For>
                  </ul>
                  <form class="edit-links__add" onSubmit={addLink}>
                    <input
                      class="input"
                      aria-label="Link name"
                      placeholder="Name"
                      value={newLink().name}
                      onInput={(event) =>
                        setNewLink((l) => ({ ...l, name: event.currentTarget.value }))
                      }
                    />
                    <input
                      class="input"
                      aria-label="Link address"
                      placeholder="https://…"
                      type="url"
                      value={newLink().url}
                      onInput={(event) =>
                        setNewLink((l) => ({ ...l, url: event.currentTarget.value }))
                      }
                    />
                    <button type="submit" class="btn">
                      {editingLink() === null ? "Add link" : "Save link"}
                    </button>
                  </form>
                </div>
              </Match>
            </Switch>
          </div>
        </div>
        <Show when={error()}>
          <p class="error-text edit-game__error" role="alert">
            {error()}
          </p>
        </Show>
        <div class="edit-game__footer">
          <button
            type="button"
            class="btn btn--ghost"
            onClick={() =>
              void refreshMetadata(id).then((failure) =>
                failure ? setError(failure) : store.pushToast("info", "Refreshing metadata…"),
              )
            }
          >
            Refresh metadata
          </button>
          <span class="edit-game__buttons">
            <button type="button" class="btn" onClick={props.onClose}>
              Cancel
            </button>
            <button
              type="button"
              class="btn btn--primary"
              disabled={saving() || !ready()}
              onClick={() => void save()}
            >
              Save
            </button>
          </span>
        </div>
      </div>
      <Show when={picker()}>
        {(slot) => (
          <Portal>
            {/* Outside the dialog, so its focus trap doesn't fight the picker's. */}
            <ImagePicker
              gameId={id}
              gameName={game().name}
              slot={slot()}
              current={urlOf(game(), slot()) ?? pathOf(slot())}
              onClose={() => setPicker(null)}
              onPick={(url) => {
                const chosen = slot();
                setPicker(null);
                void imageAction(() => api.setUserImage(id, chosen, url)).then(() =>
                  markUser(chosen, true),
                );
              }}
            />
          </Portal>
        )}
      </Show>
    </Modal>
  );
}
