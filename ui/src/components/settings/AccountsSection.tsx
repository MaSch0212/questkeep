import ChevronRight from "lucide-solid/icons/chevron-right";
import LogIn from "lucide-solid/icons/log-in";
import LogOut from "lucide-solid/icons/log-out";
import { createSignal, For, onMount, Show } from "solid-js";
import { createStore } from "solid-js/store";
import type { AccountStatus } from "../../bindings/AccountStatus";
import * as api from "../../lib/api";
import { errorMessage } from "../../lib/errors";
import { store } from "../../lib/store";
import { openLink } from "./openLink";

interface RowState {
  signingIn: boolean;
  confirmingSignOut: boolean;
  codeOpen: boolean;
  code: string;
  submitting: boolean;
}

const initialRow = (): RowState => ({
  signingIn: false,
  confirmingSignOut: false,
  codeOpen: false,
  code: "",
  submitting: false,
});

export function AccountsSection() {
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

  return (
    <div class="form">
      <p class="muted">
        Sign in to list games you own but have not installed. Sign-in tokens are stored in your
        system&apos;s credential store, not in the Questkeep data folder.
      </p>
      <Show when={accounts()} fallback={<p class="muted">Loading…</p>}>
        {(list) => (
          <>
            <Show when={list().length === 0}>
              <p class="muted">No store accounts available.</p>
            </Show>
            <For each={list()}>
              {(account) => (
                <div class="account" role="group" aria-label={account.name}>
                  <div class="account__head">
                    <div class="account__info">
                      <span class="account__name">{account.name}</span>
                      <span class="muted" role="status">
                        {account.signedIn
                          ? `Signed in as ${account.displayName ?? "your account"}`
                          : "Not signed in"}
                      </span>
                    </div>
                    <Show
                      when={account.signedIn}
                      fallback={
                        <button
                          type="button"
                          class="btn"
                          disabled={row(account.provider).signingIn}
                          aria-label={`Sign in to ${account.name}`}
                          onClick={() => void signIn(account)}
                        >
                          <LogIn class="icon" aria-hidden="true" />{" "}
                          {row(account.provider).signingIn ? "Waiting for sign-in…" : "Sign in"}
                        </button>
                      }
                    >
                      <Show
                        when={row(account.provider).confirmingSignOut}
                        fallback={
                          <button
                            type="button"
                            class="btn"
                            aria-label={`Sign out of ${account.name}`}
                            onClick={() => patch(account.provider, { confirmingSignOut: true })}
                          >
                            <LogOut class="icon" aria-hidden="true" /> Sign out
                          </button>
                        }
                      >
                        <div class="details__row">
                          <button
                            type="button"
                            class="btn btn--danger-solid"
                            onClick={() => void signOut(account)}
                          >
                            Confirm sign out
                          </button>
                          <button
                            type="button"
                            class="btn"
                            onClick={() => patch(account.provider, { confirmingSignOut: false })}
                          >
                            Cancel
                          </button>
                        </div>
                      </Show>
                    </Show>
                  </div>
                  <Show when={row(account.provider).signingIn}>
                    <p class="field__hint">Finish signing in in the window that opened.</p>
                  </Show>
                  <Show when={!account.signedIn && account.supportsCodeSignIn}>
                    <button
                      type="button"
                      class="link-btn account__disclosure"
                      aria-expanded={row(account.provider).codeOpen}
                      onClick={() =>
                        patch(account.provider, { codeOpen: !row(account.provider).codeOpen })
                      }
                    >
                      <ChevronRight
                        class="icon account__chevron"
                        classList={{ "account__chevron--open": row(account.provider).codeOpen }}
                        aria-hidden="true"
                      />{" "}
                      Sign in with a code instead
                    </button>
                    <Show when={row(account.provider).codeOpen}>
                      <form
                        class="form"
                        onSubmit={(event) => {
                          event.preventDefault();
                          void signInWithCode(account);
                        }}
                      >
                        <ol class="account__steps">
                          <li>Open the sign-in page in your normal browser and sign in.</li>
                          <li>
                            {account.provider === "epic"
                              ? "Copy the authorizationCode value from the page that follows."
                              : "Copy the code, or the whole address of the final page."}
                          </li>
                          <li>Paste it below and submit.</li>
                        </ol>
                        <Show when={account.codeUrl}>
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
                          <label class="field__label" for={`code-${account.provider}`}>
                            {account.name} sign-in code
                          </label>
                          <div class="field__row">
                            <input
                              id={`code-${account.provider}`}
                              class="input"
                              type="text"
                              autocomplete="off"
                              spellcheck={false}
                              value={row(account.provider).code}
                              onInput={(e) =>
                                patch(account.provider, { code: e.currentTarget.value })
                              }
                            />
                            <button
                              type="submit"
                              class="btn btn--primary"
                              disabled={
                                row(account.provider).submitting ||
                                row(account.provider).code.trim() === ""
                              }
                            >
                              {row(account.provider).submitting ? "Signing in…" : "Submit code"}
                            </button>
                          </div>
                        </div>
                      </form>
                    </Show>
                  </Show>
                </div>
              )}
            </For>
          </>
        )}
      </Show>
    </div>
  );
}
