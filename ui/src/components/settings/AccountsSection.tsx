import Check from "lucide-solid/icons/check";
import ChevronRight from "lucide-solid/icons/chevron-right";
import KeyRound from "lucide-solid/icons/key-round";
import LogIn from "lucide-solid/icons/log-in";
import LogOut from "lucide-solid/icons/log-out";
import RefreshCw from "lucide-solid/icons/refresh-cw";
import { createSignal, For, onMount, Show } from "solid-js";
import { createStore } from "solid-js/store";
import type { AccountStatus } from "../../bindings/AccountStatus";
import { accountState, connectedSummary, storeRows, type StoreRow } from "../../lib/accountState";
import { credentialsSaved } from "../../lib/providerSettings";
import * as api from "../../lib/api";
import { errorMessage } from "../../lib/errors";
import { store } from "../../lib/store";
import { isProviderSyncing } from "../../lib/syncStatus";
import { SourceIcon } from "../SourceIcon";
import { CredentialsForm } from "./CredentialsForm";
import { openLink } from "./openLink";
import { StoreOptions } from "./StoreOptions";
import type { SettingsModel } from "./useSettings";

interface RowState {
  signingIn: boolean;
  confirmingSignOut: boolean;
  /** The row shows its settings and account actions. */
  open: boolean;
  codeOpen: boolean;
  /** The store's credentials form is open. */
  keyOpen: boolean;
  code: string;
  submitting: boolean;
}

const initialRow = (): RowState => ({
  signingIn: false,
  confirmingSignOut: false,
  open: false,
  codeOpen: false,
  keyOpen: false,
  code: "",
  submitting: false,
});

/** Progress, the result and failures arrive as scan events and end up in the sync summary. */
async function syncStore(id: string) {
  try {
    await api.scanLibrary(id);
    store.scheduleRefresh();
  } catch {
    // Already reported through the scanFailed event.
  }
}

/** The line under a store's name: its account, or that its client is missing. */
function storeDetail(entry: StoreRow, accountDetail: string | null): string {
  if (!entry.installed && entry.canScan) {
    return entry.account === null
      ? "Not installed on this PC"
      : entry.account.signedIn || entry.syncable
        ? `Not installed on this PC · ${accountDetail ?? ""}`
        : "Not installed on this PC. Sign in to sync the games you own.";
  }
  return accountDetail ?? "Imports the games installed on this PC";
}

/** Whether the row has anything below its head: options, sign-out, credentials or code sign-in. */
function expandable(entry: StoreRow): boolean {
  return (
    entry.settings.options.length > 0 ||
    (entry.account !== null &&
      (entry.account.signedIn ||
        entry.account.supportsCodeSignIn ||
        entry.settings.credentials !== null))
  );
}

