import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { GameQuery } from "../bindings/GameQuery";
import type { ScriptInput } from "../bindings/ScriptInput";
import * as api from "../lib/api";
import { defaultQuery } from "../lib/query";
import { emitScriptRunEvent } from "../lib/scriptRun";
import { store } from "../lib/store";
import { makeGame, makeScript, RUNTIMES, SCRIPT_VARIABLES } from "../test-utils";
import { ScriptDialog } from "./ScriptDialog";

vi.mock("../lib/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => p,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn<() => void>() }));

// CodeMirror needs a real layout engine: the dialog is tested with a plain text area.
vi.mock("./CodeEditor", async () => {
  const { createEffect } = await import("solid-js");
  return {
    CodeEditor: (props: {
      value: string;
      label: string;
      onChange: (text: string) => void;
      onFocus?: () => void;
      ref?: (handle: { insert: (text: string) => void; focus: () => void }) => void;
    }) => {
      const el = document.createElement("textarea");
      el.setAttribute("aria-label", props.label);
      createEffect(() => {
        el.value = props.value;
      });
      el.addEventListener("input", () => props.onChange(el.value));
      el.addEventListener("focus", () => props.onFocus?.());
      props.ref?.({
        insert: (text) => {
          el.value += text;
          props.onChange(el.value);
        },
        focus: () => el.focus(),
      });
      return el;
    },
  };
});

// The filter editor is its own component (and tested with the category dialog).
vi.mock("./AutoFilterEditor", () => ({
  AutoFilterEditor: (props: { query: GameQuery; onChange: (q: GameQuery) => void }) => {
    const button = document.createElement("button");
    button.textContent = "Only installed";
    button.addEventListener("click", () => props.onChange({ ...props.query, installed: "yes" }));
    return button;
  },
}));

const hades = makeGame({ id: "g1", name: "Hades" });

beforeEach(async () => {
  vi.clearAllMocks();
  store.reset();
  vi.mocked(listen).mockResolvedValue(() => undefined);
  vi.mocked(api.listProviders).mockResolvedValue([]);
  vi.mocked(api.runningGames).mockResolvedValue([]);
  vi.mocked(api.listGames).mockResolvedValue([hades]);
  vi.mocked(api.appInfo).mockResolvedValue({
    version: "1",
    dataDir: "",
    logsDir: "",
    portable: false,
    devTools: false,
  });
  await store.init();
  vi.mocked(api.scriptRuntimes).mockResolvedValue(RUNTIMES);
  vi.mocked(api.scriptVariables).mockResolvedValue(SCRIPT_VARIABLES);
  vi.mocked(api.saveScript).mockImplementation((input: ScriptInput) =>
    Promise.resolve(makeScript({ ...input, id: input.id ?? "new", position: 0n })),
  );
  vi.mocked(api.runScriptDraft).mockResolvedValue(undefined);
  vi.mocked(api.stopScriptRun).mockResolvedValue(true);
  vi.mocked(api.deleteScript).mockResolvedValue(undefined);
});

const dialog = (name: string) => screen.getByRole("dialog", { name });
const source = (name: string) => screen.getByRole("radio", { name });
const textbox = (name: string) => screen.getByRole("textbox", { name });

async function ready() {
  // The runtimes and the variables arrive after the first paint.
  await screen.findAllByRole("button", { name: /^Insert / });
}

function saved(): ScriptInput {
  const call = vi.mocked(api.saveScript).mock.calls[0];
  if (!call) throw new Error("not saved");
  return call[0];
}

describe("ScriptDialog", () => {
  it("saves a program script with the expected input", async () => {
    const onClose = vi.fn<() => void>();
    render(() => <ScriptDialog script={null} kind="pre" onClose={onClose} />);
    await ready();
    await fireEvent.input(textbox("Name"), { target: { value: "Open saves" } });
    await fireEvent.input(textbox("Program"), { target: { value: "explorer.exe" } });
    await fireEvent.input(textbox("Arguments"), { target: { value: '"{InstallDir}\\saves"' } });
    await fireEvent.click(screen.getByRole("button", { name: "Add" }));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(saved()).toEqual({
      id: null,
      name: "Open saves",
      kind: "pre",
      enabled: true,
      gameId: null,
      filter: null,
      timing: "beforeStart",
      source: "program",
      program: "explorer.exe",
      runtime: null,
      args: '"{InstallDir}\\saves"',
      workingDir: null,
      options: { showWindow: false, wait: true, timeoutSeconds: 60, stopLaunchOnFailure: false },
      icon: null,
      content: null,
    });
  });

  it("previews the command line and working folder with the chosen game's values", async () => {
    render(() => <ScriptDialog script={null} kind="action" onClose={() => undefined} />);
    await ready();
    await fireEvent.input(textbox("Program"), { target: { value: "C:\\Windows\\explorer.exe" } });
    await fireEvent.input(textbox("Arguments"), {
      target: { value: '"{installdir}\\saves" {Nope}' },
    });
    const preview = screen.getByRole("group", { name: "Preview" });
    expect(preview).toHaveTextContent('C:\\Windows\\explorer.exe "C:\\Games\\Hades\\saves" {Nope}');
    expect(preview).toHaveTextContent("Working folder: C:\\Games\\Hades");
    expect(within(preview).getByText("Preview for Hades")).toBeInTheDocument();
  });

  it("shows the editor, the runtime picker and no preview for a script, and inserts variables in its syntax", async () => {
    render(() => <ScriptDialog script={null} kind="pre" onClose={() => undefined} />);
    await ready();
    await fireEvent.click(source("Script"));
    expect(screen.queryByRole("group", { name: "Preview" })).toBeNull();
    const runtime = screen.getByRole("combobox", { name: "Runtime" });
    await waitFor(() =>
      expect(
        within(runtime)
          .getAllByRole("option")
          .map((o) => o.textContent),
      ).toEqual(["Windows PowerShell 5.1", "Bash", "Python 3.12"]),
    );
    const editor = textbox("Script");
    await fireEvent.focus(editor);
    await fireEvent.click(screen.getByRole("button", { name: "Insert QK_GAME_NAME" }));
    expect(editor).toHaveValue("$env:QK_GAME_NAME");

    await fireEvent.change(runtime, { target: { value: "bash" } });
    await fireEvent.click(screen.getByRole("button", { name: "Insert QK_GENRES" }));
    expect(editor).toHaveValue("$env:QK_GAME_NAME$QK_GENRES");

    await fireEvent.change(runtime, { target: { value: "python" } });
    await fireEvent.click(screen.getByRole("button", { name: "Insert QK_PHASE" }));
    expect(editor).toHaveValue('$env:QK_GAME_NAME$QK_GENRESos.environ["QK_PHASE"]');

    // The { } switch gives placeholders in the editor as well.
    await fireEvent.click(screen.getByRole("radio", { name: "{ }" }));
    await fireEvent.click(screen.getByRole("button", { name: "Insert {Phase}" }));
    expect((editor as HTMLTextAreaElement).value.endsWith("{Phase}")).toBe(true);
  });

  it("inserts placeholders into the field that was focused last", async () => {
    render(() => <ScriptDialog script={null} kind="pre" onClose={() => undefined} />);
    await ready();
    const args = textbox("Arguments");
    await fireEvent.focus(args);
    await fireEvent.click(screen.getByRole("button", { name: "Insert {GameName}" }));
    expect(args).toHaveValue("{GameName}");
    // The folder field in Advanced is a target too.
    const folder = screen.getByLabelText("Working folder");
    await fireEvent.focusIn(folder);
    await fireEvent.click(screen.getByRole("button", { name: "Insert {InstallDir}" }));
    expect(folder).toHaveValue("{InstallDir}");
  });

  it("saves a script with its runtime and text", async () => {
    render(() => <ScriptDialog script={null} kind="post" onClose={() => undefined} />);
    await ready();
    await fireEvent.input(textbox("Name"), { target: { value: "Back up" } });
    await fireEvent.click(source("Script"));
    await waitFor(() =>
      expect(screen.getByRole("combobox", { name: "Runtime" })).toHaveValue("windowsPowershell"),
    );
    await fireEvent.change(screen.getByRole("combobox", { name: "Runtime" }), {
      target: { value: "bash" },
    });
    await fireEvent.input(textbox("Script"), { target: { value: "echo hi" } });
    await fireEvent.click(screen.getByRole("button", { name: "Add" }));
    await waitFor(() => expect(api.saveScript).toHaveBeenCalled());
    expect(saved()).toMatchObject({
      kind: "post",
      source: "inline",
      runtime: "bash",
      content: "echo hi",
      program: "",
      timing: "beforeStart",
    });
  });

  it("picks the runtime of a linked file from its extension and imports a copy", async () => {
    vi.mocked(api.readScriptFile).mockResolvedValue("print('hi')");
    render(() => <ScriptDialog script={null} kind="pre" onClose={() => undefined} />);
    await ready();
    await fireEvent.click(source("Linked script file"));
    await fireEvent.input(textbox("Script file"), { target: { value: "C:\\Tools\\show.py" } });
    await waitFor(() =>
      expect(screen.getByRole("combobox", { name: "Run with" })).toHaveValue("python"),
    );
    await fireEvent.input(textbox("Script file"), { target: { value: "C:\\Tools\\go.sh" } });
    await waitFor(() =>
      expect(screen.getByRole("combobox", { name: "Run with" })).toHaveValue("bash"),
    );
    await fireEvent.click(screen.getByRole("button", { name: "Import a copy" }));
    await waitFor(() => expect(textbox("Script")).toHaveValue("print('hi')"));
    expect(source("Script")).toBeChecked();
  });

  it("imports a file into an empty editor, and asks before replacing text", async () => {
    vi.mocked(open).mockResolvedValue("C:\\Tools\\go.sh");
    vi.mocked(api.readScriptFile).mockResolvedValue("echo imported");
    render(() => <ScriptDialog script={null} kind="pre" onClose={() => undefined} />);
    await ready();
    await fireEvent.click(source("Script"));
    await fireEvent.click(screen.getByRole("button", { name: "Import file…" }));
    await waitFor(() => expect(textbox("Script")).toHaveValue("echo imported"));
    expect(screen.getByRole("combobox", { name: "Runtime" })).toHaveValue("bash");

    vi.mocked(api.readScriptFile).mockResolvedValue("echo second");
    await fireEvent.click(screen.getByRole("button", { name: "Import file…" }));
    const ask = await screen.findByRole("dialog", { name: "Replace the script?" });
    expect(textbox("Script")).toHaveValue("echo imported");
    await fireEvent.click(within(ask).getByRole("button", { name: "Replace" }));
    await waitFor(() => expect(textbox("Script")).toHaveValue("echo second"));
  });

  it("shows When and the stop option for pre scripts only, and the icon for actions only", async () => {
    render(() => <ScriptDialog script={null} kind="pre" onClose={() => undefined} />);
    await ready();
    expect(screen.getByRole("radio", { name: "Once the game is running" })).toBeInTheDocument();
    expect(screen.getByLabelText("Don't start the game if this script fails")).toBeInTheDocument();
    expect(screen.queryByRole("radiogroup", { name: "Icon" })).toBeNull();

    await fireEvent.click(screen.getByRole("radio", { name: "After it closes" }));
    expect(screen.queryByRole("radio", { name: "Once the game is running" })).toBeNull();
    expect(screen.queryByLabelText("Don't start the game if this script fails")).toBeNull();

    await fireEvent.click(screen.getByRole("radio", { name: "As an action" }));
    expect(screen.getByRole("radiogroup", { name: "Icon" })).toBeInTheDocument();
    // Actions don't wait for the program by default.
    expect(screen.getByLabelText("Wait until it finishes")).not.toBeChecked();
  });

  it("saves an action with the chosen icon", async () => {
    render(() => <ScriptDialog script={null} kind="action" onClose={() => undefined} />);
    await ready();
    await fireEvent.input(textbox("Name"), { target: { value: "Open saves" } });
    await fireEvent.input(textbox("Program"), { target: { value: "explorer.exe" } });
    await fireEvent.click(screen.getByRole("radio", { name: "folder open" }));
    await fireEvent.click(screen.getByRole("button", { name: "Add" }));
    await waitFor(() => expect(api.saveScript).toHaveBeenCalled());
    expect(saved()).toMatchObject({ kind: "action", icon: "folder-open", timing: "beforeStart" });
    expect(saved().options.wait).toBe(false);
  });

  it("chooses the games a script applies to with the filter editor", async () => {
    render(() => <ScriptDialog script={null} kind="pre" onClose={() => undefined} />);
    await ready();
    await fireEvent.input(textbox("Name"), { target: { value: "Racing" } });
    await fireEvent.input(textbox("Program"), { target: { value: "a.exe" } });
    // Choosing "Matching games" opens the filter dialog at once.
    await fireEvent.click(screen.getByRole("radio", { name: "Matching games" }));
    const filter = await screen.findByRole("dialog", { name: "Games this script applies to" });
    await fireEvent.click(within(filter).getByRole("button", { name: "Only installed" }));
    await fireEvent.click(within(filter).getByRole("button", { name: "Apply" }));
    await waitFor(() => expect(screen.getByText(/Installed · 1 game matches/)).toBeInTheDocument());
    await fireEvent.click(screen.getByRole("button", { name: "Add" }));
    await waitFor(() => expect(api.saveScript).toHaveBeenCalled());
    expect(saved().filter).toEqual({ ...defaultQuery(), installed: "yes" });
  });

  it("needs a filter for matching games before it saves", async () => {
    render(() => <ScriptDialog script={null} kind="pre" onClose={() => undefined} />);
    await ready();
    await fireEvent.input(textbox("Name"), { target: { value: "X" } });
    await fireEvent.input(textbox("Program"), { target: { value: "a.exe" } });
    await fireEvent.click(screen.getByRole("radio", { name: "Matching games" }));
    const filter = await screen.findByRole("dialog", { name: "Games this script applies to" });
    await fireEvent.click(within(filter).getByRole("button", { name: "Cancel" }));
    await fireEvent.click(screen.getByRole("button", { name: "Add" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Choose at least one filter.");
    expect(api.saveScript).not.toHaveBeenCalled();
  });

  it("hides For for a game's own script and saves it with the game", async () => {
    render(() => <ScriptDialog script={null} kind="pre" gameId="g1" onClose={() => undefined} />);
    await ready();
    expect(screen.queryByRole("radio", { name: "All games" })).toBeNull();
    await fireEvent.input(textbox("Name"), { target: { value: "Mine" } });
    await fireEvent.input(textbox("Program"), { target: { value: "a.exe" } });
    await fireEvent.click(screen.getByRole("button", { name: "Add" }));
    await waitFor(() => expect(api.saveScript).toHaveBeenCalled());
    expect(saved()).toMatchObject({ gameId: "g1", filter: null });
  });

  it("explains what stops a save", async () => {
    render(() => <ScriptDialog script={null} kind="pre" onClose={() => undefined} />);
    await ready();
    await fireEvent.click(screen.getByRole("button", { name: "Add" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Give the script a name.");
    await fireEvent.input(textbox("Name"), { target: { value: "X" } });
    expect(screen.queryByRole("alert")).toBeNull();
    await fireEvent.click(screen.getByRole("button", { name: "Add" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Choose a program.");
    expect(api.saveScript).not.toHaveBeenCalled();
  });

  it("asks before Escape or a click outside discards changes; Cancel doesn't ask", async () => {
    const onClose = vi.fn<() => void>();
    render(() => <ScriptDialog script={null} kind="pre" onClose={onClose} />);
    await ready();
    await fireEvent.keyDown(dialog("New script"), { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);

    await fireEvent.input(textbox("Name"), { target: { value: "Changed" } });
    await fireEvent.keyDown(dialog("New script"), { key: "Escape" });
    const ask = await screen.findByRole("dialog", { name: "Unsaved changes" });
    expect(onClose).toHaveBeenCalledTimes(1);
    await fireEvent.click(within(ask).getByRole("button", { name: "Keep editing" }));
    expect(screen.queryByRole("dialog", { name: "Unsaved changes" })).toBeNull();

    await fireEvent.mouseDown(dialog("New script").parentElement as HTMLElement);
    await screen.findByRole("dialog", { name: "Unsaved changes" });
    await fireEvent.click(screen.getByRole("button", { name: "Discard" }));
    expect(onClose).toHaveBeenCalledTimes(2);

    await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onClose).toHaveBeenCalledTimes(3);
    expect(screen.queryByRole("dialog", { name: "Unsaved changes" })).toBeNull();
  });

  it("saves from the unsaved-changes prompt", async () => {
    const onClose = vi.fn<() => void>();
    render(() => <ScriptDialog script={null} kind="pre" onClose={onClose} />);
    await ready();
    await fireEvent.input(textbox("Name"), { target: { value: "Via prompt" } });
    await fireEvent.input(textbox("Program"), { target: { value: "a.exe" } });
    await fireEvent.keyDown(dialog("New script"), { key: "Escape" });
    await fireEvent.click(await screen.findByRole("button", { name: "Save" }));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(saved().name).toBe("Via prompt");
  });

  it("edits a saved script, keeps its id and deletes after confirming", async () => {
    const script = makeScript({ id: "s9", name: "Existing" });
    const onClose = vi.fn<() => void>();
    render(() => <ScriptDialog script={script} onClose={onClose} />);
    await ready();
    expect(textbox("Name")).toHaveValue("Existing");
    expect(textbox("Script")).toHaveValue("Write-Output hi");
    await fireEvent.input(textbox("Name"), { target: { value: "Renamed" } });
    await fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(api.saveScript).toHaveBeenCalled());
    expect(saved()).toMatchObject({ id: "s9", name: "Renamed", content: "Write-Output hi" });
  });

  it("deletes a saved script after confirming", async () => {
    const onClose = vi.fn<() => void>();
    render(() => <ScriptDialog script={makeScript({ id: "s9" })} onClose={onClose} />);
    await ready();
    await fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    const ask = screen.getByRole("dialog", { name: "Delete script" });
    expect(api.deleteScript).not.toHaveBeenCalled();
    await fireEvent.click(within(ask).getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(api.deleteScript).toHaveBeenCalledWith("s9"));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
  });

  it("runs the draft now for a chosen game without saving", async () => {
    render(() => <ScriptDialog script={null} kind="pre" onClose={() => undefined} />);
    await ready();
    await fireEvent.input(textbox("Name"), { target: { value: "Try" } });
    await fireEvent.input(textbox("Program"), { target: { value: "a.exe" } });
    await fireEvent.click(screen.getByRole("button", { name: "Run now…" }));
    await fireEvent.click(await screen.findByRole("option", { name: /Hades/ }));
    await waitFor(() => expect(api.runScriptDraft).toHaveBeenCalled());
    const [input, gameId, run] = vi.mocked(api.runScriptDraft).mock.calls[0] ?? [];
    expect(gameId).toBe("g1");
    expect(input).toMatchObject({ name: "Try", program: "a.exe", id: null });
    expect(api.saveScript).not.toHaveBeenCalled();

    // The run's output shows in a popover as it arrives, then its exit code.
    const output = await screen.findByRole("dialog", { name: "Output of the run" });
    expect(within(output).getByRole("status").textContent).toContain("Running…");
    const id = run ?? "";
    emitScriptRunEvent({ type: "scriptOutput", run: id, stream: "stdout", text: "hello\n" });
    emitScriptRunEvent({
      type: "scriptOutput",
      run: "other",
      stream: "stdout",
      text: "not mine\n",
    });
    emitScriptRunEvent({ type: "scriptOutput", run: id, stream: "stderr", text: "oops\n" });
    await waitFor(() => expect(output.querySelector("pre")?.textContent).toBe("hello\noops\n"));
    expect(output.querySelector(".run-output__stderr")?.textContent).toBe("oops\n");
    await fireEvent.click(within(output).getByRole("button", { name: "Stop" }));
    expect(api.stopScriptRun).toHaveBeenCalledWith(run);
    emitScriptRunEvent({ type: "scriptRunEnded", run: id, exitCode: 3, error: null });
    await waitFor(() =>
      expect(within(output).getByRole("status").textContent).toContain("Exit code 3"),
    );

    // Run again starts a new run for the same game.
    await fireEvent.click(within(output).getByRole("button", { name: "Run again" }));
    await waitFor(() => expect(api.runScriptDraft).toHaveBeenCalledTimes(2));
    const [, again, second] = vi.mocked(api.runScriptDraft).mock.calls[1] ?? [];
    expect(again).toBe("g1");
    expect(second).not.toBe(run);
  });

  it("copies the command line", async () => {
    const writeText = vi.fn<(text: string) => Promise<void>>().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    render(() => <ScriptDialog script={null} kind="pre" onClose={() => undefined} />);
    await ready();
    await fireEvent.input(textbox("Program"), { target: { value: "a.exe" } });
    await fireEvent.input(textbox("Arguments"), { target: { value: "{GameName}" } });
    await fireEvent.click(screen.getByRole("button", { name: "Copy command line" }));
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("a.exe Hades"));
  });
});
