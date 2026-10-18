import { openPath, openUrl } from "@tauri-apps/plugin-opener";
import { createSignal, For, onMount, Show } from "solid-js";
import { createStore } from "solid-js/store";
import type { Settings } from "../bindings/Settings";
import * as api from "../lib/api";
import { errorMessage } from "../lib/errors";
import { scanSummaryText } from "../lib/query";
import { textOf, withSteamSettings } from "../lib/steamSettings";
import { store } from "../lib/store";
import { EyeIcon, EyeOffIcon } from "./Icons";
import { Modal } from "./Modal";

const STEAM_KEY_URL = "https://steamcommunity.com/dev/apikey";

interface ScanState {
  busy: boolean;
  message?: string;
  failed?: boolean;
}

async function openLink(url: string) {
  try {
    await openUrl(url);
  } catch (error) {
    store.pushToast("error", errorMessage(error));
  }
}

async function openFolder(path: string) {
  try {
    await openPath(path);
  } catch (error) {
    store.pushToast("error", errorMessage(error));
  }
}

const scannable = () => store.providers().filter((p) => p.canScan);

export function SettingsDialog(props: { onClose: () => void }) {
  const [settings, setSettings] = createSignal<Settings | null>(null);
  const [scans, setScans] = createStore<Record<string, ScanState>>({});
  const [seeding, setSeeding] = createSignal(false);
  const [apiKey, setApiKey] = createSignal("");
  const [steamId, setSteamId] = createSignal("");
  const [showKey, setShowKey] = createSignal(false);
  const [savingSteam, setSavingSteam] = createSignal(false);

  onMount(() => {
    api
      .getSettings()
      .then((loaded) => {
        setSettings(loaded);
        setApiKey(textOf(loaded.providers.steam?.apiKey));
        setSteamId(textOf(loaded.providers.steam?.steamId));
      })
      .catch((error: unknown) => store.pushToast("error", errorMessage(error)));
  });

  async function toggle(key: keyof Settings, value: boolean) {
    const previous = settings();
    if (!previous) return;
    setSettings({ ...previous, [key]: value });
    try {
      await api.saveSettings({ ...previous, [key]: value });
    } catch (error) {
      setSettings(previous);
      store.pushToast("error", errorMessage(error));
    }
  }

  async function saveSteam() {
    const previous = settings();
    if (!previous || savingSteam()) return;
    const next = withSteamSettings(previous, apiKey(), steamId());
    const keyChanged =
      textOf(previous.providers.steam?.apiKey) !== textOf(next.providers.steam?.apiKey);
    const idChanged =
      textOf(previous.providers.steam?.steamId) !== textOf(next.providers.steam?.steamId);
    setSavingSteam(true);
    try {
      await api.saveSettings(next);
      setSettings(next);
      setApiKey(textOf(next.providers.steam?.apiKey));
      setSteamId(textOf(next.providers.steam?.steamId));
      store.pushToast("info", "Steam account saved");
    } catch (error) {
      store.pushToast("error", errorMessage(error));
      setSavingSteam(false);
      return;
    }
    setSavingSteam(false);
    // A new key or account can reveal owned games that were not visible before.
    if ((keyChanged || idChanged) && textOf(next.providers.steam?.apiKey) !== "") {
      try {
        const summary = await api.scanLibrary("steam");
        store.pushToast("info", `Steam: ${scanSummaryText(summary)}`);
        store.scheduleRefresh();
      } catch (error) {
        store.pushToast("error", errorMessage(error));
      }
    }
  }

  async function scan(providerId: string) {
    setScans(providerId, { busy: true });
    try {
      const summary = await api.scanLibrary(providerId);
      setScans(providerId, { busy: false, message: scanSummaryText(summary), failed: false });
      store.scheduleRefresh();
    } catch (error) {
      setScans(providerId, { busy: false, message: errorMessage(error), failed: true });
    }
  }

  async function seed() {
    setSeeding(true);
    try {
      const count = await api.seedFakeLibrary(5000);
      store.pushToast("info", `Seeded ${count} fake games`);
      store.scheduleRefresh();
    } catch (error) {
      store.pushToast("error", errorMessage(error));
    } finally {
      setSeeding(false);
    }
  }

  return (
    <Modal title="Settings" onClose={props.onClose}>
      <section class="settings-section">
        <h3>About</h3>
        <Show when={store.appInfo()} fallback={<p class="muted">Loading…</p>}>
          {(info) => (
            <dl class="facts">
              <dt>Version</dt>
              <dd>{info().version}</dd>
              <dt>Data directory</dt>
              <dd class="facts__path">
                <span class="selectable">{info().dataDir}</span>
                <button type="button" class="btn" onClick={() => void openFolder(info().dataDir)}>
                  Open folder
                </button>
              </dd>
              <dt>Logs directory</dt>
              <dd class="facts__path">
                <span class="selectable">{info().logsDir}</span>
                <button type="button" class="btn" onClick={() => void openFolder(info().logsDir)}>
                  Open folder
                </button>
              </dd>
            </dl>
          )}
        </Show>
      </section>

      <section class="settings-section">
        <h3>Library</h3>
        <Show when={settings()} fallback={<p class="muted">Loading…</p>}>
          {(current) => (
            <>
              <label class="check">
                <input
                  type="checkbox"
                  checked={current().scanOnStartup}
                  onChange={(e) => void toggle("scanOnStartup", e.currentTarget.checked)}
                />
                <span>Scan stores when the app starts</span>
              </label>
              <label class="check">
                <input
                  type="checkbox"
                  checked={current().downloadCovers}
                  onChange={(e) => void toggle("downloadCovers", e.currentTarget.checked)}
                />
                <span>Download missing covers</span>
              </label>
              <label class="check">
                <input
                  type="checkbox"
                  checked={current().fetchMetadata}
                  onChange={(e) => void toggle("fetchMetadata", e.currentTarget.checked)}
                />
                <span>Download store metadata</span>
              </label>
            </>
          )}
        </Show>
        <For each={scannable()}>
          {(provider) => (
            <div class="scan-row">
              <button
                type="button"
                class="btn"
                disabled={scans[provider.id]?.busy === true}
                onClick={() => void scan(provider.id)}
              >
                {scans[provider.id]?.busy
                  ? `Scanning ${provider.name}…`
                  : `Scan ${provider.name} now`}
              </button>
              <Show when={scans[provider.id]?.message}>
                <span
                  classList={{ "error-text": scans[provider.id]?.failed === true }}
                  role="status"
                >
                  {scans[provider.id]?.message}
                </span>
              </Show>
            </div>
          )}
        </For>
      </section>

      <section class="settings-section" aria-labelledby="steam-heading">
        <h3 id="steam-heading">Steam account</h3>
        <Show when={settings()} fallback={<p class="muted">Loading…</p>}>
          <form
            class="form"
            onSubmit={(event) => {
              event.preventDefault();
              void saveSteam();
            }}
          >
            <p class="muted">
              The Steam Web API key lets Questkeep list games you own but have not installed. Create
              one at{" "}
              <button type="button" class="link-btn" onClick={() => void openLink(STEAM_KEY_URL)}>
                {STEAM_KEY_URL}
              </button>
              .
            </p>
            <div class="field">
              <label class="field__label" for="steam-api-key">
                Steam Web API key
              </label>
              <div class="field__row">
                <input
                  id="steam-api-key"
                  class="input"
                  type={showKey() ? "text" : "password"}
                  autocomplete="off"
                  spellcheck={false}
                  value={apiKey()}
                  onInput={(e) => setApiKey(e.currentTarget.value)}
                />
                <button
                  type="button"
                  class="icon-btn icon-btn--bordered"
                  aria-label={showKey() ? "Hide API key" : "Show API key"}
                  aria-pressed={showKey()}
                  title={showKey() ? "Hide API key" : "Show API key"}
                  onClick={() => setShowKey((v) => !v)}
                >
                  {showKey() ? <EyeOffIcon /> : <EyeIcon />}
                </button>
              </div>
            </div>
            <div class="field">
              <label class="field__label" for="steam-id">
                SteamID64 (optional)
              </label>
              <input
                id="steam-id"
                class="input"
                type="text"
                inputmode="numeric"
                autocomplete="off"
                aria-describedby="steam-id-hint"
                value={steamId()}
                onInput={(e) => setSteamId(e.currentTarget.value)}
              />
              <span id="steam-id-hint" class="field__hint">
                Leave empty to use the account last signed in to Steam.
              </span>
            </div>
            <p class="field__hint">
              The key is stored unencrypted in the Questkeep data folder. Only use a key you are
              comfortable keeping there.
            </p>
            <div class="form__actions">
              <button type="submit" class="btn btn--primary" disabled={savingSteam()}>
                {savingSteam() ? "Saving…" : "Save Steam account"}
              </button>
            </div>
          </form>
        </Show>
      </section>

      <Show when={store.appInfo()?.devTools}>
        <section class="settings-section">
          <h3>Developer</h3>
          <button type="button" class="btn" disabled={seeding()} onClick={() => void seed()}>
            {seeding() ? "Seeding…" : "Seed 5,000 fake games"}
          </button>
        </section>
      </Show>
    </Modal>
  );
}
