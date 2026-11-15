import { translator } from "../i18n";
import { gameLaunch } from "../i18n/gameLaunch";
import GripVertical from "lucide-solid/icons/grip-vertical";
import Ellipsis from "lucide-solid/icons/ellipsis";
import Plus from "lucide-solid/icons/plus";
import { createSignal, createUniqueId, For, Show } from "solid-js";
import type { Game } from "../bindings/Game";
import type { LaunchProfile } from "../bindings/LaunchProfile";
import type { LaunchProfiles } from "../bindings/LaunchProfiles";
import type { PlayAction } from "../bindings/PlayAction";
import { moveItem } from "../lib/editGame";
import { pickDirectory, pickExecutable } from "../lib/pickers";
import { Menu, type MenuEntry } from "./Menu";
import type { PopoverAnchor } from "./Popover";
import { SourceIcon, sourceName } from "./SourceIcon";

const tr = translator(gameLaunch);

const EXECUTABLE = "executable";

function emptyToNull(value: string): string | null {
  const trimmed = value.trim();
  return trimmed === "" ? null : trimmed;
}

/** One profile's form: name, what it is based on, arguments and the executable fields. */
function ProfileForm(props: {
  game: Game;
  profile: LaunchProfile | null;
  /** A store copy's own profile: only name and arguments (or, for a manual game, its launch). */
  copyAction: PlayAction | null;
  /** Whether a copy's launch takes arguments. */
  argsSupportedFor: (copyId: string) => boolean;
  onSave: (input: {
    name: string | null;
    basedOn: string;
    args: string | null;
    action: PlayAction | null;
  }) => string | null;
  onCancel: () => void;
}) {
  const own = () => props.profile === null || props.profile.own;
  const manualCopy = () => !own() && props.profile?.source === "manual";
  const startAction = () => (manualCopy() ? props.copyAction : props.profile?.action) ?? null;
  const [name, setName] = createSignal(props.profile?.customName ?? "");
  const [basedOn, setBasedOn] = createSignal(
    props.profile ? (props.profile.copyId ?? EXECUTABLE) : (props.game.copies[0]?.id ?? EXECUTABLE),
  );
  const [args, setArgs] = createSignal(
    (manualCopy() ? startAction()?.args : props.profile?.args) ?? "",
  );
  const [target, setTarget] = createSignal(startAction()?.target ?? "");
  const [workingDir, setWorkingDir] = createSignal(startAction()?.workingDir ?? "");
  const [error, setError] = createSignal<string | null>(null);
  const argsId = createUniqueId();

  const executable = () => manualCopy() || basedOn() === EXECUTABLE;
  const argsSupported = () => {
    if (executable()) return true;
    if (props.profile && !own()) return props.profile.argsSupported;
    // A new or own profile based on a copy: like that copy's own profile.
    return props.argsSupportedFor(basedOn());
  };

  function save(event: SubmitEvent) {
    event.preventDefault();
    event.stopPropagation();
    const base = startAction();
    const action: PlayAction | null = executable()
      ? {
          kind: base?.kind ?? "file",
          target: target().trim(),
          args: emptyToNull(args()),
          workingDir: emptyToNull(workingDir()),
          tracking: base?.tracking ?? "default",
          trackingPath: base?.trackingPath ?? null,
        }
      : null;
    if (executable() && action?.target === "") {
      setError(tr("chooseExecutable"));
      return;
    }
    const failure = props.onSave({
      name: emptyToNull(name()),
      basedOn: basedOn(),
      args: executable() ? null : emptyToNull(args()),
      action,
    });
    setError(failure);
  }

  return (
    <form class="profile__body" onSubmit={save}>
      <label class="field">
        <span class="field__label">{tr("name")}</span>
        <input
          class="input"
          data-autofocus
          value={name()}
          placeholder={props.profile?.name ?? tr("profileName")}
          onInput={(event) => setName(event.currentTarget.value)}
        />
      </label>
      <Show when={own()}>
        <div class="field">
          <span class="field__label" id="profile-based-on">
            {tr("basedOn")}
          </span>
          <div class="segmented" role="radiogroup" aria-labelledby="profile-based-on">
            <For each={props.game.copies}>
              {(copy) => (
                <button
                  type="button"
                  role="radio"
                  class="segmented__option"
                  aria-checked={basedOn() === copy.id}
                  onClick={() => setBasedOn(copy.id)}
                >
                  {copy.source === "manual" ? tr("itsExecutable") : sourceName(copy.source)}
                </button>
              )}
            </For>
            <button
              type="button"
              role="radio"
              class="segmented__option"
              aria-checked={basedOn() === EXECUTABLE}
              onClick={() => setBasedOn(EXECUTABLE)}
            >
              {tr("executable")}
            </button>
          </div>
        </div>
      </Show>
      <Show when={executable()}>
        <label class="field field--full">
          <span class="field__label">{tr("executable")}</span>
          <span class="field__row">
            <input
              class="input"
              value={target()}
              onInput={(event) => setTarget(event.currentTarget.value)}
            />
            <button
              type="button"
              class="btn"
              onClick={() => void pickExecutable().then((path) => path !== null && setTarget(path))}
            >
              {tr("browse")}
            </button>
          </span>
        </label>
      </Show>
      <div class="field field--full">
        <label class="field__label" for={argsId}>
          {executable() ? tr("arguments") : tr("extraArguments")}
        </label>
        <input
          id={argsId}
          class="input"
          value={args()}
          disabled={!argsSupported()}
          placeholder={tr("argsPlaceholder")}
          aria-describedby="profile-args-hint"
          onInput={(event) => setArgs(event.currentTarget.value)}
        />
        <span class="field__hint" id="profile-args-hint">
          {argsSupported()
            ? executable()
              ? tr("passedToExe")
              : tr("passedToStore")
            : tr("noArgs")}
        </span>
      </div>
      <Show when={executable()}>
        <label class="field field--full">
          <span class="field__label">{tr("workingDir")}</span>
          <span class="field__row">
            <input
              class="input"
              value={workingDir()}
              placeholder={tr("workingDirPlaceholder")}
              onInput={(event) => setWorkingDir(event.currentTarget.value)}
            />
            <button
              type="button"
              class="btn"
              onClick={() =>
                void pickDirectory().then((path) => path !== null && setWorkingDir(path))
              }
            >
              {tr("browse")}
            </button>
          </span>
        </label>
      </Show>
      <Show when={error()}>
        <p class="error-text field--full" role="alert">
          {error()}
        </p>
      </Show>
      <div class="profile__actions field--full">
        <button type="button" class="btn" onClick={() => props.onCancel()}>
          {tr("cancel")}
        </button>
        <button type="submit" class="btn btn--primary">
          {props.profile ? tr("saveProfile") : tr("addProfile")}
        </button>
      </div>
    </form>
  );
}

