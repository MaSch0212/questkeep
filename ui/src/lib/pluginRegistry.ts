import type { RegistryPluginView } from "../bindings/RegistryPluginView";
import type { RegistryVersionView } from "../bindings/RegistryVersionView";
import type { PluginKind } from "./plugins";

/** What the search box and the filter chips ask for. */
export interface RegistryFilter {
  query: string;
  /** Show plugins that bring any of these (none: all kinds). */
  kinds: ReadonlySet<PluginKind>;
  installed: boolean;
  updates: boolean;
}

export const NO_FILTER: RegistryFilter = {
  query: "",
  kinds: new Set(),
  installed: false,
  updates: false,
};

const KINDS: readonly PluginKind[] = ["store", "extension", "theme"];

/** The kinds a registry plugin brings, in display order (unknown ones are left out). */
export function registryKinds(plugin: RegistryPluginView): PluginKind[] {
  return KINDS.filter((kind) => plugin.kinds.includes(kind));
}

/** Whether every word of the query is in the plugin's name, summary, author or tags. */
export function matchesQuery(plugin: RegistryPluginView, query: string): boolean {
  const words = query.toLocaleLowerCase().split(/\s+/).filter(Boolean);
  if (words.length === 0) return true;
  const text = [plugin.name, plugin.summary, plugin.author, ...plugin.tags]
    .join("\n")
    .toLocaleLowerCase();
  return words.every((word) => text.includes(word));
}

/** The plugins that pass the search and the chips. */
export function filterRegistry(
  plugins: readonly RegistryPluginView[],
  filter: RegistryFilter,
): RegistryPluginView[] {
  return plugins.filter(
    (plugin) =>
      matchesQuery(plugin, filter.query) &&
      (filter.kinds.size === 0 || registryKinds(plugin).some((kind) => filter.kinds.has(kind))) &&
      (!filter.installed || plugin.installed !== null) &&
      (!filter.updates || plugin.updateAvailable),
  );
}

/** Plugins with an update first, then the rest by name (ignoring case), then by id. */
export function sortRegistry(plugins: readonly RegistryPluginView[]): RegistryPluginView[] {
  return plugins.toSorted(
    (a, b) =>
      Number(b.updateAvailable) - Number(a.updateAvailable) ||
      a.name.localeCompare(b.name, undefined, { sensitivity: "base" }) ||
      a.id.localeCompare(b.id),
  );
}

function splitVersion(version: string) {
  const withoutBuild = version.replace(/^v/, "").split("+")[0] ?? "";
  const dash = withoutBuild.indexOf("-");
  const core = dash < 0 ? withoutBuild : withoutBuild.slice(0, dash);
  const pre = dash < 0 ? undefined : withoutBuild.slice(dash + 1);
  return { numbers: core.split(".").map((part) => Number.parseInt(part, 10) || 0), pre };
}

/** Compares two semver versions (numbers first, a pre-release is below its release). */
export function compareVersions(a: string, b: string): number {
  const left = splitVersion(a);
  const right = splitVersion(b);
  for (let i = 0; i < Math.max(left.numbers.length, right.numbers.length); i += 1) {
    const diff = (left.numbers[i] ?? 0) - (right.numbers[i] ?? 0);
    if (diff !== 0) return Math.sign(diff);
  }
  if (left.pre === right.pre) return 0;
  if (left.pre === undefined) return 1;
  if (right.pre === undefined) return -1;
  return left.pre.localeCompare(right.pre, undefined, { numeric: true });
}

export type VersionAction = "install" | "update" | "downgrade" | "reinstall";

/** What installing `version` means for the plugin as it is installed now. */
export function versionAction(plugin: RegistryPluginView, version: string): VersionAction {
  const installed = plugin.installed;
  if (installed === null || !installed.fromRegistry) return "install";
  const order = compareVersions(version, installed.version);
  if (order === 0) return "reinstall";
  return order > 0 ? "update" : "downgrade";
}

/** Why a version cannot be installed: withdrawn, or it does not run in this Questkeep. */
export function versionBlock(version: RegistryVersionView): "yanked" | "incompatible" | null {
  if (version.yanked) return "yanked";
  return version.compatible ? null : "incompatible";
}

/** The versions to list, newest first: the installable ones, or all of them. */
export function listedVersions(
  plugin: RegistryPluginView,
  showAll: boolean,
): RegistryVersionView[] {
  const list = showAll ? plugin.versions : plugin.versions.filter((v) => versionBlock(v) === null);
  return list.toReversed();
}

/** Whether the list has versions that "Show all versions" would add. */
export function hasHiddenVersions(plugin: RegistryPluginView): boolean {
  return plugin.versions.some((version) => versionBlock(version) !== null);
}
