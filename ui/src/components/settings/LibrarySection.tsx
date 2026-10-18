import { For, Show } from "solid-js";
import { createStore } from "solid-js/store";
import * as api from "../../lib/api";
import { errorMessage } from "../../lib/errors";
import { scanSummaryText } from "../../lib/query";
import { store } from "../../lib/store";

interface ScanState {
  busy: boolean;
  message?: string;
  failed?: boolean;
}

const scannable = () => store.providers().filter((p) => p.canScan);

export function LibrarySection() {
  const [scans, setScans] = createStore<Record<string, ScanState>>({});

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

  return (
    <>
      <Show when={scannable().length === 0}>
        <p class="muted">No stores to scan.</p>
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
              <span classList={{ "error-text": scans[provider.id]?.failed === true }} role="status">
                {scans[provider.id]?.message}
              </span>
            </Show>
          </div>
        )}
      </For>
    </>
  );
}
