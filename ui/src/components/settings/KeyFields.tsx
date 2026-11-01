import Eye from "lucide-solid/icons/eye";
import EyeOff from "lucide-solid/icons/eye-off";
import { createEffect, createSignal, createUniqueId, Match, on, Show, Switch } from "solid-js";
import { translator } from "../../i18n";
import { settingsKeys } from "../../i18n/settingsKeys";
import * as api from "../../lib/api";
import { errorMessage } from "../../lib/errors";
import { openLink } from "./openLink";
import type { SettingsModel } from "./useSettings";

const tr = translator(settingsKeys);

export const TWITCH_CONSOLE_URL = "https://dev.twitch.tv/console/apps";
export const GRIDDB_KEY_URL = "https://www.steamgriddb.com/profile/preferences/api";

/** How to get an IGDB Client ID and Secret, step by step. */
export function IgdbSteps() {
  return (
    <ol class="key-steps">
      <li>
        {tr("signInAt")}{" "}
        <button type="button" class="link-btn" onClick={() => void openLink(TWITCH_CONSOLE_URL)}>
          dev.twitch.tv/console
        </button>{" "}
        {tr("igdbStep1After")}
      </li>
      <li>
        {tr("igdbStep2Before")} <code>http://localhost</code>
        {tr("igdbStep2After")}
      </li>
      <li>{tr("igdbStep3")}</li>
    </ol>
  );
}

type TestResult =
  | { kind: "idle" }
  | { kind: "testing" }
  | { kind: "ok" }
  | { kind: "error"; message: string };

/** The IGDB Client ID and Secret inputs plus "Test keys", bound to the settings model. */
export function IgdbFields(props: { model: SettingsModel }) {
  const idField = createUniqueId();
  const secretField = createUniqueId();
  const [showSecret, setShowSecret] = createSignal(false);
  const [result, setResult] = createSignal<TestResult>({ kind: "idle" });

  // A result is about the values it tested; typing clears it.
  createEffect(
    on(
      () => [props.model.igdbClientId(), props.model.igdbClientSecret()],
      () => setResult({ kind: "idle" }),
      { defer: true },
    ),
  );

  async function test() {
    setResult({ kind: "testing" });
    try {
      await api.testIgdbCredentials(props.model.igdbClientId(), props.model.igdbClientSecret());
      setResult({ kind: "ok" });
    } catch (error) {
      setResult({ kind: "error", message: errorMessage(error) });
    }
  }

  const failure = () => {
    const current = result();
    return current.kind === "error" ? current.message : null;
  };

  const filled = () =>
    props.model.igdbClientId().trim() !== "" && props.model.igdbClientSecret().trim() !== "";

  return (
    <>
      <div class="key-fields">
        <div class="field">
          <label class="field__label" for={idField}>
            {tr("igdbClientId")}
          </label>
          <input
            id={idField}
            class="input"
            type="text"
            autocomplete="off"
            spellcheck={false}
            value={props.model.igdbClientId()}
            onInput={(e) => props.model.setIgdbClientId(e.currentTarget.value)}
          />
        </div>
        <div class="field">
          <label class="field__label" for={secretField}>
            {tr("igdbClientSecret")}
          </label>
          <div class="field__row">
            <input
              id={secretField}
              class="input"
              type={showSecret() ? "text" : "password"}
              autocomplete="off"
              spellcheck={false}
              value={props.model.igdbClientSecret()}
              onInput={(e) => props.model.setIgdbClientSecret(e.currentTarget.value)}
            />
            <button
              type="button"
              class="icon-btn icon-btn--bordered"
              aria-label={showSecret() ? tr("hideIgdbSecret") : tr("showIgdbSecret")}
              aria-pressed={showSecret()}
              title={showSecret() ? tr("hideSecret") : tr("showSecret")}
              onClick={() => setShowSecret((v) => !v)}
            >
              <Show when={showSecret()} fallback={<Eye class="icon" aria-hidden="true" />}>
                <EyeOff class="icon" aria-hidden="true" />
              </Show>
            </button>
          </div>
        </div>
      </div>
      <div class="key-test">
        <button
          type="button"
          class="btn"
          disabled={!filled() || result().kind === "testing"}
          onClick={() => void test()}
        >
          {result().kind === "testing" ? tr("testing") : tr("testKeys")}
        </button>
        <span class="key-test__result" role="status">
          <Switch>
            <Match when={result().kind === "ok"}>
              <span class="key-test__ok">{tr("keysWork")}</span>
            </Match>
            <Match when={failure()}>
              {(message) => <span class="key-test__error">{message()}</span>}
            </Match>
          </Switch>
        </span>
      </div>
    </>
  );
}

/** How to get a SteamGridDB key, plus its input, bound to the settings model. */
export function GridDbField(props: { model: SettingsModel }) {
  const field = createUniqueId();
  const [showKey, setShowKey] = createSignal(false);
  return (
    <>
      <ol class="key-steps">
        <li>
          {tr("signInAt")}{" "}
          <button type="button" class="link-btn" onClick={() => void openLink(GRIDDB_KEY_URL)}>
            steamgriddb.com
          </button>
          {tr("griddbStepAfter")}
        </li>
      </ol>
      <div class="field">
        <label class="field__label" for={field}>
          {tr("griddbKey")}
        </label>
        <div class="field__row">
          <input
            id={field}
            class="input"
            type={showKey() ? "text" : "password"}
            autocomplete="off"
            spellcheck={false}
            value={props.model.gridDbKey()}
            onInput={(e) => props.model.setGridDbKey(e.currentTarget.value)}
          />
          <button
            type="button"
            class="icon-btn icon-btn--bordered"
            aria-label={showKey() ? tr("hideGriddbKey") : tr("showGriddbKey")}
            aria-pressed={showKey()}
            title={showKey() ? tr("hideKey") : tr("showKey")}
            onClick={() => setShowKey((v) => !v)}
          >
            <Show when={showKey()} fallback={<Eye class="icon" aria-hidden="true" />}>
              <EyeOff class="icon" aria-hidden="true" />
            </Show>
          </button>
        </div>
      </div>
    </>
  );
}
