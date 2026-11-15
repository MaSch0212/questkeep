import type { Game } from "../bindings/Game";
import type { GameQuery } from "../bindings/GameQuery";
import type { ListField } from "../bindings/ListField";
import type { RuntimeId } from "../bindings/RuntimeId";
import type { RuntimeInfo } from "../bindings/RuntimeInfo";
import type { Script } from "../bindings/Script";
import type { ScriptInput } from "../bindings/ScriptInput";
import type { ScriptKind } from "../bindings/ScriptKind";
import type { ScriptSlot } from "../bindings/ScriptSlot";
import type { ScriptVariable } from "../bindings/ScriptVariable";
import type { VariableGroup } from "../bindings/VariableGroup";
import { translator } from "../i18n";
import { libraryFilters } from "../i18n/libraryFilters";
import { libraryFilterPills } from "../i18n/libraryFilterPills";
import { scriptsText } from "../i18n/scripts";
import { joinerWord, treePills } from "./filters";

const tr = translator(scriptsText);
const trPills = translator(libraryFilterPills);
const trFilters = translator(libraryFilters);

/** The names of the runtimes when the PC did not report one (or the list is not loaded yet). */
export const RUNTIME_NAMES: Record<RuntimeId, string> = {
  pwsh: "PowerShell 7",
  windowsPowershell: "Windows PowerShell 5.1",
  bash: "Bash",
  cmd: "Command Prompt",
  python: "Python",
};

export const runtimeName = (id: RuntimeId, runtimes: readonly RuntimeInfo[]): string =>
  runtimes.find((r) => r.id === id)?.name ?? RUNTIME_NAMES[id];

/** The order of the groups in the variables panel. */
export const VARIABLE_GROUPS: readonly VariableGroup[] = [
  "context",
  "game",
  "lists",
  "images",
  "launch",
  "session",
];

export type VariableSyntax = "env" | "placeholder";

/**
 * What a click on a variable inserts. In arguments and the working folder only `{Placeholder}`s
 * are expanded, so those always get that; the code editor gets the runtime's own way to read an
 * environment variable (or a placeholder when the panel is switched to that syntax).
 */
export function insertionText(
  variable: ScriptVariable,
  syntax: VariableSyntax,
  runtime: RuntimeId | null,
  target: "editor" | "field",
): string {
  if (syntax === "placeholder" || target === "field") return `{${variable.placeholder}}`;
  switch (runtime) {
    case "bash":
      return `$${variable.env}`;
    case "cmd":
      return `%${variable.env}%`;
    case "python":
      return `os.environ["${variable.env}"]`;
    default:
      return `$env:${variable.env}`;
  }
}

/** The variables as the panel lists them: filtered by name or value, in group order. */
export function groupVariables(
  variables: readonly ScriptVariable[],
  filter: string,
): { group: VariableGroup; items: ScriptVariable[] }[] {
  const needle = filter.trim().toLowerCase();
  const shown = variables.filter(
    (v) =>
      needle === "" ||
      v.env.toLowerCase().includes(needle) ||
      v.placeholder.toLowerCase().includes(needle) ||
      v.value.toLowerCase().includes(needle),
  );
  return VARIABLE_GROUPS.flatMap((group) => {
    const items = shown.filter((v) => v.group === group);
    return items.length > 0 ? [{ group, items }] : [];
  });
}

/**
 * Replaces `{Name}` placeholders (case-insensitive) with the variables' values; unknown names
 * stay as typed.
 */
export function expandPlaceholders(text: string, variables: readonly ScriptVariable[]): string {
  const values = new Map(variables.map((v) => [v.placeholder.toLowerCase(), v.value]));
  return text.replace(/\{(\w+)\}/g, (all, name: string) => values.get(name.toLowerCase()) ?? all);
}

/** A program path as it would be typed on a command line (quoted when it has spaces). */
function quoted(path: string): string {
  return /\s/.test(path) && !path.startsWith('"') ? `"${path}"` : path;
}

/**
 * The command line and working folder a script would run with for a game's values: the program
 * (or script file) followed by the expanded arguments, and the expanded working folder (the
 * game's install folder when none is set).
 */
