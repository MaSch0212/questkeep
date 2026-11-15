import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../lib/api";
import { store } from "../lib/store";
import { makeScript, RUNTIMES } from "../test-utils";
import { ScriptEditorHost } from "./ScriptEditorHost";

vi.mock("../lib/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => p,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn<() => void>() }));

beforeEach(() => {
  vi.clearAllMocks();
  store.reset();
  vi.mocked(api.scriptRuntimes).mockResolvedValue(RUNTIMES);
  vi.mocked(api.listGames).mockResolvedValue([]);
});

describe("the bell's Edit script", () => {
  it("opens the dialog of that script, and Cancel closes it", async () => {
    vi.mocked(api.getScript).mockResolvedValue(
      makeScript({ id: "s1", name: "Broken script", source: "program", program: "x.exe" }),
    );
    store.openScriptEditor("s1");
    expect(store.scriptEditorId()).toBe("s1");
    render(() => <ScriptEditorHost id="s1" />);
    const dialog = await screen.findByRole("dialog", { name: "Edit script" });
    expect(dialog).toBeInTheDocument();
    expect(screen.getByRole("textbox", { name: "Name" })).toHaveValue("Broken script");
    expect(api.getScript).toHaveBeenCalledWith("s1");
    await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(store.scriptEditorId()).toBeNull();
  });

  it("closes with an error when the script cannot be loaded", async () => {
    vi.mocked(api.getScript).mockRejectedValue("script not found");
    store.openScriptEditor("gone");
    render(() => <ScriptEditorHost id="gone" />);
    await waitFor(() => expect(store.scriptEditorId()).toBeNull());
    expect(store.toasts().some((t) => t.message === "script not found")).toBe(true);
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});
