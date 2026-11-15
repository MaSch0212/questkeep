import {
  type Color,
  createAnsiSequenceParser,
  namedColors,
  type ParseToken,
} from "ansi-sequence-parser";
import type { CoreEvent } from "../bindings/CoreEvent";
import type { OutputStream } from "../bindings/OutputStream";

/** Output beyond this many characters drops its oldest lines. */
export const MAX_OUTPUT_CHARS = 200_000;

type AnsiParser = ReturnType<typeof createAnsiSequenceParser>;

/** One piece of output as the script wrote it: its text split where its colors change. */
export interface OutputLine {
  stream: OutputStream;
  tokens: ParseToken[];
  /** The text without escape codes. */
  text: string;
}

/** A run of a script from the script dialog ("Run now"), as the output popover shows it. */
export interface ScriptRun {
  id: string;
  /** The script's name when it started. */
  name: string;
  game: { id: string; name: string };
  output: OutputLine[];
  /** Characters dropped from the start because the output grew too long. */
  dropped: number;
  ended: boolean;
  exitCode: number | null;
  error: string | null;
  /** Colors set on one line hold on the next one of the same output. */
  parsers: Record<OutputStream, AnsiParser>;
}

export function startRun(id: string, name: string, game: { id: string; name: string }): ScriptRun {
  return {
    id,
    name,
    game,
    output: [],
    dropped: 0,
    ended: false,
    exitCode: null,
    error: null,
    parsers: { stdout: createAnsiSequenceParser(), stderr: createAnsiSequenceParser() },
  };
}

/**
 * Escape sequences other than colors and styles (`ESC[…m`): cursor moves, clearing, window
 * titles. The parser would read them as colors up to the next "m", so they go first.
 */
const OTHER_ESCAPES =
  // oxlint-disable-next-line no-control-regex
  /\u001b\[[0-?]*[ -/]*[@-ln-~]|\u001b\][^\u0007\u001b]*(?:\u0007|\u001b\\)?|\u001b[@-Z\\-_]/g;

/** The text of a line without escape codes, split where its colors change. */
export function parseLine(parser: AnsiParser, text: string): ParseToken[] {
  return parser.parse(text.replace(OTHER_ESCAPES, "")).filter((token) => token.value !== "");
}

/** The run after one core event (unchanged for events of other runs). */
export function applyRunEvent(run: ScriptRun, event: CoreEvent): ScriptRun {
  if (event.type === "scriptRunEnded" && event.run === run.id) {
    return { ...run, ended: true, exitCode: event.exitCode, error: event.error };
  }
  if (event.type !== "scriptOutput" || event.run !== run.id || run.ended) return run;
  const tokens = parseLine(run.parsers[event.stream], event.text);
  const line = { stream: event.stream, tokens, text: tokens.map((t) => t.value).join("") };
  const output = [...run.output, line];
  let dropped = run.dropped;
  let total = output.reduce((sum, piece) => sum + piece.text.length, 0);
  // Whole lines go, the newest always stays.
  while (total > MAX_OUTPUT_CHARS && output.length > 1) {
    const first = output.shift();
    if (!first) break;
    dropped += first.text.length;
    total -= first.text.length;
  }
  return { ...run, output, dropped };
}

/** All output as plain text, without escape codes (for copying). */
export function outputText(run: ScriptRun): string {
  return run.output.map((line) => line.text).join("");
}

/** Whether a run ended well: exit code 0 and nothing went wrong. */
export function succeeded(run: ScriptRun): boolean {
  return run.ended && run.error === null && run.exitCode === 0;
}

const CUBE = [0, 95, 135, 175, 215, 255];

/**
 * How a color shows: the 16 named colors (also table entries 0–15) as the theme's
 * `--qk-color-ansi-*` variables, other table entries and RGB colors as the script asked for them.
 */
export function cssColor(color: Color): string {
  if (color.type === "named") return `var(--qk-color-ansi-${kebab(color.name)})`;
  if (color.type === "rgb") return `rgb(${color.rgb.join(" ")})`;
  const index = color.index;
  const named = namedColors[index];
  if (named !== undefined) return `var(--qk-color-ansi-${kebab(named)})`;
  if (index >= 232 && index <= 255) {
    const gray = 8 + (index - 232) * 10;
    return `rgb(${gray} ${gray} ${gray})`;
  }
  const cube = Math.min(Math.max(index - 16, 0), 215);
  const r = CUBE[Math.floor(cube / 36)] ?? 0;
  const g = CUBE[Math.floor(cube / 6) % 6] ?? 0;
  const b = CUBE[cube % 6] ?? 0;
  return `rgb(${r} ${g} ${b})`;
}

function kebab(name: string): string {
  return name.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`);
}

/** The inline style of a token: its colors and decorations (reverse swaps the colors). */
export function tokenStyle(token: ParseToken): Record<string, string> {
  const style: Record<string, string> = {};
  const reverse = token.decorations.has("reverse");
  const fg = reverse ? token.background : token.foreground;
  const bg = reverse ? token.foreground : token.background;
  if (fg) style.color = cssColor(fg);
  else if (reverse) style.color = "var(--qk-color-surface)";
  if (bg) style["background-color"] = cssColor(bg);
  else if (reverse) style["background-color"] = "var(--qk-color-text)";
  if (token.decorations.has("bold")) style["font-weight"] = "bold";
  if (token.decorations.has("dim")) style.opacity = "var(--qk-ansi-dim-opacity)";
  if (token.decorations.has("italic")) style["font-style"] = "italic";
  if (token.decorations.has("hidden")) style.visibility = "hidden";
  const lines = [
    token.decorations.has("underline") ? "underline" : "",
    token.decorations.has("strikethrough") ? "line-through" : "",
    token.decorations.has("overline") ? "overline" : "",
  ].filter((l) => l !== "");
  if (lines.length > 0) style["text-decoration"] = lines.join(" ");
  return style;
}

type Listener = (event: CoreEvent) => void;
const listeners = new Set<Listener>();

/** Hears the output and ends of script runs. Returns the function that stops listening. */
export function onScriptRunEvent(listener: Listener): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** Hands a core event to the run listeners (called by the store for every core event). */
export function emitScriptRunEvent(event: CoreEvent) {
  if (event.type !== "scriptOutput" && event.type !== "scriptRunEnded") return;
  for (const listener of listeners) listener(event);
}