export function commandPreview(
  spec: { program: string; args: string; workingDir: string | null },
  variables: readonly ScriptVariable[],
): { command: string; workingDir: string } {
  const program = quoted(spec.program.trim());
  const args = expandPlaceholders(spec.args, variables).trim();
  const folder = spec.workingDir?.trim();
  const installDir = variables.find((v) => v.placeholder.toLowerCase() === "installdir")?.value;
  return {
    command: args === "" ? program : `${program} ${args}`,
    workingDir: folder ? expandPlaceholders(folder, variables) : (installDir ?? ""),
  };
}

/**
 * The runtime to run a script file with, from its extension (installed ones only). A file with
 * another extension gets the first installed runtime, or none with `fallback` off.
 */
export function runtimeForPath(
  path: string,
  runtimes: readonly RuntimeInfo[],
  fallback = true,
): RuntimeId | null {
  const extension = /\.([A-Za-z0-9]+)$/.exec(path.trim())?.[1]?.toLowerCase() ?? "";
  const preferred: RuntimeId[] =
    extension === "ps1"
      ? ["windowsPowershell", "pwsh"]
      : extension === "sh"
        ? ["bash"]
        : extension === "cmd" || extension === "bat"
          ? ["cmd"]
          : extension === "py"
            ? ["python"]
            : [];
  const installed = new Set(runtimes.map((r) => r.id));
  // A known extension keeps its runtime even when it isn't installed: running says so then.
  return (
    preferred.find((id) => installed.has(id)) ??
    preferred[0] ??
    (fallback ? (runtimes[0]?.id ?? null) : null)
  );
}

/** The runtime a draft runs with: its own choice when installed, else the first installed one. */
export function effectiveRuntime(
  input: Pick<ScriptInput, "runtime" | "source" | "program">,
  runtimes: readonly RuntimeInfo[],
): RuntimeId | null {
  if (input.source === "program") return null;
  if (input.runtime !== null && runtimes.some((r) => r.id === input.runtime)) return input.runtime;
  // Without a runtime for its extension the file runs by itself, as the backend does.
  if (input.source === "linked") return runtimeForPath(input.program, runtimes, false);
  return runtimes[0]?.id ?? null;
}

// ---- ordering -----------------------------------------------------------------------------

/** The list with the item at `from` moved to the place of the item at `to`. */
export function moveItem<T>(list: readonly T[], from: number, to: number): T[] {
  if (from === to || from < 0 || to < 0 || from >= list.length || to >= list.length) {
    return [...list];
  }
  const next = [...list];
  const [item] = next.splice(from, 1);
  next.splice(to, 0, item as T);
  return next;
}

/** The list with the item at `index` moved one place up (`-1`) or down (`1`). */
export const moveBy = <T>(list: readonly T[], index: number, by: -1 | 1): T[] =>
  moveItem(list, index, index + by);

/** An identity for a slot of the pre/post order. */
export const slotKey = (slot: ScriptSlot): string =>
  slot.type === "gameScripts" ? "gameScripts" : slot.id;

// ---- summaries ----------------------------------------------------------------------------

/** The filters of a query as short text ("Installed, Genre is Racing +1"), or null when none. */
export function filterSummary(
  query: GameQuery,
  name: (field: ListField, key: string) => string,
  sourceName: (id: string) => string = (id) => id,
  categoryName: (id: string) => string = (id) => id,
): string | null {
  const parts: string[] = [];
  if (query.sources.length > 0) {
    parts.push(`${trFilters("source")}: ${query.sources.map(sourceName).join(", ")}`);
  }
  if (query.categories.length > 0) {
    parts.push(`${trFilters("category")}: ${query.categories.map(categoryName).join(", ")}`);
  }
  if (query.kind !== "games") {
    parts.push(query.kind === "apps" ? trPills("apps") : trPills("gamesAndApps"));
  }
  if (query.installed !== "any") {
    parts.push(query.installed === "yes" ? trPills("installed") : trPills("notInstalled"));
  }
  if (query.favorite !== "any") {
    parts.push(query.favorite === "yes" ? trPills("favorite") : trPills("notFavorite"));
  }
  if (query.played !== "any") {
    parts.push(query.played === "played" ? trPills("played") : trPills("neverPlayed"));
  }
  if (query.hidden !== "visible") {
    parts.push(query.hidden === "all" ? trPills("hiddenIncluded") : trPills("onlyHidden"));
  }
  for (const pill of treePills(query.filter, name)) {
    const joiner = ` ${joinerWord(pill.joiner)} `;
    parts.push(
      pill.parts
        .map((p) => (p.value === "" ? p.label : `${p.label} ${p.value}`).trim())
        .join(joiner),
    );
  }
  if (parts.length === 0) return null;
  return parts.length <= 2
    ? parts.join(", ")
    : `${parts.slice(0, 2).join(", ")} +${parts.length - 2}`;
}

