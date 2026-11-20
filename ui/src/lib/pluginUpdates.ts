import type { PluginChanges } from "../bindings/PluginChanges";
import { translator } from "../i18n";
import { pluginUpdates as pluginUpdatesMessages } from "../i18n/pluginUpdates";
import type { PluginUpdate } from "../bindings/PluginUpdate";
import type { RegistryPluginPreview } from "../bindings/RegistryPluginPreview";

/** What an update can ask for beyond what the installed version has. */
export type ChangeKind = "http" | "read" | "folders" | "extension" | "store";

/** The things `changes` lists, in display order (empty when the update needs no review). */
export function changeKinds(changes: PluginChanges): ChangeKind[] {
  const kinds: ChangeKind[] = [];
  if (changes.newHttp.length > 0) kinds.push("http");
  if (changes.newRead.length > 0) kinds.push("read");
  if (changes.newFolders.length > 0) kinds.push("folders");
  if (changes.addsExtension) kinds.push("extension");
  if (changes.addsStore) kinds.push("store");
  return kinds;
}

/** The hosts, paths or folder names behind a kind (empty for kinds that have none). */
export function changeValues(changes: PluginChanges, kind: ChangeKind): string[] {
  switch (kind) {
    case "http":
      return changes.newHttp;
    case "read":
      return changes.newRead;
    case "folders":
      return changes.newFolders.map((folder) => folder.label);
    default:
      return [];
  }
}

const tr = translator(pluginUpdatesMessages);

const ASK_LABEL = {
  http: "askHttp",
  read: "askRead",
  folders: "askFolders",
  extension: "askExtension",
  store: "askStore",
} as const satisfies Record<ChangeKind, string>;

/** "web access to a.com, b.com, a folder you choose: Instances": what an update asks for. */
export function askedFor(changes: PluginChanges): string {
  return changeKinds(changes)
    .map((kind) => tr(ASK_LABEL[kind], { values: changeValues(changes, kind).join(", ") }))
    .join(", ");
}

/** "0.1.0 → 0.2.0". */
export const versionChange = (update: Pick<PluginUpdate, "installedVersion" | "latestVersion">) =>
  `${update.installedVersion} → ${update.latestVersion}`;

/** An update whose zip was downloaded and checked. */
export interface Prepared {
  update: PluginUpdate;
  preview: RegistryPluginPreview;
}

/**
 * Splits prepared updates for "Update all": those that ask for nothing new can be installed in one
 * go after one confirmation, the others stay for the user to look at one by one.
 */
export function planBatch(prepared: readonly Prepared[]): {
  ready: Prepared[];
  review: Prepared[];
} {
  return {
    ready: prepared.filter((p) => !p.preview.changes.needsReview),
    review: prepared.filter((p) => p.preview.changes.needsReview),
  };
}

/** Whether one of the prepared updates runs an extension (JavaScript with the app's own rights). */
export const hasExtension = (prepared: readonly Prepared[]): boolean =>
  prepared.some((p) => p.preview.preview.hasExtension);

/** What happened to one plugin in "Update all". */
export interface UpdateOutcome {
  id: string;
  name: string;
  /** "0.1.0 → 0.2.0". */
  versions: string;
  /** The version that was to be installed. */
  version: string;
  status: "updated" | "failed" | "review";
  /** The readable reason, for a failure. */
  message: string | null;
}

/** How many outcomes have each status. */
export function countOutcomes(
  outcomes: readonly UpdateOutcome[],
): Record<UpdateOutcome["status"], number> {
  const counts = { updated: 0, failed: 0, review: 0 };
  for (const outcome of outcomes) counts[outcome.status] += 1;
  return counts;
}

/** The updates sorted by name, for a stable list. */
export function sortUpdates(updates: readonly PluginUpdate[]): PluginUpdate[] {
  return updates.toSorted(
    (a, b) =>
      a.name.localeCompare(b.name, undefined, { sensitivity: "base" }) || a.id.localeCompare(b.id),
  );
}
