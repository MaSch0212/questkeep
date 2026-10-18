import { openPath } from "@tauri-apps/plugin-opener";
import { createSignal, For, onMount, Show } from "solid-js";
import { createStore } from "solid-js/store";
import type { Settings } from "../bindings/Settings";
import * as api from "../lib/api";
import { errorMessage } from "../lib/errors";
import { scanSummaryText } from "../lib/query";
import { store } from "../lib/store";
import { Modal } from "./Modal";

interface ScanState {
  busy: boolean;
  message?: string;
  failed?: boolean;
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

  onMount(() => {
    api
      .getSettings()
      .then(setSettings)
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
