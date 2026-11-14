import { For, Match, Show, Switch } from "solid-js";
import { translator } from "../../i18n";
import { extensions } from "../../i18n/extensions";
import { errorMessage } from "../../lib/errors";
import { coerceValue, type DeclaredSetting } from "../../lib/extensions/declaredSettings";
import { settingsControllers, settingsSections } from "../../lib/extensions/registry";
import type { SettingsController } from "../../lib/extensions/settings";
import { sortPlugins } from "../../lib/plugins";
import { store } from "../../lib/store";
import { ExtensionMount } from "../ExtensionMount";

const tr = translator(extensions);

/** Ids of the plugins with something for the settings page (declared settings or own sections),
 *  in the order of the plugin list. */
export function pluginsWithSettings(): string[] {
  const controllers = settingsControllers();
  const withSections = new Set(settingsSections().map((entry) => entry.plugin));
  return sortPlugins(store.plugins())
    .map((plugin) => plugin.id)
    .filter((id) => (controllers[id]?.declared.length ?? 0) > 0 || withSections.has(id));
}

function SettingRow(props: {
  plugin: string;
  controller: SettingsController;
  setting: DeclaredSetting;
}) {
  // Taken once: a reload makes a new controller and new rows.
  const { controller, setting } = props;
  const id = `plugin-${props.plugin}-setting-${setting.key}`;
  const hintId = setting.hint === null ? undefined : `${id}-hint`;
  const value = () => controller.get(setting.key);

  function save(next: unknown) {
    controller.set(setting.key, next).catch((error: unknown) => {
      store.pushToast("error", tr("saveFailed", { error: errorMessage(error) }));
    });
  }

  /** Saves what a control holds if it fits the setting; else the control shows the old value. */
  function change(input: HTMLInputElement | HTMLSelectElement, raw: unknown) {
    const next = coerceValue(setting, raw);
    if (next === undefined) input.value = String(value());
    else if (next !== value()) save(next);
    else input.value = String(next);
  }

  const reset = (
    <button
      type="button"
      class="btn btn--small plugin-setting__reset"
      aria-label={tr("resetOf", { name: setting.label })}
      disabled={!controller.isSet(setting.key)}
      onClick={() => save(undefined)}
    >
      {tr("resetDefault")}
    </button>
  );

  return (
    <div class="plugin-setting" data-setting={setting.key}>
      <div class="settings-row plugin-setting__row">
        <Switch>
          <Match when={setting.type === "toggle"}>
            <label class="check plugin-setting__label">
              <input
                id={id}
                type="checkbox"
                aria-describedby={hintId}
                checked={value() === true}
                onChange={(event) => save(event.currentTarget.checked)}
              />
              <span>{setting.label}</span>
            </label>
          </Match>
          <Match when={setting.type === "text"}>
            <label class="plugin-setting__label" for={id}>
              {setting.label}
            </label>
            <input
              id={id}
              type="text"
              class="input settings-row__input"
              aria-describedby={hintId}
              value={String(value())}
              onChange={(event) => change(event.currentTarget, event.currentTarget.value)}
            />
          </Match>
          <Match when={setting.type === "number"}>
            <label class="plugin-setting__label" for={id}>
              {setting.label}
            </label>
            <input
              id={id}
              type="number"
              class="input settings-row__number"
              aria-describedby={hintId}
              min={setting.min ?? undefined}
              max={setting.max ?? undefined}
              step={setting.step ?? undefined}
              value={String(value())}
              onChange={(event) => change(event.currentTarget, event.currentTarget.value)}
            />
          </Match>
          <Match when={setting.type === "select"}>
            <label class="plugin-setting__label" for={id}>
              {setting.label}
            </label>
            <select
              id={id}
              class="input settings-row__input"
              aria-describedby={hintId}
              value={String(value())}
              onChange={(event) => change(event.currentTarget, event.currentTarget.value)}
            >
              <For each={setting.options}>
                {(option) => <option value={option.value}>{option.label}</option>}
              </For>
            </select>
          </Match>
          <Match when={setting.type === "color"}>
            <label class="plugin-setting__label" for={id}>
              {setting.label}
            </label>
            <input
              id={id}
              type="color"
              class="plugin-setting__color"
              aria-describedby={hintId}
              value={String(value())}
              onChange={(event) => change(event.currentTarget, event.currentTarget.value)}
            />
            <input
              type="text"
              class="input plugin-setting__hex"
              aria-label={tr("colorHex", { name: setting.label })}
              spellcheck={false}
              value={String(value())}
              onChange={(event) => change(event.currentTarget, event.currentTarget.value)}
            />
          </Match>
        </Switch>
        {reset}
      </div>
      <Show when={setting.hint}>
        <p class="field__hint plugin-setting__hint" id={hintId}>
          {setting.hint}
        </p>
      </Show>
    </div>
  );
}

/**
 * One plugin's part of the settings page: a control for each setting it declares in
 * `plugin.json`, then the sections its extension added with own UI.
 */
export function PluginSettings(props: { plugin: string }) {
  const controller = () => settingsControllers()[props.plugin];
  const sections = () => settingsSections().filter((entry) => entry.plugin === props.plugin);
  return (
    <div class="form plugin-settings">
      <Show when={controller()} keyed>
        {(current) => (
          <Show when={current.declared.length > 0}>
            <div class="plugin-settings__declared">
              <For each={current.declared}>
                {(setting) => (
                  <SettingRow plugin={props.plugin} controller={current} setting={setting} />
                )}
              </For>
            </div>
          </Show>
        )}
      </Show>
      <For each={sections()}>
        {(entry) => (
          <section class="plugin-settings__section" aria-label={entry.section.title}>
            <h3 class="plugin-settings__title">{entry.section.title}</h3>
            <ExtensionMount plugin={entry.plugin} render={entry.section.render} />
          </section>
        )}
      </For>
    </div>
  );
}
