import type { PluginInfo } from "../../bindings/PluginInfo";
import { translator } from "../../i18n";
import { settingsPlugins } from "../../i18n/settingsPlugins";
import * as api from "../../lib/api";
import { errorMessage } from "../../lib/errors";
import { foldersToChoose } from "../../lib/plugins";
import { scanSummaryText } from "../../lib/query";
import { store } from "../../lib/store";

const tr = translator(settingsPlugins);

/**
 * After an install: scans the plugin's stores, unless a folder it asks for is still unset and its
 * default is not there. Scanning would only fail then, so the toast says where to choose it.
 */
export async function scanAfterInstall(plugin: PluginInfo) {
  const missing = foldersToChoose(plugin);
  if (missing.length > 0 && plugin.providers.length > 0) {
    store.pushToast(
      "info",
      tr("folderSetupHint", { name: plugin.name, folders: missing.map((f) => f.label).join(", ") }),
    );
    return;
  }
  await scanPluginStores(plugin);
}

/** Scans the plugin's stores, so their games show up (or change) right away. */
export async function scanPluginStores(plugin: PluginInfo) {
  await Promise.all(
    plugin.providers.map(async (id) => {
      try {
        const summary = await api.scanLibrary(id);
        const name = store.providers().find((p) => p.id === id)?.name ?? id;
        store.pushToast("info", tr("scanned", { name, summary: scanSummaryText(summary) }));
        store.scheduleRefresh();
      } catch (error) {
        store.pushToast("error", errorMessage(error));
      }
    }),
  );
}
