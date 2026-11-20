import { createSignal, For, Index, Show } from "solid-js";
import type { PluginPreview } from "../../bindings/PluginPreview";
import { translator } from "../../i18n";
import { settingsPlugins } from "../../i18n/settingsPlugins";
import { pluginBrowser } from "../../i18n/pluginBrowser";
import { pluginUpdates } from "../../i18n/pluginUpdates";
import type { PluginChanges } from "../../bindings/PluginChanges";
import { askedFor } from "../../lib/pluginUpdates";
import { shellCommon } from "../../i18n/shellCommon";
import type { PluginFolder } from "../../bindings/PluginFolder";
import { errorMessage } from "../../lib/errors";
import { formatReleaseDate, formatSize } from "../../lib/format";
import {
  applyFolderChoice,
  installFolderMap,
  withFolderChoice,
  type FolderChoices,
} from "../../lib/plugins";
import { store } from "../../lib/store";
import { Modal } from "../Modal";
import { PluginFolderRow } from "./PluginFolderRow";
import { PluginIcon } from "./PluginIcon";
import { pickPluginFolder } from "./pluginFolderPicker";

const tr = translator(settingsPlugins);
const trCommon = translator(shellCommon);
const trRegistry = translator(pluginBrowser);
const trUpdates = translator(pluginUpdates);

/** What the registry adds to the preview of one of its versions. */
export interface RegistryExtras {
  released: string | null;
  notes: string | null;
  size: number;
  /** Installing it keeps the plugin from being updated automatically. */
  pinned: boolean;
  /** An installed version is newer: the plugin's data is backed up first. */
  downgrade: boolean;
  /** What this version asks for beyond the installed one. */
  changes?: PluginChanges;
}

interface PluginInstallDialogProps {
  /** The folder or file the plugin comes from (none for a registry download). */
  path?: string;
  /** Set for a version from the plugin registry. */
  registry?: RegistryExtras;
  plugin: PluginPreview;
  /** `folders`: what the user chose for the plugin's folders (key to path, blank for the default); undefined when nothing was changed. */
  onInstall: (folders?: Record<string, string>) => void;
  onClose: () => void;
}

/**
 * Shows what a chosen plugin is (name, kinds, what its stores may reach) before it is installed.
 * Cancel has the focus, so Enter does not install by accident.
 */
