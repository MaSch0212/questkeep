import { For, Show } from "solid-js";
import type { PluginPreview } from "../../bindings/PluginPreview";
import { translator } from "../../i18n";
import { settingsPlugins } from "../../i18n/settingsPlugins";
import { shellCommon } from "../../i18n/shellCommon";
import { Modal } from "../Modal";

const tr = translator(settingsPlugins);
const trCommon = translator(shellCommon);

interface PluginInstallDialogProps {
  /** The folder or file the plugin comes from. */
  path: string;
  plugin: PluginPreview;
  onInstall: () => void;
  onClose: () => void;
}

/**
 * Shows what a chosen plugin is (name, kinds, what its stores may reach) before it is installed.
 * Cancel has the focus, so Enter does not install by accident.
 */
export function PluginInstallDialog(props: PluginInstallDialogProps) {
  const plugin = () => props.plugin;
  const hasPermissions = () =>
    plugin().permissions.read.length > 0 || plugin().permissions.http.length > 0;

  return (
    <Modal title={tr("installTitle")} onClose={props.onClose}>
      <div class="plugin-preview">
        <div class="account__head">
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
        <dl class="plugin__details">
          <dt>{tr("source")}</dt>
          <dd class="selectable plugin-preview__path">{props.path}</dd>
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
            props.onInstall();
          }}
        >
          {tr("installConfirm")}
        </button>
      </div>
    </Modal>
  );
}
