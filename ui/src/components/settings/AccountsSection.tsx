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
import * as api from "../../lib/api";
import { errorMessage } from "../../lib/errors";
import { store } from "../../lib/store";
import { isProviderSyncing } from "../../lib/syncStatus";
import { SourceIcon } from "../SourceIcon";
import { openLink } from "./openLink";
import { SteamKeyForm } from "./SteamKeyForm";
import { StoreOptions } from "./StoreOptions";
import type { SettingsModel } from "./useSettings";

interface RowState {
  signingIn: boolean;
  confirmingSignOut: boolean;
  codeOpen: boolean;
  /** Steam only: the API key form is open. */
  keyOpen: boolean;
  code: string;
  submitting: boolean;
}

const initialRow = (): RowState => ({
  signingIn: false,
  confirmingSignOut: false,
  codeOpen: false,
  keyOpen: false,
  code: "",
  submitting: false,
});

/** What to copy after signing in on the store's page, per store. */
export function codeHint(provider: string): string {
  switch (provider) {
    case "epic":
      return "Copy the authorizationCode value from the page that follows.";
    case "xbox":
      return "The browser ends on a blank page. Copy the whole address from its address bar.";
    case "amazon":
      return "Amazon then shows its front page. Copy the whole address from the address bar.";
    default:
      return "Copy the code, or the whole address of the final page.";
  }
}

/** Progress, the result and failures arrive as scan events and end up in the sync summary. */
async function syncStore(id: string) {
  try {
    await api.scanLibrary(id);
    store.scheduleRefresh();
  } catch {
    // Already reported through the scanFailed event.
  }
}

/** Every store in one place: its sign-in (or Steam's API key), its options and a sync button. */
export function AccountsSection(props: { model: SettingsModel }) {
  const [accounts, setAccounts] = createSignal<AccountStatus[] | null>(null);
  const [rows, setRows] = createStore<Record<string, RowState>>({});
  const row = (provider: string) => rows[provider] ?? initialRow();

  async function refresh() {
    try {
      setAccounts(await api.accountStatus());
    } catch (error) {
      store.pushToast("error", errorMessage(error));
      setAccounts((current) => current ?? []);
    }
  }

  onMount(() => void refresh());

  const state = (account: AccountStatus) =>
    accountState(account, account.provider === "steam" && props.model.steamKeySaved());

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
    } catch (error) {
      store.pushToast("error", errorMessage(error));
    }
  }

  const stores = () => storeRows(store.providers(), accounts() ?? []);
  const connection = (entry: StoreRow) =>
    entry.account === null ? null : state(entry.account).connection;

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
              stores().flatMap((r) => (r.account === null ? [] : [state(r.account)])),
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
                <div class="account__head">
                  <SourceIcon source={entry.id} />
                  <div class="account__info">
                    <span class="account__name">{entry.name}</span>
                    <span class="muted account__detail">
                      {entry.account === null
                        ? "Imports the games installed on this PC"
                        : state(entry.account).detail}
                    </span>
                  </div>
                  <Show when={entry.account}>
                    {(account) => (
                      <span
                        class="account__badge"
                        classList={{
                          "account__badge--on": state(account()).connection !== "none",
                        }}
                        role="status"
                      >
                        <Show when={state(account()).connection === "signedIn"}>
                          <Check class="icon" aria-hidden="true" />
                        </Show>
                        <Show when={state(account()).connection === "apiKey"}>
                          <KeyRound class="icon" aria-hidden="true" />
                        </Show>
                        {state(account()).badge}
                      </span>
                    )}
                  </Show>
                  <Show when={entry.canScan}>
                    <button
                      type="button"
                      class="icon-btn icon-btn--small"
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
                  <Show when={entry.account}>
                    {(account) => (
                      <Show
                        when={account().signedIn}
                        fallback={
                          <Show
                            when={state(account()).connection === "apiKey"}
                            fallback={
                              <button
                                type="button"
                                class="btn"
                                disabled={row(account().provider).signingIn}
                                aria-label={`Sign in to ${account().name}`}
                                onClick={() => void signIn(account())}
                              >
                                <LogIn class="icon" aria-hidden="true" />{" "}
                                {row(account().provider).signingIn
                                  ? "Waiting for sign-in…"
                                  : "Sign in"}
                              </button>
                            }
                          >
                            <button
                              type="button"
                              class="btn"
                              disabled={props.model.savingSteam()}
                              onClick={() => void props.model.removeSteamKey()}
                            >
                              Remove key
                            </button>
                          </Show>
                        }
                      >
                        <Show
                          when={row(account().provider).confirmingSignOut}
                          fallback={
                            <button
                              type="button"
                              class="btn"
                              aria-label={`Sign out of ${account().name}`}
                              onClick={() => patch(account().provider, { confirmingSignOut: true })}
                            >
                              <LogOut class="icon" aria-hidden="true" /> Sign out
                            </button>
                          }
                        >
                          <div class="details__row">
                            <button
                              type="button"
                              class="btn btn--danger-solid"
                              onClick={() => void signOut(account())}
                            >
                              Confirm sign out
                            </button>
                            <button
                              type="button"
                              class="btn"
                              onClick={() =>
                                patch(account().provider, { confirmingSignOut: false })
                              }
                            >
                              Cancel
                            </button>
                          </div>
                        </Show>
                      </Show>
                    )}
                  </Show>
                </div>
                <Show when={entry.account}>
                  {(account) => (
                    <>
                      <Show when={row(account().provider).signingIn}>
                        <p class="field__hint">Finish signing in in the window that opened.</p>
                      </Show>
                      <Show when={account().provider === "steam"}>
                        <Show when={state(account()).connection === "apiKey"}>
                          <p class="field__hint">Remove the key to sign in to Steam instead.</p>
                        </Show>
                        <Show when={account().signedIn && props.model.steamKeySaved()}>
                          <p class="field__hint">
                            A Steam Web API key is also saved; it is not needed while you are signed
                            in.{" "}
                            <button
                              type="button"
                              class="link-btn"
                              disabled={props.model.savingSteam()}
                              onClick={() => void props.model.removeSteamKey()}
                            >
                              Remove key
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
                            {state(account()).connection === "apiKey"
                              ? "Change key"
                              : "Use a Steam Web API key instead"}
                          </button>
                          <Show when={row(account().provider).keyOpen}>
                            <SteamKeyForm model={props.model} />
                          </Show>
                        </Show>
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
                              <li>{codeHint(account().provider)}</li>
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
                <StoreOptions providerId={entry.id} model={props.model} />
              </div>
            );
          }}
        </For>
      </Show>
    </div>
  );
}