/**
 * The Launch section: the game's launch profiles. Changes are a draft like the rest of the Edit
 * dialog: `onChange` gets the edited list, the dialog saves it with Save. A manual copy's own
 * profile edits that copy's launch (`onCopyAction`). Drag a profile, or use its menu, to reorder.
 */
export function EditLaunchSection(props: {
  game: Game;
  profiles: LaunchProfiles | undefined;
  onChange: (profiles: LaunchProfiles) => void;
  /** The (edited) launch of a manual copy. */
  copyAction: (copyId: string) => PlayAction | null;
  onCopyAction: (copyId: string, action: PlayAction) => void;
}) {
  const [open, setOpen] = createSignal<string | null>(null);
  const [menu, setMenu] = createSignal<{ profile: LaunchProfile; anchor: PopoverAnchor } | null>(
    null,
  );
  const [dragged, setDragged] = createSignal<number | null>(null);
  const menuButtons = new Map<string, HTMLButtonElement>();
  const list = () => props.profiles?.profiles ?? [];

  function change(profiles: LaunchProfile[], defaultId = props.profiles?.defaultId ?? null) {
    const current = props.profiles;
    if (!current) return;
    const kept = defaultId !== null && profiles.some((p) => p.id === defaultId) ? defaultId : null;
    props.onChange({ ...current, profiles, defaultId: kept });
  }

  const setDefault = (id: string | null) => change(list(), id);

  /** The name a profile shows without one of the user's. */
  function baseName(profile: LaunchProfile): string {
    if (profile.own) return tr("profileNumber", { number: list().indexOf(profile) + 1 });
    return profile.source === "manual" || profile.source === null
      ? tr("executable")
      : sourceName(profile.source);
  }

  function saveProfile(
    profile: LaunchProfile | null,
    input: { name: string | null; basedOn: string; args: string | null; action: PlayAction | null },
  ): string | null {
    // A manual game's own profile is its launch: that is the copy's play action.
    if (profile && !profile.own && profile.source === "manual" && input.action) {
      props.onCopyAction(profile.id, input.action);
    }
    const copy =
      input.basedOn === EXECUTABLE ? null : props.game.copies.find((c) => c.id === input.basedOn);
    if (profile && !profile.own) {
      const manual = profile.source === "manual";
      const next: LaunchProfile = {
        ...profile,
        customName: input.name,
        name: input.name ?? (profile.customName === null ? profile.name : baseName(profile)),
        args: manual ? null : input.args,
      };
      change(list().map((p) => (p.id === profile.id ? next : p)));
    } else {
      const next: LaunchProfile = {
        id: profile?.id ?? crypto.randomUUID(),
        name: input.name ?? profile?.name ?? tr("profileNumber", { number: list().length + 1 }),
        customName: input.name,
        copyId: copy?.id ?? null,
        source: copy?.source ?? null,
        own: true,
        args: input.args,
        action: copy ? null : input.action,
        argsSupported: copy ? argsSupportedFor(copy.id) : true,
        installed: copy?.installed ?? true,
      };
      change(profile ? list().map((p) => (p.id === profile.id ? next : p)) : [...list(), next]);
    }
    setOpen(null);
    return null;
  }

  function reorder(ids: string[]) {
    const byId = new Map(list().map((p) => [p.id, p]));
    change(ids.flatMap((id) => byId.get(id) ?? []));
  }

  function move(index: number, by: -1 | 1) {
    reorder(moveItem(list(), index, by).map((p) => p.id));
  }

  const menuEntries = (profile: LaunchProfile): MenuEntry[] => {
    const index = list().findIndex((p) => p.id === profile.id);
    const isDefault = props.profiles?.defaultId === profile.id;
    return [
      {
        label: tr("edit"),
        onSelect: () => setOpen(profile.id),
      },
      isDefault
        ? { label: tr("useAutomatic"), onSelect: () => setDefault(null) }
        : { label: tr("makeDefault"), onSelect: () => setDefault(profile.id) },
      {
        label: tr("duplicate"),
        onSelect: () => {
          const name = tr("copyName", { name: profile.name });
          const duplicate: LaunchProfile = {
            ...profile,
            id: crypto.randomUUID(),
            name,
            customName: name,
            own: true,
            copyId: profile.copyId,
            // A copy's profile starts its copy; a manual copy's launch is its executable.
            action:
              profile.own || profile.source !== "manual"
                ? profile.action
                : props.copyAction(profile.id),
          };
          if (!profile.own && profile.source === "manual") duplicate.copyId = null;
          change([...list(), duplicate]);
        },
      },
      { kind: "separator" },
      { label: tr("moveUp"), disabled: index <= 0, onSelect: () => move(index, -1) },
      {
        label: tr("moveDown"),
        disabled: index >= list().length - 1,
        onSelect: () => move(index, 1),
      },
      ...(profile.own
        ? ([
            { kind: "separator" },
            {
              label: tr("delete"),
              danger: true,
              onSelect: () => change(list().filter((p) => p.id !== profile.id)),
            },
          ] satisfies MenuEntry[])
        : []),
    ];
  };

  const argsSupportedFor = (copyId: string) =>
    list().find((p) => p.id === copyId && !p.own)?.argsSupported ?? false;
  const copyAction = (profile: LaunchProfile) =>
    profile.source === "manual" ? props.copyAction(profile.id) : null;

  return (
    <div class="edit-section">
      <p class="field__hint">{tr("intro")}</p>
      <p class="profiles__default">
        <Show when={props.profiles?.defaultId} fallback={<>{tr("playStartsAutomatic")}</>}>
          {tr("playStartsDefault")}{" "}
          <button type="button" class="link-btn" onClick={() => setDefault(null)}>
            {tr("useAutomatic")}
          </button>
        </Show>
      </p>
      <ol class="profiles" aria-label={tr("launchProfiles")}>
        <For each={list()}>
          {(profile, index) => (
            <li
              class="profile"
              classList={{
                "profile--open": open() === profile.id,
                "profile--dragging": dragged() === index(),
              }}
              onDragOver={(event) => {
                if (dragged() !== null) event.preventDefault();
              }}
              onDrop={(event) => {
                event.preventDefault();
                const from = dragged();
                setDragged(null);
                if (from === null || from === index()) return;
                const ids = list().map((p) => p.id);
                const [id] = ids.splice(from, 1);
                if (id !== undefined) ids.splice(index(), 0, id);
                reorder(ids);
              }}
            >
              <div class="profile__top">
                <span
                  class="profile__drag"
                  draggable="true"
                  data-tooltip={tr("dragToReorder")}
                  aria-hidden="true"
                  onDragStart={(event) => {
                    setDragged(index());
                    event.dataTransfer?.setData("text/plain", profile.id);
                  }}
                  onDragEnd={() => setDragged(null)}
                >
                  <GripVertical class="icon" />
                </span>
                <Show when={profile.source}>{(source) => <SourceIcon source={source()} />}</Show>
                <button
                  type="button"
                  class="profile__name"
                  aria-expanded={open() === profile.id}
                  onClick={() => setOpen(open() === profile.id ? null : profile.id)}
                >
                  {profile.name}
                  <Show when={profile.args}>
                    <small class="muted"> {profile.args}</small>
                  </Show>
                  <Show when={!profile.installed}>
                    <small class="muted"> · {tr("notInstalled")}</small>
                  </Show>
                </button>
                <Show
                  when={props.profiles?.defaultId === profile.id}
                  fallback={
                    <button
                      type="button"
                      class="btn btn--sm btn--ghost"
                      onClick={() => setDefault(profile.id)}
                    >
                      {tr("makeDefault")}
                    </button>
                  }
                >
                  <span class="profile__default">{tr("default")}</span>
                </Show>
                <button
                  ref={(el) => menuButtons.set(profile.id, el)}
                  type="button"
                  class="icon-btn"
                  aria-label={tr("actions", { name: profile.name })}
                  aria-haspopup="menu"
                  onClick={(event) => {
                    const rect = event.currentTarget.getBoundingClientRect();
                    setMenu(menu() ? null : { profile, anchor: { rect, align: "end" } });
                  }}
                >
                  <Ellipsis class="icon" aria-hidden="true" />
                </button>
              </div>
              <Show when={open() === profile.id}>
                <ProfileForm
                  game={props.game}
                  profile={profile}
                  copyAction={copyAction(profile)}
                  argsSupportedFor={argsSupportedFor}
                  onSave={(input) => saveProfile(profile, input)}
                  onCancel={() => setOpen(null)}
                />
              </Show>
            </li>
          )}
        </For>
      </ol>
      <Show
        when={open() === "new"}
        fallback={
          <button type="button" class="btn profiles__add" onClick={() => setOpen("new")}>
            <Plus class="icon" aria-hidden="true" /> {tr("addProfile")}
          </button>
        }
      >
        <div class="profile profile--open">
          <ProfileForm
            game={props.game}
            profile={null}
            copyAction={null}
            argsSupportedFor={argsSupportedFor}
            onSave={(input) => saveProfile(null, input)}
            onCancel={() => setOpen(null)}
          />
        </div>
      </Show>
      <Show when={menu()}>
        {(current) => (
          <Menu
            anchor={current().anchor}
            label={tr("actions", { name: current().profile.name })}
            inside={() => [menuButtons.get(current().profile.id)]}
            onClose={(returnFocus) => {
              const button = menuButtons.get(current().profile.id);
              setMenu(null);
              if (returnFocus) button?.focus();
            }}
            entries={menuEntries(current().profile)}
          />
        )}
      </Show>
    </div>
  );
}
