import X from "lucide-solid/icons/x";
import { createMemo, createSignal, For, onCleanup, Show } from "solid-js";
import { translator } from "../i18n";
import { shellCommon } from "../i18n/shellCommon";
import { themeEditorText } from "../i18n/themeEditor";
import { themeEditor, type EditTarget } from "../lib/themeEditor";
import { appliedAppearance, themes, type Scheme } from "../lib/themes";
import { baseTheme, isBaseMissing } from "../lib/userThemeStore";
import {
  filterVariables,
  hasBalancedBraces,
  isOverrideValue,
  MAX_NAME_LENGTH,
  overrideSource,
  overridesAccent,
  overrideValue,
  resolvedHex,
  stepValue,
  themeVariables,
  userThemeVars,
  VARIABLE_GROUPS,
  type ThemeVariable,
  type VariableGroup,
} from "../lib/userThemes";
import { Modal } from "./Modal";

const tr = translator(themeEditorText);
const trCommon = translator(shellCommon);

/** Every variable of theme.css, read once. */
const VARIABLES = themeVariables();

const SCHEMES: Scheme[] = ["light", "dark"];
const TARGETS: { value: EditTarget; label: "targetAuto" | "targetScheme" | "targetBoth" }[] = [
  { value: "auto", label: "targetAuto" },
  { value: "scheme", label: "targetScheme" },
  { value: "both", label: "targetBoth" },
];

const GROUP_TITLES = {
  surfaces: "groupSurfaces",
  text: "groupText",
  accent: "groupAccent",
  status: "groupStatus",
  charts: "groupCharts",
  cards: "groupCards",
  typography: "groupTypography",
  shape: "groupShape",
  shadows: "groupShadows",
  motion: "groupMotion",
  sizes: "groupSizes",
} as const satisfies Record<VariableGroup, string>;

/** The groups that start open; the rest (the long advanced one above all) open on demand. */
const OPEN_AT_START: VariableGroup[] = ["surfaces", "text", "accent"];

const inputId = (name: string) => `theme-var${name.slice(1)}`;

const overrides = () => themeEditor.draft().overrides;

/** Themes a user theme can build on: everything but user themes. */
const bases = () => themes().filter((theme) => theme.origin !== "user");

/** `value` kept between 0 and `max` (0 when `max` is negative). */
const clamp = (value: number, max: number) => Math.min(Math.max(0, value), Math.max(0, max));

/** One variable: its name, an editor for its kind, where it is overridden, and Reset. */
function VariableRow(props: {
  variable: ThemeVariable;
  /** The theme's variables as edited, and those of its base alone (what Reset shows). */
  vars: Map<string, string>;
  baseVars: Map<string, string>;
}) {
  const [invalid, setInvalid] = createSignal(false);
  const name = () => props.variable.name;
  const value = () => overrideValue(overrides(), name(), themeEditor.scheme());
  const source = () => overrideSource(overrides(), name(), themeEditor.scheme());
  const inherited = () => props.baseVars.get(name()) ?? props.variable.base[themeEditor.scheme()];
  const stepped = () => props.variable.kind === "length" || props.variable.kind === "number";

  function typed(text: string) {
    if (text.trim() === "") {
      setInvalid(false);
      themeEditor.setOverride(props.variable, undefined);
    } else if (isOverrideValue(text)) {
      setInvalid(false);
      themeEditor.setOverride(props.variable, text);
    } else {
      setInvalid(true);
    }
  }

  function onKeyDown(event: KeyboardEvent) {
    if (!stepped() || (event.key !== "ArrowUp" && event.key !== "ArrowDown")) return;
    const next = stepValue(
      value() ?? inherited(),
      event.key === "ArrowUp" ? 1 : -1,
      event.shiftKey,
    );
    if (next === null) return;
    event.preventDefault();
    setInvalid(false);
    themeEditor.setOverride(props.variable, next, true);
  }

  return (
    <li class="theme-var" classList={{ "theme-var--changed": source() !== null }}>
      <label class="theme-var__label" for={inputId(name())}>
        <span>{props.variable.label}</span>
        <code class="theme-var__name">{name()}</code>
      </label>
      <Show when={source()}>{(where) => <span class="theme-var__mark">{tr(where())}</span>}</Show>
      <div class="theme-var__controls">
        <Show when={props.variable.kind === "color"}>
          <input
            type="color"
            class="param-color theme-var__color"
            aria-label={tr("pickColor", { label: props.variable.label })}
            value={(resolvedHex(name(), props.vars) ?? "#000000").slice(0, 7)}
            onInput={(e) => {
              setInvalid(false);
              themeEditor.setOverride(props.variable, e.currentTarget.value, true);
            }}
          />
        </Show>
        <input
          type="text"
          id={inputId(name())}
          class="input"
          spellcheck={false}
          autocomplete="off"
          aria-invalid={invalid() ? "true" : undefined}
          placeholder={inherited()}
          value={value() ?? ""}
          onInput={(e) => typed(e.currentTarget.value)}
          onKeyDown={onKeyDown}
        />
        <button
          type="button"
          class="btn btn--small btn--ghost"
          disabled={source() === null}
          aria-label={tr("resetVariable", { label: props.variable.label })}
          onClick={() => {
            setInvalid(false);
            themeEditor.setOverride(props.variable, undefined, true);
          }}
        >
          {tr("reset")}
        </button>
      </div>
      <Show when={invalid()}>
        <p class="field__hint error-text theme-var__error" role="alert">
          {tr("invalidValue")}
        </p>
      </Show>
    </li>
  );
}

