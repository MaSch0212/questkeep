import { createSignal, Show } from "solid-js";
import { Portal } from "solid-js/web";
import type { PluginInfo } from "../bindings/PluginInfo";
import type { RegistryPluginPreview } from "../bindings/RegistryPluginPreview";
import { translator } from "../i18n";
import { pluginBrowser } from "../i18n/pluginBrowser";
import * as api from "../lib/api";
import { errorMessage } from "../lib/errors";
import { store } from "../lib/store";
import { PluginInstallDialog } from "./settings/PluginInstallDialog";
import { scanAfterInstall } from "./settings/scanPluginStores";

const tr = translator(pluginBrowser);

/**
 * Installing a registry version, step by step: download and check it (`begin`), show the usual
 * install dialog with what the registry knows, install on confirmation. Render `dialog()` once.
 */
export function createRegistryInstall(onInstalled?: (plugin: PluginInfo) => void) {
  /** `id@version` while the zip is downloaded and checked. */
  const [preparing, setPreparing] = createSignal<string | null>(null);
  const [installing, setInstalling] = createSignal(false);
  const [pending, setPending] = createSignal<{
    id: string;
    version: string;
    preview: RegistryPluginPreview;
  } | null>(null);

  const busy = () => preparing() !== null || installing() || pending() !== null;

  async function begin(id: string, version: string) {
    if (busy()) return;
    setPreparing(`${id}@${version}`);
    try {
      setPending({ id, version, preview: await api.previewRegistryPlugin(id, version) });
    } catch (error) {
      store.pushToast("error", errorMessage(error));
    } finally {
      setPreparing(null);
    }
  }

  async function confirm(id: string, version: string, folders?: Record<string, string>) {
    setInstalling(true);
    try {
      const plugin = await (folders
        ? api.installRegistryPlugin(id, version, folders)
        : api.installRegistryPlugin(id, version));
      store.pushToast("info", tr("installedToast", { name: plugin.name, version }));
      onInstalled?.(plugin);
      void store.loadPluginUpdates().catch(() => undefined);
      await scanAfterInstall(plugin);
    } catch (error) {
      store.pushToast("error", errorMessage(error));
    } finally {
      setInstalling(false);
    }
  }

  /** The install dialog while a version waits for the go-ahead. In a portal, so a dialog around it keeps its own focus trap. */
  const dialog = () => (
    <Show when={pending()}>
      {(chosen) => {
        // The dialog closes before it confirms, which ends the <Show>: keep the value.
        const { id, version, preview } = chosen();
        return (
          <Portal>
            <PluginInstallDialog
              plugin={preview.preview}
              registry={preview}
              onInstall={(folders) => void confirm(id, version, folders)}
              onClose={() => setPending(null)}
            />
          </Portal>
        );
      }}
    </Show>
  );

  return { begin, busy, preparing, installing, dialog };
}