/** Every store in one place: its sign-in (or its credentials), its options and a sync button. */
export function AccountsSection(props: { model: SettingsModel }) {
  const [accounts, setAccounts] = createSignal<AccountStatus[] | null>(null);
  const [rows, setRows] = createStore<Record<string, RowState>>({});
  const row = (provider: string) => rows[provider] ?? initialRow();

  async function refresh() {
    try {
      setAccounts(await api.accountStatus());
      // Signing in again ends a "signed out" warning in the sync panel.
      void store.loadAccountWarnings();
    } catch (error) {
      store.pushToast("error", errorMessage(error));
      setAccounts((current) => current ?? []);
    }
  }

  onMount(() => void refresh());

  /** The store's credentials when they are saved. */
  const savedCredentials = (entry: StoreRow) => {
    const credentials = entry.settings.credentials;
    return credentials !== null && credentialsSaved(props.model.settings(), entry.id, credentials)
      ? credentials
      : null;
  };
  const state = (entry: StoreRow, account: AccountStatus) =>
    accountState(account, savedCredentials(entry));

  function patch(provider: string, change: Partial<RowState>) {
    setRows(provider, { ...row(provider), ...change });
  }

  async function signIn(account: AccountStatus) {
    patch(account.provider, { signingIn: true });
    try {
      if (await api.signIn(account.provider)) {
        store.pushToast("info", `Signed in to ${account.name}. Importing your library…`);
        store.scheduleRefresh();
        await refresh();
        void store.loadProviders();
      }
    } catch (error) {
      store.pushToast("error", errorMessage(error));
    } finally {
      patch(account.provider, { signingIn: false });
    }
  }

  async function signInWithCode(account: AccountStatus) {
    const code = row(account.provider).code.trim();
    if (code === "") return;
    patch(account.provider, { submitting: true });
    try {
      await api.signInWithCode(account.provider, code);
      store.pushToast("info", `Signed in to ${account.name}. Importing your library…`);
      patch(account.provider, { code: "", codeOpen: false });
      store.scheduleRefresh();
      await refresh();
      void store.loadProviders();
    } catch (error) {
      store.pushToast("error", errorMessage(error));
    } finally {
      patch(account.provider, { submitting: false });
    }
  }

  async function signOut(account: AccountStatus) {
    patch(account.provider, { confirmingSignOut: false });
    try {
      await api.signOut(account.provider);
      store.pushToast("info", `Signed out of ${account.name}`);
      await refresh();
      void store.loadProviders();
    } catch (error) {
      store.pushToast("error", errorMessage(error));
    }
  }

  const stores = () => storeRows(store.providers(), accounts() ?? []);
  const connection = (entry: StoreRow) =>
    entry.account === null ? null : state(entry, entry.account).connection;

  return (
    <div class="form">
      <p class="muted">
        Sign in to list games you own but have not installed. Sign-in tokens are stored in your
        system&apos;s credential store, not in the Questkeep data folder. Steam, Battle.net and EA
        are the exception: they only have their website sign-in, so Questkeep keeps those browser
        sessions in its data folder and deletes them when you sign out.
      </p>
      <Show when={accounts()} fallback={<p class="muted">Loading…</p>}>
        <Show when={stores().length > 0} fallback={<p class="muted">No stores available.</p>}>
          <p class="account-summary">
            {connectedSummary(
              stores().flatMap((r) => (r.account === null ? [] : [state(r, r.account)])),
            )}
          </p>
        </Show>
        <For each={stores()}>
          {(entry) => {
            const syncing = () => isProviderSyncing(store.syncState(), entry.id);
            return (
              <div
                class="account"
                classList={{
                  "account--connected": connection(entry) !== null && connection(entry) !== "none",
                }}
                role="group"
                aria-label={entry.name}
              >
                {/* The whole head opens and closes the row; its buttons keep their own action, and
                    the chevron button is the keyboard path. */}
                <div
                  class="account__head"
                  classList={{
                    "account__head--toggle": expandable(entry),
                    "account__head--open": expandable(entry) && row(entry.id).open,
                  }}
                  onClick={(event) => {
                    const target = event.target as Element;
                    if (!expandable(entry) || target.closest("button, a, input, label")) return;
                    patch(entry.id, { open: !row(entry.id).open });
                  }}
                >
                  <Show
                    when={expandable(entry)}
                    fallback={<span class="account__toggle" aria-hidden="true" />}
                  >
                    <button
                      type="button"
                      class="icon-btn icon-btn--small account__toggle"
                      aria-label={`${entry.name} settings`}
                      title={row(entry.id).open ? "Hide settings" : "Show settings"}
                      aria-expanded={row(entry.id).open}
                      aria-controls={`store-${entry.id}-details`}
                      onClick={() => patch(entry.id, { open: !row(entry.id).open })}
                    >
                      <ChevronRight
                        class="icon account__chevron"
                        classList={{ "account__chevron--open": row(entry.id).open }}
                        aria-hidden="true"
                      />
                    </button>
                  </Show>
                  <SourceIcon source={entry.id} />
                  <div class="account__info">
                    <span class="account__name">{entry.name}</span>
                    <span class="muted account__detail">
                      {storeDetail(entry, entry.account && state(entry, entry.account).detail)}
                    </span>
                  </div>
                  <Show when={entry.account}>
                    {(account) => (
                      <span
                        class="account__badge"
                        classList={{
                          "account__badge--on": state(entry, account()).connection !== "none",
                        }}
                        role="status"
                      >
                        <Show when={state(entry, account()).connection === "signedIn"}>
                          <Check class="icon" aria-hidden="true" />
                        </Show>
                        <Show when={state(entry, account()).connection === "credentials"}>
                          <KeyRound class="icon" aria-hidden="true" />
                        </Show>
                        {state(entry, account()).badge}
                      </span>
                    )}
                  </Show>
                  <Show when={entry.syncable}>
                    <button
                      type="button"
                      class="icon-btn"
                      aria-label={syncing() ? `Syncing ${entry.name}…` : `Sync ${entry.name}`}
                      title={syncing() ? `Syncing ${entry.name}…` : `Sync ${entry.name}`}
                      disabled={syncing()}
                      onClick={() => void syncStore(entry.id)}
                    >
                      <RefreshCw
                        class="icon"
                        classList={{ "icon--spin": syncing() }}
                        aria-hidden="true"
                      />
                    </button>
                  </Show>
                  <Show when={entry.account !== null && connection(entry) === "none"}>
                    <button
                      type="button"
                      class="btn"
                      disabled={row(entry.id).signingIn}
                      aria-label={`Sign in to ${entry.name}`}
                      onClick={() => entry.account && void signIn(entry.account)}
                    >
                      <LogIn class="icon" aria-hidden="true" />{" "}
                      {row(entry.id).signingIn ? "Waiting for sign-in…" : "Sign in"}
                    </button>
                  </Show>
                </div>
                <Show when={row(entry.id).signingIn}>
                  <p class="field__hint">Finish signing in in the window that opened.</p>
                </Show>
                <Show when={expandable(entry) && row(entry.id).open}>
                  <div class="account__body" id={`store-${entry.id}-details`}>
                    <Show when={entry.account}>
                      {(account) => (
                        <>
                          <Show when={entry.settings.credentials}>
                            {(credentials) => (
                              <>
                                <Show when={state(entry, account()).connection === "credentials"}>
                                  <p class="field__hint">
                                    Remove the {credentials().noun} to sign in to {entry.name}{" "}
                                    instead.
                                  </p>
                                </Show>
                                <Show when={account().signedIn && savedCredentials(entry)}>
                                  <p class="field__hint">
                                    A {credentials().noun} is also saved; it is not needed while you
                                    are signed in.{" "}
                                    <button
                                      type="button"
                                      class="link-btn"
                                      disabled={props.model.savingCredentials() === entry.id}
                                      onClick={() =>
                                        void props.model.removeCredentials(
                                          entry.id,
                                          entry.name,
                                          credentials(),
                                        )
                                      }
                                    >
                                      Remove {credentials().noun}
                                    </button>
                                  </p>
                                </Show>
                                <Show when={!account().signedIn}>
                                  <button
                                    type="button"
                                    class="link-btn account__disclosure"
                                    aria-expanded={row(account().provider).keyOpen}
                                    onClick={() =>
                                      patch(account().provider, {
                                        keyOpen: !row(account().provider).keyOpen,
                                      })
                                    }
                                  >
                                    <ChevronRight
                                      class="icon account__chevron"
                                      classList={{
                                        "account__chevron--open": row(account().provider).keyOpen,
                                      }}
                                      aria-hidden="true"
                                    />{" "}
                                    {state(entry, account()).connection === "credentials"
                                      ? `Change ${credentials().noun}`
                                      : credentials().offer}
                                  </button>
                                  <Show when={row(account().provider).keyOpen}>
                                    <CredentialsForm
                                      providerId={entry.id}
                                      storeName={entry.name}
                                      credentials={credentials()}
                                      model={props.model}
                                    />
                                  </Show>
                                </Show>
                              </>
                            )}
                          </Show>
                          <Show when={!account().signedIn && account().supportsCodeSignIn}>
                            <button
                              type="button"
                              class="link-btn account__disclosure"
                              aria-expanded={row(account().provider).codeOpen}
                              onClick={() =>
                                patch(account().provider, {
                                  codeOpen: !row(account().provider).codeOpen,
                                })
                              }
                            >
                              <ChevronRight
                                class="icon account__chevron"
                                classList={{
                                  "account__chevron--open": row(account().provider).codeOpen,
                                }}
                                aria-hidden="true"
                              />{" "}
                              Sign in with a code instead
                            </button>
                            <Show when={row(account().provider).codeOpen}>
                              <form
                                class="form"
                                onSubmit={(event) => {
                                  event.preventDefault();
                                  void signInWithCode(account());
                                }}
                              >
                                <ol class="account__steps">
                                  <li>Open the sign-in page in your normal browser and sign in.</li>
                                  <li>
                                    {account().codeHint ??
                                      "Copy the code, or the whole address of the final page."}
                                  </li>
                                  <li>Paste it below and submit.</li>
                                </ol>
                                <Show when={account().codeUrl}>
                                  {(url) => (
                                    <div>
                                      <button
                                        type="button"
                                        class="btn"
                                        onClick={() => void openLink(url())}
                                      >
                                        Open sign-in page
                                      </button>
                                    </div>
                                  )}
                                </Show>
                                <div class="field">
                                  <label class="field__label" for={`code-${account().provider}`}>
                                    {account().name} sign-in code
                                  </label>
                                  <div class="field__row">
                                    <input
                                      id={`code-${account().provider}`}
                                      class="input"
                                      type="text"
                                      autocomplete="off"
                                      spellcheck={false}
                                      value={row(account().provider).code}
                                      onInput={(e) =>
                                        patch(account().provider, { code: e.currentTarget.value })
                                      }
                                    />
                                    <button
                                      type="submit"
                                      class="btn btn--primary"
                                      disabled={
                                        row(account().provider).submitting ||
                                        row(account().provider).code.trim() === ""
                                      }
                                    >
                                      {row(account().provider).submitting
                                        ? "Signing in…"
                                        : "Submit code"}
                                    </button>
                                  </div>
                                </div>
                              </form>
                            </Show>
                          </Show>
                        </>
                      )}
                    </Show>
                    <StoreOptions
                      providerId={entry.id}
                      options={entry.settings.options}
                      model={props.model}
                    />
                    {/* Ending the account comes last, away from the head. */}
                    <Show when={entry.account}>
                      {(account) => (
                        <>
                          <Show when={account().signedIn}>
                            <div class="account__actions">
                              <Show
                                when={row(account().provider).confirmingSignOut}
                                fallback={
                                  <button
                                    type="button"
                                    class="btn btn--danger"
                                    aria-label={`Sign out of ${account().name}`}
                                    onClick={() =>
                                      patch(account().provider, { confirmingSignOut: true })
                                    }
                                  >
                                    <LogOut class="icon" aria-hidden="true" /> Sign out
                                  </button>
                                }
                              >
                                <button
                                  type="button"
                                  class="btn"
                                  onClick={() =>
                                    patch(account().provider, { confirmingSignOut: false })
                                  }
                                >
                                  Cancel
                                </button>
                                <button
                                  type="button"
                                  class="btn btn--danger-solid"
                                  onClick={() => void signOut(account())}
                                >
                                  Confirm sign out
                                </button>
                              </Show>
                            </div>
                          </Show>
                          <Show
                            when={
                              state(entry, account()).connection === "credentials" &&
                              entry.settings.credentials
                            }
                          >
                            {(credentials) => (
                              <div class="account__actions">
                                <button
                                  type="button"
                                  class="btn btn--danger"
                                  disabled={props.model.savingCredentials() === entry.id}
                                  onClick={() =>
                                    void props.model.removeCredentials(
                                      entry.id,
                                      entry.name,
                                      credentials(),
                                    )
                                  }
                                >
                                  Remove {credentials().noun}
                                </button>
                              </div>
                            )}
                          </Show>
                        </>
                      )}
                    </Show>
                  </div>
                </Show>
              </div>
            );
          }}
        </For>
      </Show>
    </div>
  );
}
