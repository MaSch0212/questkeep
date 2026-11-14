import { createEffect, createSignal, For, Match, on, onCleanup, Show, Switch } from "solid-js";
import type { Appearance } from "../../bindings/Appearance";
import type { ColorScheme } from "../../bindings/ColorScheme";
import { translator } from "../../i18n";
import { settingsAppearance } from "../../i18n/settingsAppearance";
import { themeEditorText } from "../../i18n/themeEditor";
import { normalizeHex } from "../../lib/color";
import {
  applyAppearance,
  DEFAULT_APPEARANCE,
  DEFAULT_THEME_ID,
  effectiveScheme,
  getTheme,
  resolveParams,
  themes,
  themeText,
  withoutParams,
  withParam,
  type ParamValue,
  type ThemeDef,
  type ThemeParam,
} from "../../lib/themes";
import { themeEditor } from "../../lib/themeEditor";
import { getUserTheme, isBaseMissing } from "../../lib/userThemeStore";
import { ConfirmDialog } from "../ConfirmDialog";
import type { SettingsModel } from "./useSettings";

const tr = translator(settingsAppearance);
const trEditor = translator(themeEditorText);

const SCHEMES: ColorScheme[] = ["system", "light", "dark"];

/** While a color or slider is dragged the page follows at once; saving waits this long. */
const SAVE_DELAY_MS = 150;

const paramId = (key: string) => `appearance-param-${key}`;

/** The four preview colors as custom properties (the card's CSS falls back to the page's). */
function previewStyle(theme: ThemeDef): Record<string, string> {
  const [bg, surface, text, accent] = theme.preview?.[effectiveScheme()] ?? [];
  const style: Record<string, string> = {};
  if (bg) style["--qk-preview-bg"] = bg;
  if (surface) style["--qk-preview-surface"] = surface;
  if (text) style["--qk-preview-text"] = text;
  if (accent) style["--qk-preview-accent"] = accent;
  return style;
}

/** The themes to pick from: a theme that is being created shows up once it is saved. */
const pickable = () =>
  themes().filter((item) => item.origin !== "user" || getUserTheme(item.id) !== undefined);

/** What a user theme derives from, for its card. */
function basedOn(id: string): string | null {
  const theme = getUserTheme(id);
  if (!theme) return null;
  return isBaseMissing(theme)
    ? trEditor("basedOnMissing", { id: theme.base })
    : trEditor("basedOn", { name: getTheme(theme.base)?.name ?? theme.base });
}

/** Where a theme that is not built in comes from: its plugin by name, or the user. */
function originLabel(theme: ThemeDef): string {
  if (theme.origin !== "plugin") return tr("fromUser");
  return theme.plugin ? tr("fromPluginNamed", { name: theme.plugin.name }) : tr("fromPlugin");
}

/** A small labeled button on a theme card; the label names the theme for screen readers. */
function CardAction(props: { label: string; name: string; onClick: () => void }) {
  return (
    <button
      type="button"
      class="btn btn--small btn--ghost"
      aria-label={props.name}
      onClick={() => props.onClick()}
    >
      {props.label}
    </button>
  );
}

function ThemeCard(props: {
  theme: ThemeDef;
  selected: boolean;
  onSelect: () => void;
  onDelete: () => void;
}) {
  const descriptionId = () => `appearance-theme-${props.theme.id}-description`;
  const name = () => ({ name: props.theme.name });
  return (
    <div class="theme-card" classList={{ "theme-card--on": props.selected }}>
      <label class="theme-card__pick">
        <input
          type="radio"
          name="appearance-theme"
          value={props.theme.id}
          checked={props.selected}
          aria-label={props.theme.name}
          aria-describedby={
            props.theme.description || basedOn(props.theme.id) ? descriptionId() : undefined
          }
          onChange={() => props.onSelect()}
        />
        <span class="theme-card__preview" style={previewStyle(props.theme)} aria-hidden="true">
          <span class="theme-card__preview-side" />
          <span class="theme-card__preview-body">
            <span class="theme-card__preview-line" />
            <span class="theme-card__preview-line theme-card__preview-line--short" />
            <span class="theme-card__preview-accent" />
          </span>
        </span>
        <span class="theme-card__head">
          <span class="theme-card__name">{props.theme.name}</span>
          <Show when={props.theme.origin !== "builtin"}>
            <span class="theme-card__badge">{originLabel(props.theme)}</span>
          </Show>
        </span>
        <Show when={props.theme.description}>
          {(description) => (
            <span class="theme-card__description" id={descriptionId()}>
              {themeText(props.theme, description())}
            </span>
          )}
        </Show>
        <Show when={!props.theme.description && basedOn(props.theme.id)}>
          {(text) => (
            <span class="theme-card__description" id={descriptionId()}>
              {text()}
            </span>
          )}
        </Show>
        <Show when={props.theme.origin !== "builtin" && props.theme.author}>
          {(author) => (
            <span class="theme-card__description">{tr("by", { author: author() })}</span>
          )}
        </Show>
      </label>
      <div class="theme-card__actions">
        <Show
          when={props.theme.origin === "user"}
          fallback={
            <CardAction
              label={trEditor("customize")}
              name={trEditor("customizeTheme", name())}
              onClick={() => themeEditor.create(props.theme.id)}
            />
          }
        >
          <CardAction
            label={trEditor("edit")}
            name={trEditor("editTheme", name())}
            onClick={() => themeEditor.edit(props.theme.id)}
          />
          <CardAction
            label={trEditor("duplicate")}
            name={trEditor("duplicateTheme", name())}
            onClick={() => themeEditor.create(props.theme.id)}
          />
          <CardAction
            label={trEditor("exportShort")}
            name={trEditor("exportTheme", name())}
            onClick={() => void themeEditor.exportPlugin(props.theme.id)}
          />
          <CardAction
            label={trEditor("remove")}
            name={trEditor("removeTheme", name())}
            onClick={() => props.onDelete()}
          />
        </Show>
      </div>
    </div>
  );
}

