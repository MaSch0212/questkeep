import { describe, expect, it } from "vitest";
import {
  applyRunEvent,
  cssColor,
  MAX_OUTPUT_CHARS,
  outputText,
  type ScriptRun,
  startRun,
  succeeded,
  tokenStyle,
} from "./scriptRun";

const game = { id: "g1", name: "Hades" };
const ESC = "\u001b";

function write(run: ScriptRun, text: string, stream: "stdout" | "stderr" = "stdout") {
  return applyRunEvent(run, { type: "scriptOutput", run: run.id, stream, text });
}

describe("script runs", () => {
  it("collects the output of its own run line by line", () => {
    let run = startRun("r1", "Try", game);
    run = write(run, "a\n");
    run = applyRunEvent(run, { type: "scriptOutput", run: "r2", stream: "stdout", text: "x\n" });
    run = write(run, "c\n", "stderr");
    expect(run.output.map((line) => [line.stream, line.text])).toEqual([
      ["stdout", "a\n"],
      ["stderr", "c\n"],
    ]);
    expect(outputText(run)).toBe("a\nc\n");
    expect(run.ended).toBe(false);
  });

  it("ends with an exit code or an error", () => {
    const run = startRun("r1", "Try", game);
    const ok = applyRunEvent(run, { type: "scriptRunEnded", run: "r1", exitCode: 0, error: null });
    expect(succeeded(ok)).toBe(true);
    const failed = applyRunEvent(run, {
      type: "scriptRunEnded",
      run: "r1",
      exitCode: 2,
      error: null,
    });
    expect(failed.ended && !succeeded(failed)).toBe(true);
    const broken = applyRunEvent(run, {
      type: "scriptRunEnded",
      run: "r1",
      exitCode: null,
      error: "could not start",
    });
    expect(succeeded(broken)).toBe(false);
    expect(broken.error).toBe("could not start");
    // Late output after the end is ignored.
    expect(write(ok, "x").output).toEqual([]);
  });

  it("drops the oldest lines beyond the limit", () => {
    let run = startRun("r1", "Try", game);
    const half = "x".repeat(MAX_OUTPUT_CHARS / 2);
    run = write(run, half, "stderr");
    run = write(run, half);
    run = write(run, "end");
    expect(run.output.map((line) => line.text.length)).toEqual([half.length, 3]);
    expect(run.dropped).toBe(half.length);
  });

  it("reads colors and keeps them across lines of one output", () => {
    let run = startRun("r1", "Try", game);
    run = write(run, `${ESC}[32;1mcategories : ${ESC}[0m{Casual}\r\n`);
    run = write(run, `${ESC}[31mred starts\n`);
    run = write(run, "plain error\n", "stderr");
    run = write(run, "still red\n");
    const [first, second, error, third] = run.output;
    expect(first?.tokens.map((t) => t.value)).toEqual(["categories : ", "{Casual}\r\n"]);
    expect(first?.tokens[0]?.foreground).toEqual({ type: "named", name: "green" });
    expect(first?.tokens[0]?.decorations.has("bold")).toBe(true);
    expect(first?.tokens[1]?.foreground).toBeNull();
    expect(second?.tokens[0]?.foreground).toEqual({ type: "named", name: "red" });
    expect(error?.tokens[0]?.foreground).toBeNull();
    expect(third?.tokens[0]?.foreground).toEqual({ type: "named", name: "red" });
    // Copying gives the text without the codes.
    expect(outputText(run)).toBe("categories : {Casual}\r\nred starts\nplain error\nstill red\n");
  });

  it("drops escape sequences that aren't colors", () => {
    let run = startRun("r1", "Try", game);
    run = write(run, `${ESC}]0;Window title${ESC}\\${ESC}[2K${ESC}[?25lProgress${ESC}[1A done\n`);
    expect(outputText(run)).toBe("Progress done\n");
  });

  it("shows colors through the theme or as asked", () => {
    expect(cssColor({ type: "named", name: "brightBlue" })).toBe(
      "var(--qk-color-ansi-bright-blue)",
    );
    expect(cssColor({ type: "table", index: 1 })).toBe("var(--qk-color-ansi-red)");
    expect(cssColor({ type: "table", index: 196 })).toBe("rgb(255 0 0)");
    expect(cssColor({ type: "table", index: 232 })).toBe("rgb(8 8 8)");
    expect(cssColor({ type: "rgb", rgb: [1, 2, 3] })).toBe("rgb(1 2 3)");
    const reversed = tokenStyle({
      value: "x",
      foreground: { type: "named", name: "red" },
      background: null,
      decorations: new Set(["reverse", "underline", "strikethrough"]),
    });
    expect(reversed).toEqual({
      color: "var(--qk-color-surface)",
      "background-color": "var(--qk-color-ansi-red)",
      "text-decoration": "underline line-through",
    });
  });
});
