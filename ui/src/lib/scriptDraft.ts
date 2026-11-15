import type { RuntimeInfo } from "../bindings/RuntimeInfo";
import type { Script } from "../bindings/Script";
import type { ScriptInput } from "../bindings/ScriptInput";
import type { ScriptKind } from "../bindings/ScriptKind";
import type { ScriptOptions } from "../bindings/ScriptOptions";
import { translator } from "../i18n";
import { scriptsText } from "../i18n/scripts";
import { effectiveRuntime } from "./scripts";
import { activeFilterCount } from "./views";

const tr = translator(scriptsText);

/** The advanced options a new script of a kind starts with (actions don't wait). */
export function defaultOptions(kind: ScriptKind): ScriptOptions {
  return {
    showWindow: false,
    wait: kind !== "action",
    timeoutSeconds: 60,
    stopLaunchOnFailure: false,
  };
}

/** A new, empty script (a game's own script when `gameId` is given). */
export function blankScript(kind: ScriptKind, gameId: string | null = null): ScriptInput {
  return {
    id: null,
    name: "",
    kind,
    enabled: true,
    gameId,
    filter: null,
    timing: "beforeStart",
    source: "program",
    program: "",
    runtime: null,
    args: "",
    workingDir: null,
    options: defaultOptions(kind),
    icon: kind === "action" ? "zap" : null,
    content: null,
  };
}

/** A saved script as the dialog edits it. */
export function inputOf(script: Script): ScriptInput {
  return {
    id: script.id,
    name: script.name,
    kind: script.kind,
    enabled: script.enabled,
    gameId: script.gameId,
    filter: script.filter,
    timing: script.timing,
    source: script.source,
    program: script.program,
    runtime: script.runtime,
    args: script.args,
    workingDir: script.workingDir,
    options: { ...script.options },
    icon: script.icon,
    content: script.content,
  };
}

/** A copy of a script to save as a new one. */
export function duplicateOf(script: Script, name: string): ScriptInput {
  return { ...inputOf(script), id: null, name };
}

/**
 * The input as it is sent: only the fields of the chosen source and kind (a leftover icon on a
 * pre script or text on a program script would be saved otherwise).
 */
export function cleanInput(input: ScriptInput, runtimes: readonly RuntimeInfo[]): ScriptInput {
  const program = input.source === "inline" ? "" : input.program.trim();
  return {
    ...input,
    name: input.name.trim(),
    program,
    runtime: input.source === "program" ? null : effectiveRuntime(input, runtimes),
    content: input.source === "inline" ? (input.content ?? "") : null,
    timing: input.kind === "pre" ? input.timing : "beforeStart",
    icon: input.kind === "action" ? input.icon : null,
    workingDir: input.workingDir?.trim() ? input.workingDir.trim() : null,
    filter: input.gameId === null ? input.filter : null,
    options: {
      ...input.options,
      stopLaunchOnFailure: input.kind === "pre" && input.options.stopLaunchOnFailure,
      timeoutSeconds: Math.max(0, Math.floor(input.options.timeoutSeconds) || 0),
    },
  };
}

/** What stops the script from being saved, as a message; null when it can be saved. */
export function scriptIssue(input: ScriptInput, runtimes: readonly RuntimeInfo[]): string | null {
  if (input.name.trim() === "") return tr("needName");
  if (input.source === "program" && input.program.trim() === "") return tr("needProgram");
  if (input.source === "linked" && input.program.trim() === "") return tr("needFile");
  if (input.source !== "program" && effectiveRuntime(input, runtimes) === null) {
    return tr("needRuntime");
  }
  if (input.gameId === null && input.filter !== null && activeFilterCount(input.filter) === 0) {
    return tr("needFilter");
  }
  return null;
}

/** Whether two drafts differ (what the dialog asks about before discarding). */
export function sameInput(a: ScriptInput, b: ScriptInput): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}
