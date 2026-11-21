import { For } from "solid-js";
import type { PluginFolder } from "../../bindings/PluginFolder";
import type { PluginInfo } from "../../bindings/PluginInfo";
import { translator } from "../../i18n";
import { settingsPlugins } from "../../i18n/settingsPlugins";
import { PluginFolderRow } from "./PluginFolderRow";

const tr = translator(settingsPlugins);

interface PluginFoldersProps {
  plugin: PluginInfo;
  /** An action on the plugin is under way: its buttons stay off. */
  disabled: boolean;
  onChoose: (folder: PluginFolder) => void;
  onUseDefault: (folder: PluginFolder) => void;
}

/**
 * The folders a plugin asks the user for (`permissions.folders`): what each is for, where it
 * points to now, and the buttons to choose another folder or go back to the default.
 */
export function PluginFolders(props: PluginFoldersProps) {
  return (
    <div class="plugin-folders" data-plugin-folders={props.plugin.id}>
      <span class="plugin__perm-title">{tr("folders")}</span>
      <p class="muted plugin-folders__intro">{tr("foldersHint")}</p>
      <For each={props.plugin.folders}>
        {(folder) => (
          <PluginFolderRow
            folder={folder}
            disabled={props.disabled}
            onChoose={props.onChoose}
            onUseDefault={props.onUseDefault}
          />
        )}
      </For>
    </div>
  );
}