/**
 * The live theme editor: a panel beside the app (not a dialog), so the app stays usable while a
 * theme is edited. Rendered by App while `themeEditor.isOpen()`.
 */
export function ThemeEditor() {
  const [search, setSearch] = createSignal("");
  const [opened, setOpened] = createSignal<VariableGroup[]>(OPEN_AT_START);
  const [dock, setDock] = createSignal<"left" | "right">("right");
  /** Where the panel was dragged to; null while docked. */
  const [position, setPosition] = createSignal<{ x: number; y: number } | null>(null);
  const [asking, setAsking] = createSignal(false);
  let panel: HTMLElement | undefined;
  let stopDrag: (() => void) | undefined;

  const draft = themeEditor.draft;
  const scheme = themeEditor.scheme;
  const base = createMemo(() => baseTheme(draft()));
  const saved = () => appliedAppearance()?.params[draft().id];
  const vars = createMemo(() => userThemeVars(draft(), base(), scheme(), { saved: saved() }));
  const baseVars = createMemo(() =>
    userThemeVars(draft(), base(), scheme(), { overrides: false, saved: saved() }),
  );
  const searching = () => search().trim() !== "";
  const shown = createMemo(() => filterVariables(VARIABLES, search()));
  const groups = createMemo(() =>
    VARIABLE_GROUPS.map((group) => ({
      group,
      variables: shown().filter((variable) => variable.group === group),
    })).filter((entry) => entry.variables.length > 0),
  );
  const changedIn = (variables: ThemeVariable[]) =>
    variables.filter((v) => overrideSource(draft().overrides, v.name, scheme()) !== null).length;

  function requestClose() {
    if (themeEditor.isDirty()) setAsking(true);
    else themeEditor.close();
  }

  function startDrag(event: PointerEvent) {
    if (!panel || (event.target as Element).closest("button, input, select")) return;
    const box = panel.getBoundingClientRect();
    const grab = { x: event.clientX - box.left, y: event.clientY - box.top };
    const header = (event.currentTarget as HTMLElement).offsetHeight;
    const move = (moved: PointerEvent) =>
      setPosition({
        x: clamp(moved.clientX - grab.x, window.innerWidth - box.width),
        y: clamp(moved.clientY - grab.y, window.innerHeight - header),
      });
    stopDrag?.();
    stopDrag = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", stopDrag as () => void);
      stopDrag = undefined;
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", stopDrag);
    event.preventDefault();
  }
  onCleanup(() => stopDrag?.());

  /** Docks to the other side; a panel that was dragged away docks where it was docked last. */
  function toggleDock() {
    if (position()) setPosition(null);
    else setDock(dock() === "right" ? "left" : "right");
  }
  const dockLabel = () =>
    position()
      ? tr(dock() === "right" ? "dockRight" : "dockLeft")
      : tr(dock() === "right" ? "dockLeft" : "dockRight");

  return (
    <>
      <aside
        ref={(el) => (panel = el)}
        class="theme-editor"
        classList={{
          "theme-editor--left": dock() === "left" && position() === null,
          "theme-editor--floating": position() !== null,
        }}
        style={
          position()
            ? {
                "--qk-theme-editor-x": `${position()?.x ?? 0}px`,
                "--qk-theme-editor-y": `${position()?.y ?? 0}px`,
              }
            : undefined
        }
        role="complementary"
        aria-label={tr("title")}
      >
        <header
          class="theme-editor__header"
          data-tooltip={tr("dragToMove")}
          onPointerDown={startDrag}
        >
          <h2 class="theme-editor__title">{tr("title")}</h2>
          <button type="button" class="btn btn--small btn--ghost" onClick={toggleDock}>
            {dockLabel()}
          </button>
          <button type="button" class="icon-btn" aria-label={tr("close")} onClick={requestClose}>
            <X class="icon" aria-hidden="true" />
          </button>
        </header>
        <div class="theme-editor__body">
          <div class="theme-editor__field">
            <label for="theme-editor-name">{tr("name")}</label>
            <input
              type="text"
              id="theme-editor-name"
              class="input"
              maxLength={MAX_NAME_LENGTH}
              autocomplete="off"
              value={draft().name}
              onInput={(e) => themeEditor.update({ name: e.currentTarget.value })}
            />
          </div>
          <div class="theme-editor__field">
            <label for="theme-editor-base">{tr("base")}</label>
            <select
              id="theme-editor-base"
              class="input"
              value={draft().base}
              onChange={(e) => themeEditor.update({ base: e.currentTarget.value }, true)}
            >
              <Show when={isBaseMissing(draft())}>
                <option value={draft().base}>{draft().base}</option>
              </Show>
              <For each={bases()}>{(theme) => <option value={theme.id}>{theme.name}</option>}</For>
            </select>
            <Show when={isBaseMissing(draft())}>
              <p class="field__hint">{tr("baseMissing", { id: draft().base })}</p>
            </Show>
          </div>
          <div class="theme-editor__row">
            <span id="theme-editor-mode-label">{tr("editedMode")}</span>
            <div class="choice" role="radiogroup" aria-labelledby="theme-editor-mode-label">
              <For each={SCHEMES}>
                {(option) => (
                  <label class="choice__option">
                    <input
                      type="radio"
                      name="theme-editor-scheme"
                      value={option}
                      checked={scheme() === option}
                      onChange={() => themeEditor.setEditScheme(option)}
                    />
                    <span>{tr(option)}</span>
                  </label>
                )}
              </For>
            </div>
          </div>
          <div class="theme-editor__field">
            <label for="theme-editor-target">{tr("appliesTo")}</label>
            <select
              id="theme-editor-target"
              class="input"
              value={themeEditor.target()}
              onChange={(e) => themeEditor.setTarget(e.currentTarget.value as EditTarget)}
            >
              <For each={TARGETS}>
                {(option) => <option value={option.value}>{tr(option.label)}</option>}
              </For>
            </select>
          </div>
          <Show
            when={
              overridesAccent(draft().overrides, scheme()) &&
              base()?.params.some((param) => param.key === "accent")
            }
          >
            <p class="field__hint theme-editor__note">{tr("accentHint")}</p>
          </Show>
          <input
            type="search"
            class="input"
            aria-label={tr("search")}
            placeholder={tr("search")}
            value={search()}
            onInput={(e) => setSearch(e.currentTarget.value)}
          />
          <Show when={groups().length === 0}>
            <p class="muted">{tr("noMatches")}</p>
          </Show>
          <For each={groups()}>
            {(entry) => {
              const isOpen = () => searching() || opened().includes(entry.group);
              return (
                <details
                  class="theme-editor__group"
                  open={isOpen()}
                  onToggle={(e) => {
                    // While a search holds every group open, toggles are not the user's choice.
                    if (searching()) return;
                    const others = opened().filter((group) => group !== entry.group);
                    setOpened(e.currentTarget.open ? [...others, entry.group] : others);
                  }}
                >
                  <summary>
                    <span>{tr(GROUP_TITLES[entry.group])}</span>
                    <Show when={changedIn(entry.variables) > 0}>
                      <span class="theme-editor__count">
                        {tr("changedCount", { count: changedIn(entry.variables) })}
                      </span>
                    </Show>
                  </summary>
                  {/* Rows exist only while their group is open: the advanced group is long. */}
                  <Show when={isOpen()}>
                    <ul class="theme-editor__vars">
                      <For each={entry.variables}>
                        {(variable) => (
                          <VariableRow variable={variable} vars={vars()} baseVars={baseVars()} />
                        )}
                      </For>
                    </ul>
                  </Show>
                </details>
              );
            }}
          </For>
          <div class="theme-editor__field">
            <label for="theme-editor-css">{tr("customCss")}</label>
            <textarea
              id="theme-editor-css"
              class="input input--multiline theme-editor__css"
              spellcheck={false}
              aria-describedby="theme-editor-css-hint"
              value={draft().css}
              onInput={(e) => themeEditor.update({ css: e.currentTarget.value })}
            />
            <p class="field__hint" id="theme-editor-css-hint">
              {tr("customCssHint")}
            </p>
            <Show when={!hasBalancedBraces(draft().css)}>
              <p class="field__hint error-text" role="alert">
                {tr("cssUnbalanced")}
              </p>
            </Show>
          </div>
        </div>
        <footer class="theme-editor__footer">
          <button
            type="button"
            class="btn btn--primary"
            disabled={!themeEditor.isDirty() && !themeEditor.isNew()}
            onClick={() => void themeEditor.save()}
          >
            {tr("save")}
          </button>
          <button type="button" class="btn" onClick={() => void themeEditor.save(true)}>
            {tr("saveCopy")}
          </button>
          <button
            type="button"
            class="btn"
            disabled={!themeEditor.isDirty()}
            onClick={() => themeEditor.discard()}
          >
            {tr("discard")}
          </button>
          <button
            type="button"
            class="btn"
            disabled={themeEditor.isDirty() || themeEditor.isNew()}
            aria-describedby="theme-editor-export-hint"
            onClick={() => void themeEditor.exportPlugin(draft().id)}
          >
            {tr("exportPlugin")}
          </button>
          <button type="button" class="btn" onClick={requestClose}>
            {tr("close")}
          </button>
          <Show when={themeEditor.isDirty() || themeEditor.isNew()}>
            <p class="field__hint theme-editor__footer-hint" id="theme-editor-export-hint">
              {tr("exportNeedsSave")}
            </p>
          </Show>
        </footer>
      </aside>
      <Show when={asking()}>
        <Modal title={tr("unsavedTitle")} onClose={() => setAsking(false)}>
          <p>{tr("unsavedMessage", { name: draft().name })}</p>
          <div class="form__actions">
            <button type="button" class="btn" data-autofocus onClick={() => setAsking(false)}>
              {trCommon("cancel")}
            </button>
            <button
              type="button"
              class="btn btn--danger-solid"
              onClick={() => {
                setAsking(false);
                themeEditor.close();
              }}
            >
              {tr("discard")}
            </button>
            <button
              type="button"
              class="btn btn--primary"
              onClick={() => {
                setAsking(false);
                void themeEditor.save().then((ok) => {
                  if (ok) themeEditor.close();
                });
              }}
            >
              {tr("save")}
            </button>
          </div>
        </Modal>
      </Show>
    </>
  );
}