export function PluginInstallDialog(props: PluginInstallDialogProps) {
  const plugin = () => props.plugin;
  /** What the user picked for the plugin's folders here; the rest stays as the preview says. */
  const [choices, setChoices] = createSignal<FolderChoices>({});
  const folders = () => plugin().folders.map((folder) => applyFolderChoice(folder, choices()));
  const original = (key: string) => plugin().folders.find((f) => f.key === key);

  async function chooseFolder(shown: PluginFolder) {
    const folder = original(shown.key);
    if (!folder) return;
    try {
      const path = await pickPluginFolder(folder.label, shown.exists ? shown.effectivePath : null);
      if (path !== null) setChoices((current) => withFolderChoice(current, folder, path));
    } catch (error) {
      store.pushToast("error", errorMessage(error));
    }
  }

  function useDefaultFolder(shown: PluginFolder) {
    const folder = original(shown.key);
    if (folder) setChoices((current) => withFolderChoice(current, folder, null));
  }

  const hasPermissions = () =>
    plugin().permissions.read.length > 0 ||
    plugin().permissions.http.length > 0 ||
    plugin().permissions.folders.length > 0;

  return (
    <Modal title={tr("installTitle")} onClose={props.onClose}>
      <div class="plugin-preview">
        <div class="account__head">
          <PluginIcon src={plugin().icon} />
          <div class="account__info">
            <span class="account__name">
              {plugin().name} <span class="muted plugin__version">{plugin().version}</span>
            </span>
            <Show when={plugin().author}>
              {(author) => (
                <span class="muted account__detail">{tr("by", { author: author() })}</span>
              )}
            </Show>
          </div>
          <Show when={plugin().stores > 0}>
            <span class="account__badge">{tr("kindStore")}</span>
          </Show>
          <Show when={plugin().hasExtension}>
            <span class="account__badge">{tr("kindExtension")}</span>
          </Show>
          <Show when={plugin().themes.length > 0}>
            <span class="account__badge">{tr("kindTheme")}</span>
          </Show>
        </div>
        <Show when={plugin().description}>
          {(description) => <p class="plugin__description">{description()}</p>}
        </Show>
        <Show when={plugin().installedVersion}>
          {(version) => (
            <p class="plugin-preview__replaces">{tr("installReplaces", { version: version() })}</p>
          )}
        </Show>
        <Show when={props.registry?.changes?.needsReview ? props.registry.changes : null}>
          {(changes) => (
            <p class="plugin-preview__replaces" data-plugin-needs-review>
              {trUpdates("reviewNeeded", { what: askedFor(changes()) })}
            </p>
          )}
        </Show>
        <Show when={props.registry?.downgrade}>
          <p class="plugin-preview__replaces">{trRegistry("downgradeNotice")}</p>
        </Show>
        <Show when={props.registry?.pinned}>
          <p class="plugin-preview__replaces">{trRegistry("pinNotice")}</p>
        </Show>
        <dl class="plugin__details">
          <Show when={props.path}>
            {(path) => (
              <>
                <dt>{tr("source")}</dt>
                <dd class="selectable plugin-preview__path">{path()}</dd>
              </>
            )}
          </Show>
          <Show when={props.registry}>
            {(registry) => (
              <>
                <Show when={registry().released}>
                  {(released) => (
                    <>
                      <dt>{trRegistry("released")}</dt>
                      <dd>{formatReleaseDate(released())}</dd>
                    </>
                  )}
                </Show>
                <dt>{trRegistry("size")}</dt>
                <dd>{formatSize(registry().size)}</dd>
                <Show when={registry().notes}>
                  {(notes) => (
                    <>
                      <dt>{trRegistry("notes")}</dt>
                      <dd class="plugin-prose">{notes()}</dd>
                    </>
                  )}
                </Show>
              </>
            )}
          </Show>
          <Show when={plugin().themes.length > 0}>
            <dt>{tr("installThemes")}</dt>
            <dd class="selectable">{plugin().themes.join(", ")}</dd>
          </Show>
          <Show when={plugin().stores > 0}>
            <dt>{tr("permissions")}</dt>
            <dd>
              <Show
                when={hasPermissions()}
                fallback={<span class="muted">{tr("noPermissions")}</span>}
              >
                <Show when={plugin().permissions.read.length > 0}>
                  <span class="plugin__perm-title">{tr("readFolders")}</span>
                  <ul class="plugin__perms selectable">
                    <For each={plugin().permissions.read}>{(path) => <li>{path}</li>}</For>
                  </ul>
                </Show>
                <Show when={plugin().permissions.folders.length > 0}>
                  <ul class="plugin__perms selectable" data-plugin-folder-requests>
                    <For each={plugin().permissions.folders}>
                      {(folder) => <li>{tr("folderRequest", { label: folder.label })}</li>}
                    </For>
                  </ul>
                </Show>
                <Show when={plugin().permissions.http.length > 0}>
                  <span class="plugin__perm-title">{tr("httpHosts")}</span>
                  <ul class="plugin__perms selectable">
                    <For each={plugin().permissions.http}>{(host) => <li>{host}</li>}</For>
                  </ul>
                </Show>
              </Show>
            </dd>
          </Show>
        </dl>
        <Show when={folders().length > 0}>
          <div class="plugin-folders" data-plugin-folders>
            <span class="plugin__perm-title">{tr("installFolders")}</span>
            <p class="muted plugin-folders__intro">{tr("installFoldersHint")}</p>
            {/* By position, so a row (and its focus) stays while its folder changes. */}
            <Index each={folders()}>
              {(folder) => (
                <PluginFolderRow
                  folder={folder()}
                  disabled={false}
                  unusableNote={tr("installFolderLater")}
                  onChoose={(shown) => void chooseFolder(shown)}
                  onUseDefault={useDefaultFolder}
                />
              )}
            </Index>
          </div>
        </Show>
        <p class="muted plugin-preview__trust">{tr("installMessage")}</p>
      </div>
      <div class="form__actions">
        <button type="button" class="btn" data-autofocus onClick={props.onClose}>
          {trCommon("cancel")}
        </button>
        <button
          type="button"
          class="btn btn--primary"
          onClick={() => {
            props.onClose();
            props.onInstall(installFolderMap(choices()));
          }}
        >
          {tr("installConfirm")}
        </button>
      </div>
    </Modal>
  );
}
