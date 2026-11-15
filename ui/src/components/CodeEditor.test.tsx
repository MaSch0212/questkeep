import { render, screen, waitFor } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import { SCRIPT_VARIABLES } from "../test-utils";
import { CodeEditor, type CodeEditorHandle } from "./CodeEditor";
import { completionsFor, languageOf } from "../lib/codeMirror";

describe("CodeEditor", () => {
  it("stands in with a text area until CodeMirror has loaded, then edits the same text", async () => {
    const onChange = vi.fn<(text: string) => void>();
    let handle: CodeEditorHandle | undefined;
    const { container } = render(() => (
      <CodeEditor
        value="echo hi"
        onChange={onChange}
        runtime="bash"
        variables={SCRIPT_VARIABLES}
        label="Script"
        ref={(h) => (handle = h)}
      />
    ));
    // The text area is there at once.
    expect(screen.getByRole("textbox", { name: "Script" })).toHaveValue("echo hi");
    // CodeMirror replaces it.
    await waitFor(() => expect(container.querySelector(".cm-editor")).not.toBeNull());
    expect(container.querySelector(".cm-content")?.textContent).toBe("echo hi");
    expect(container.querySelector("textarea")).toBeNull();

    handle?.insert("$QK_PHASE");
    await waitFor(() => expect(onChange).toHaveBeenCalledWith("$QK_PHASEecho hi"));
  });

  it("follows the text it is given", async () => {
    const { container } = render(() => (
      <CodeEditor
        value="one"
        onChange={() => undefined}
        runtime="python"
        variables={[]}
        label="Script"
      />
    ));
    await waitFor(() => expect(container.querySelector(".cm-editor")).not.toBeNull());
    expect(container.querySelector(".cm-content")?.textContent).toBe("one");
  });
});

const labels = (runtime: Parameters<typeof completionsFor>[0]) =>
  completionsFor(runtime, SCRIPT_VARIABLES).map((c) => c.label);

describe("script editor language", () => {
  it("offers the variables in each runtime's own syntax and the runtime's keywords", () => {
    expect(labels("windowsPowershell")).toContain("$env:QK_GAME_NAME");
    expect(labels("windowsPowershell")).toContain("Write-Output");
    expect(labels("bash")).toContain("$QK_GENRES");
    expect(labels("bash")).toContain("fi");
    expect(labels("cmd")).toContain("%QK_INSTALL_DIR%");
    expect(labels("cmd")).toContain("goto");
    expect(labels("python")).toContain('os.environ["QK_PHASE"]');
    expect(labels("python")).toContain("import");
  });

  it("has a language for every runtime", () => {
    for (const runtime of ["pwsh", "windowsPowershell", "bash", "cmd", "python", null] as const) {
      expect(languageOf(runtime)).toBeDefined();
    }
  });
});