/** One parameter: its control, a reset button and its hint. */
function ParamField(props: {
  theme: ThemeDef;
  param: ThemeParam;
  value: ParamValue;
  /** A value is saved for it (else it shows the theme's default). */
  changed: boolean;
  /** `undefined` goes back to the default; `live` marks changes that arrive while dragging. */
  onChange: (value: ParamValue | undefined, live?: boolean) => void;
}) {
  const [invalidHex, setInvalidHex] = createSignal(false);
  const id = () => paramId(props.param.key);
  const hintId = () => `${id()}-hint`;
  const label = () => themeText(props.theme, props.param.label);
  const describedBy = () => (props.param.hint ? hintId() : undefined);

  function typedHex(input: HTMLInputElement) {
    const typed = input.value.trim();
    const hex = normalizeHex(typed.startsWith("#") ? typed : `#${typed}`);
    setInvalidHex(hex === null);
    if (hex !== null) props.onChange(hex);
  }

  const reset = (
    <button
      type="button"
      class="btn btn--small btn--ghost param-reset"
      disabled={!props.changed}
      aria-label={tr("resetParam", { label: label() })}
      onClick={() => {
        setInvalidHex(false);
        props.onChange(undefined);
      }}
    >
      {tr("reset")}
    </button>
  );

  return (
    <>
      <div class="settings-row param-row">
        <Switch>
          <Match when={props.param.type === "color" && props.param}>
            <label for={id()}>{label()}</label>
            <div class="param-control">
              <input
                type="color"
                id={id()}
                class="param-color"
                aria-describedby={describedBy()}
                value={String(props.value)}
                onInput={(e) => {
                  setInvalidHex(false);
                  props.onChange(e.currentTarget.value, true);
                }}
              />
              <input
                type="text"
                class="input param-hex"
                aria-label={tr("hex", { label: label() })}
                aria-invalid={invalidHex() ? "true" : undefined}
                spellcheck={false}
                autocomplete="off"
                maxLength={9}
                value={String(props.value)}
                onChange={(e) => typedHex(e.currentTarget)}
              />
            </div>
          </Match>
          <Match when={props.param.type === "number" && props.param}>
            {(param) => (
              <>
                <label for={id()}>{label()}</label>
                <div class="param-control">
                  <input
                    type="range"
                    id={id()}
                    class="param-range"
                    aria-describedby={describedBy()}
                    min={param().min}
                    max={param().max}
                    step={param().step ?? 1}
                    value={Number(props.value)}
                    onInput={(e) => props.onChange(Number(e.currentTarget.value), true)}
                  />
                  <output class="param-value" for={id()}>
                    {`${String(props.value)}${param().unit ?? ""}`}
                  </output>
                </div>
              </>
            )}
          </Match>
          <Match when={props.param.type === "select" && props.param}>
            {(param) => (
              <>
                <label for={id()}>{label()}</label>
                <div class="param-control">
                  <select
                    id={id()}
                    class="input"
                    aria-describedby={describedBy()}
                    value={String(props.value)}
                    onChange={(e) => props.onChange(e.currentTarget.value)}
                  >
                    <For each={param().options}>
                      {(option) => (
                        <option value={option.value}>{themeText(props.theme, option.label)}</option>
                      )}
                    </For>
                  </select>
                </div>
              </>
            )}
          </Match>
          <Match when={props.param.type === "toggle"}>
            <label class="check">
              <input
                type="checkbox"
                id={id()}
                aria-describedby={describedBy()}
                checked={props.value === true}
                onChange={(e) => props.onChange(e.currentTarget.checked)}
              />
              <span>{label()}</span>
            </label>
          </Match>
        </Switch>
        {reset}
      </div>
      <Show when={invalidHex()}>
        <p class="field__hint error-text" role="alert">
          {tr("invalidHex")}
        </p>
      </Show>
      <Show when={props.param.hint}>
        {(hint) => (
          <p class="field__hint" id={hintId()}>
            {themeText(props.theme, hint())}
          </p>
        )}
      </Show>
    </>
  );
}

