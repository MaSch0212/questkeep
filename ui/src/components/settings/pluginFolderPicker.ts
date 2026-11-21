import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { translator } from "../../i18n";
import { settingsPlugins } from "../../i18n/settingsPlugins";

const tr = translator(settingsPlugins);

/**
 * A folder for one of a plugin's declared folders from the system dialog, starting in `from`
 * when given; null when cancelled.
 */
export async function pickPluginFolder(label: string, from: string | null): Promise<string | null> {
  const result = await openDialog({
    multiple: false,
    directory: true,
    title: tr("folderChooseTitle", { label }),
    ...(from === null ? {} : { defaultPath: from }),
  });
  return typeof result === "string" ? result : null;
}
