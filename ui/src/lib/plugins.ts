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
  return plugin.permissions.read.length > 0 || plugin.permissions.http.length > 0;
}
