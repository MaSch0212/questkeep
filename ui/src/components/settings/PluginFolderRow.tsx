import { Show } from "solid-js";
import type { PluginFolder } from "../../bindings/PluginFolder";
import { translator } from "../../i18n";
import { settingsPlugins } from "../../i18n/settingsPlugins";
import { folderMissing, folderState, folderUnusable } from "../../lib/plugins";

const tr = translator(settingsPlugins);

interface PluginFolderRowProps {
  folder: PluginFolder;
  /** An action is under way: the buttons stay off. */
  disabled: boolean;
  /** Shown under the row while it points nowhere usable (not set, or a missing folder). */
  unusableNote?: string;
  onChoose: (folder: PluginFolder) => void;
  onUseDefault: (folder: PluginFolder) => void;
}

/**
 * One folder a plugin asks for: what it is for, where it points to, and the buttons to choose
 * another folder or go back to the default. Shared by the plugin's settings and the install dialog.
 */
export function PluginFolderRow(props: PluginFolderRowProps) {
  const state = () => folderState(props.folder);
  return (
    <div
      class="plugin-folder"
      classList={{ "plugin-folder--missing": folderMissing(props.folder) }}
      role="group"
      aria-label={props.folder.label}
      data-plugin-folder={props.folder.key}
    >
      <div class="plugin-folder__text">
        <span class="plugin-folder__label">{props.folder.label}</span>
        <Show when={props.folder.hint}>
          {(hint) => <span class="muted plugin-folder__hint">{hint()}</span>}
        </Show>
        <span class="plugin-folder__path selectable" data-folder-state={state()}>
          {state() === "chosen" ? (
            props.folder.path
          ) : state() === "default" ? (
            tr("folderDefault", { path: props.folder.effectivePath ?? "" })
          ) : (
            <span class="muted">{tr("folderNotSet")}</span>
          )}
        </span>
        <Show when={folderMissing(props.folder)}>
          <span class="plugin__error" role="alert">
            {tr("folderNotFound")}
          </span>
        </Show>
        <Show when={folderUnusable(props.folder) && props.unusableNote}>
          {(note) => <span class="muted plugin-folder__note">{note()}</span>}
        </Show>
      </div>
      <div class="plugin-folder__actions">
        <button
          type="button"
          class="btn"
          aria-label={tr("folderChooseFor", { label: props.folder.label })}
          disabled={props.disabled}
          onClick={() => props.onChoose(props.folder)}
        >
          {tr("folderChoose")}
        </button>
        <Show when={props.folder.path !== null}>
          <button
            type="button"
            class="btn"
            aria-label={tr("folderUseDefaultFor", { label: props.folder.label })}
            disabled={props.disabled}
            onClick={() => props.onUseDefault(props.folder)}
          >
            {tr("folderUseDefault")}
          </button>
        </Show>
      </div>
    </div>
  );
}
