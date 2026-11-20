import type { PluginFolder } from "../bindings/PluginFolder";
import type { PluginInfo } from "../bindings/PluginInfo";

export type PluginKind = "store" | "extension" | "theme";
export type PluginStatus = "loaded" | "disabled" | "failed";

/** What a plugin brings, in display order. */
export function pluginKinds(plugin: PluginInfo): PluginKind[] {
  const kinds: PluginKind[] = [];
  if (plugin.providers.length > 0) kinds.push("store");
  if (plugin.hasExtension) kinds.push("extension");
  if (plugin.themes > 0) kinds.push("theme");
  return kinds;
}

/**
 * The plugin file path of the icon (for `pluginFileUrl`), or null without one. The load counter
 * in front makes the webview fetch a reloaded plugin's icon again.
 */
export function pluginIconPath(plugin: Pick<PluginInfo, "icon" | "revision">): string | null {
  const icon = plugin.icon?.trim();
  return icon ? `@${plugin.revision}/${icon.replace(/\\/g, "/")}` : null;
}

/** A disabled plugin is just off; an enabled one that is not running (or has an error) failed. */
export function pluginStatus(plugin: PluginInfo): PluginStatus {
  if (!plugin.enabled) return "disabled";
  if (plugin.error !== null || !plugin.loaded) return "failed";
  return "loaded";
}

/** Plugins by name (ignoring case), then by id. */
export function sortPlugins(plugins: readonly PluginInfo[]): PluginInfo[] {
  return plugins.toSorted(
    (a, b) =>
      a.name.localeCompare(b.name, undefined, { sensitivity: "base" }) || a.id.localeCompare(b.id),
  );
}

/** Whether the plugin asks for access beyond its own folders. */
export function hasPermissions(plugin: PluginInfo): boolean {
  return (
    plugin.permissions.read.length > 0 ||
    plugin.permissions.http.length > 0 ||
    plugin.permissions.folders.length > 0
  );
}

/** Where a folder a plugin asks for stands: chosen by the user, the plugin's default, or neither. */
export type FolderState = "chosen" | "default" | "unset";

export function folderState(folder: PluginFolder): FolderState {
  if (folder.path !== null) return "chosen";
  return folder.effectivePath !== null ? "default" : "unset";
}

/** The folder is set (chosen or by default) but there is nothing at that path. */
export function folderMissing(folder: PluginFolder): boolean {
  return folder.effectivePath !== null && !folder.exists;
}

/** The folder points nowhere usable: not set and without a default, or set to a missing folder. */
export function folderUnusable(folder: PluginFolder): boolean {
  return folderMissing(folder) || folderState(folder) === "unset";
}

/** The folder has no choice and its default is not there (or it has none): the plugin sees nothing. */
export function folderNeedsChoice(folder: PluginFolder): boolean {
  return folder.path === null && !folder.defaultExists;
}

/** The folders of an installed plugin the user still has to choose before its stores can list games. */
export function foldersToChoose(plugin: Pick<PluginInfo, "folders">): PluginFolder[] {
  return plugin.folders.filter(folderNeedsChoice);
}

/**
 * What the user changed in the install dialog, by folder key: a path, or null for "use the
 * default" (only kept for a folder that has a choice already, an update).
 */
export type FolderChoices = Record<string, string | null>;

/** `folder` as the install dialog shows it, with the user's change applied. */
export function applyFolderChoice(folder: PluginFolder, choices: FolderChoices): PluginFolder {
  const choice = choices[folder.key];
  if (choice === undefined) return folder;
  if (choice === null) {
    return {
      ...folder,
      path: null,
      effectivePath: folder.defaultPath,
      exists: folder.defaultExists,
    };
  }
  return { ...folder, path: choice, effectivePath: choice, exists: true };
}

/**
 * The choices after the user picked `path` (null: the default) for `folder`, which is the folder
 * as the preview reported it. A pick that changes nothing is not a choice.
 */
export function withFolderChoice(
  choices: FolderChoices,
  folder: PluginFolder,
  path: string | null,
): FolderChoices {
  const { [folder.key]: _old, ...rest } = choices;
  return path === folder.path ? rest : { ...rest, [folder.key]: path };
}

/** The `folders` argument of the install commands: key to path, a blank path for the default. */
export function installFolderMap(choices: FolderChoices): Record<string, string> | undefined {
  const entries = Object.entries(choices).map(([key, path]) => [key, path ?? ""] as const);
  return entries.length === 0 ? undefined : Object.fromEntries(entries);
}
