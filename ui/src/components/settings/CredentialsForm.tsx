import Eye from "lucide-solid/icons/eye";
import EyeOff from "lucide-solid/icons/eye-off";
import { createSignal, For, Show } from "solid-js";
import type { CredentialField } from "../../bindings/CredentialField";
import type { StoreCredentials } from "../../bindings/StoreCredentials";
import { translator } from "../../i18n";
import { settingsAccounts } from "../../i18n/settingsAccounts";
import { openLink } from "./openLink";
import type { SettingsModel } from "./useSettings";

const tr = translator(settingsAccounts);

/** A store's credentials (Steam's Web API key), the alternative to signing in. */
export function CredentialsForm(props: {
  providerId: string;
  storeName: string;
  credentials: StoreCredentials;
  model: SettingsModel;
}) {
  const draft = (key: string) => props.model.credentialDraft(props.providerId, key);
  const saving = () => props.model.savingCredentials() === props.providerId;
  const missing = () =>
    props.credentials.fields.some((f) => f.required && draft(f.key).trim() === "");

  return (
    <form
      class="form"
      onSubmit={(event) => {
        event.preventDefault();
        void props.model.saveCredentials(props.providerId, props.storeName, props.credentials);
      }}
    >
      <p class="field__hint">
        {props.credentials.intro}
        <Show when={props.credentials.link}>
          {(link) => (
            <>
              {" "}
              {tr("getOneAt")}{" "}
              <button type="button" class="link-btn" onClick={() => void openLink(link())}>
                {link()}
              </button>
              .
            </>
          )}
        </Show>
      </p>
      <For each={props.credentials.fields}>
        {(field) => (
          <Field
            id={`${props.providerId}-${field.key}`}
            field={field}
            value={draft(field.key)}
            onInput={(value) => props.model.setCredentialDraft(props.providerId, field.key, value)}
          />
        )}
      </For>
      <div class="form__actions">
        <button type="submit" class="btn btn--primary" disabled={saving() || missing()}>
          {saving() ? tr("saving") : tr("saveNoun", { noun: props.credentials.noun })}
        </button>
      </div>
    </form>
  );
}

function Field(props: {
  id: string;
  field: CredentialField;
  value: string;
  onInput: (value: string) => void;
}) {
  const [shown, setShown] = createSignal(false);
  const input = () => (
    <input
      id={props.id}
      class="input"
      type={props.field.secret && !shown() ? "password" : "text"}
      inputmode={props.field.numeric ? "numeric" : undefined}
      autocomplete="off"
      spellcheck={false}
      aria-describedby={props.field.hint === null ? undefined : `${props.id}-hint`}
      value={props.value}
      onInput={(e) => props.onInput(e.currentTarget.value)}
    />
  );
  return (
    <div class="field">
      <label class="field__label" for={props.id}>
        {props.field.label}
      </label>
      <Show when={props.field.secret} fallback={input()}>
        <div class="field__row">
          {input()}
          <button
            type="button"
            class="icon-btn icon-btn--bordered"
            aria-label={tr(shown() ? "hideField" : "showField", { label: props.field.label })}
            aria-pressed={shown()}
            title={tr(shown() ? "hideField" : "showField", { label: props.field.label })}
            onClick={() => setShown((v) => !v)}
          >
            <Show when={shown()} fallback={<Eye class="icon" aria-hidden="true" />}>
              <EyeOff class="icon" aria-hidden="true" />
            </Show>
          </button>
        </div>
      </Show>
      <Show when={props.field.hint}>
        {(hint) => (
          <span id={`${props.id}-hint`} class="field__hint">
            {hint()}
          </span>
        )}
      </Show>
    </div>
  );
}