/** "All games" or "Filter: …" for a global or filtered script. */
export function scopeText(script: Pick<Script, "filter" | "gameId">, summary: string | null) {
  if (script.gameId !== null) return tr("scopeGame");
  if (script.filter === null) return tr("scopeAll");
  return tr("scopeFilter", { summary: summary ?? "…" });
}

/** The text after the scope on a row: runtime or program, what it runs, and when. */
export function runsText(
  script: Pick<Script, "kind" | "timing" | "source" | "program" | "runtime" | "args">,
  runtimes: readonly RuntimeInfo[],
): string {
  const parts: string[] = [];
  const runtime = effectiveRuntime(script, runtimes) ?? script.runtime;
  if (script.source === "program") {
    parts.push(`${script.program}${script.args.trim() === "" ? "" : ` ${script.args.trim()}`}`);
  } else {
    if (runtime !== null) parts.push(runtimeName(runtime, runtimes));
    parts.push(
      script.source === "inline" ? tr("inlineScript") : tr("linkedFile", { path: script.program }),
    );
  }
  if (script.kind === "pre") {
    parts.push(script.timing === "onceRunning" ? tr("timingRunning") : tr("timingBefore"));
  }
  return parts.join(" · ");
}

/** The kind as a short word ("Pre", "Post", "Action"). */
export const kindWord = (kind: ScriptKind): string =>
  kind === "pre" ? tr("kindPre") : kind === "post" ? tr("kindPost") : tr("kindAction");

/** The one-line summary of a script in the Edit dialog's tab: kind first, then what it runs. */
export function ownSummary(script: Script, runtimes: readonly RuntimeInfo[]): string {
  return [kindWord(script.kind), runsText(script, runtimes)].join(" · ");
}

// ---- actions on the game page ---------------------------------------------------------------

/** The enabled action scripts of a game in menu order. */
export function actionScripts(applied: readonly Script[]): Script[] {
  return applied.filter((s) => s.kind === "action" && s.enabled);
}

/** The action buttons of the game page: the first ones as buttons, the rest under "More". */
export function splitActions(
  actions: readonly Script[],
  buttons = 2,
): { shown: Script[]; more: Script[] } {
  return { shown: actions.slice(0, buttons), more: actions.slice(buttons) };
}

/** The group a game's Edit dialog lists its scripts per kind in: pre, post and action order. */
export const KIND_ORDER: readonly ScriptKind[] = ["pre", "post", "action"];

/** The game the variables panel shows values for: the edited game, else the one played last. */
export function sampleGame(games: readonly Game[], gameId: string | null): Game | undefined {
  if (gameId !== null) return games.find((g) => g.id === gameId);
  const played = games
    .filter((g) => g.lastPlayed !== null)
    .toSorted((a, b) => (b.lastPlayed ?? "").localeCompare(a.lastPlayed ?? ""));
  return played[0] ?? games[0];
}

/** One line of the Edit dialog's "Scripts that apply" list: a script, or the game's own ones. */
export type AppliedRow =
  | { kind: ScriptKind; script: Script; own?: undefined }
  | { kind: ScriptKind; own: Script[]; script?: undefined };

/**
 * The scripts that apply to a game as the list shows them: per kind in run order, with the
 * game's own scripts that run one after the other folded into a single line (they run where
 * Settings puts the placeholder).
 */
export function appliedRows(applied: readonly Script[]): AppliedRow[] {
  const rows: AppliedRow[] = [];
  for (const kind of KIND_ORDER) {
    let own: Script[] | null = null;
    for (const script of applied.filter((s) => s.kind === kind)) {
      if (script.gameId === null) {
        rows.push({ kind, script });
        own = null;
      } else if (own === null) {
        own = [script];
        rows.push({ kind, own });
      } else {
        own.push(script);
      }
    }
  }
  return rows;
}