export function AppearanceSection(props: { model: SettingsModel }) {
  /** A change that is on the page but not saved yet. */
  const [draft, setDraft] = createSignal<Appearance | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;

  const saved = (): Appearance => ({
    ...DEFAULT_APPEARANCE,
    ...props.model.settings()?.appearance,
  });
  const current = () => draft() ?? saved();

  async function save() {
    clearTimeout(timer);
    timer = undefined;
    const next = draft();
    if (!next) return;
    await props.model.update({ appearance: next });
    if (draft() !== next) return;
    setDraft(null);
    // What is saved now: `next`, or the previous settings when saving failed.
    applyAppearance(saved());
  }

  /** Shows a change right away and saves it (a moment later for `live` changes). */
  function change(next: Appearance, live = false) {
    setDraft(next);
    applyAppearance(next);
    clearTimeout(timer);
    if (live) timer = setTimeout(() => void save(), SAVE_DELAY_MS);
    else void save();
  }

  onCleanup(() => {
    if (timer !== undefined) void save();
  });

  // The theme editor saves the settings itself (selecting a new theme): show what it saved.
  createEffect(on(themeEditor.settingsSaved, () => void props.model.reload(), { defer: true }));

  /** The user theme a delete was asked for, while its confirmation shows. */
  const [deleting, setDeleting] = createSignal<ThemeDef | null>(null);

  /** The theme whose options show: the chosen one, or the default while that is missing. */
  const theme = () => getTheme(current().theme) ?? getTheme(DEFAULT_THEME_ID);
  const missing = () => getTheme(current().theme) === undefined;

  return (
    <Show when={props.model.settings()} fallback={<p class="muted">{tr("loading")}</p>}>
      <div class="settings-row">
        <span id="appearance-mode-label">{tr("mode")}</span>
        <div class="choice" role="radiogroup" aria-labelledby="appearance-mode-label">
          <For each={SCHEMES}>
            {(scheme) => (
              <label class="choice__option">
                <input
                  type="radio"
                  name="appearance-scheme"
                  value={scheme}
                  checked={current().scheme === scheme}
                  onChange={() => change({ ...current(), scheme })}
                />
                <span>{tr(scheme)}</span>
              </label>
            )}
          </For>
        </div>
      </div>
      <fieldset class="fieldset settings-group">
        <legend class="fieldset__legend">{tr("theme")}</legend>
        <div class="theme-picker" role="radiogroup" aria-label={tr("theme")}>
          <For each={pickable()}>
            {(item) => (
              <ThemeCard
                theme={item}
                selected={theme()?.id === item.id}
                onSelect={() => change({ ...current(), theme: item.id })}
                onDelete={() => setDeleting(item)}
              />
            )}
          </For>
        </div>
        <div class="appearance-actions appearance-actions--start">
          <button
            type="button"
            class="btn btn--small"
            onClick={() => themeEditor.create(theme()?.id ?? DEFAULT_THEME_ID)}
          >
            {trEditor("createTheme")}
          </button>
        </div>
        <Show when={deleting()} keyed>
          {(item) => (
            // Confirming closes the dialog first, so the theme is taken from here, not the signal.
            <ConfirmDialog
              title={trEditor("deleteTitle")}
              message={trEditor("deleteMessage", { name: item.name })}
              confirmLabel={trEditor("remove")}
              onConfirm={() => void themeEditor.remove(item.id)}
              onClose={() => setDeleting(null)}
            />
          )}
        </Show>
        <Show when={missing()}>
          <p class="field__hint">{tr("missingTheme", { id: current().theme })}</p>
        </Show>
      </fieldset>
      <Show when={theme()} keyed>
        {(active) => {
          const values = () =>
            resolveParams(active, current().params[active.id], effectiveScheme());
          const field = (param: ThemeParam) => (
            <ParamField
              theme={active}
              param={param}
              value={values()[param.key] as ParamValue}
              changed={current().params[active.id]?.[param.key] !== undefined}
              onChange={(value, live) =>
                change(withParam(current(), active.id, param.key, value), live)
              }
            />
          );
          const basic = active.params.filter((p) => !p.advanced);
          const advanced = active.params.filter((p) => p.advanced);
          return (
            <Show when={active.params.length > 0}>
              <fieldset class="fieldset settings-group">
                <legend class="fieldset__legend">{tr("options", { name: active.name })}</legend>
                <For each={basic}>{field}</For>
                <Show when={advanced.length > 0}>
                  <details class="appearance-advanced">
                    <summary>{tr("advanced")}</summary>
                    <For each={advanced}>{field}</For>
                  </details>
                </Show>
                <div class="appearance-actions">
                  <button
                    type="button"
                    class="btn btn--small"
                    disabled={current().params[active.id] === undefined}
                    onClick={() => change(withoutParams(current(), active.id))}
                  >
                    {tr("resetAll")}
                  </button>
                </div>
              </fieldset>
            </Show>
          );
        }}
      </Show>
    </Show>
  );
}
