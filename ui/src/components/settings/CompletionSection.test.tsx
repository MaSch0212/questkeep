import { fireEvent, render, screen } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CompletionSettings } from "../../bindings/CompletionSettings";
import * as api from "../../lib/api";
import { store } from "../../lib/store";
import { CompletionSection } from "./CompletionSection";

vi.mock("../../lib/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => p,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));

const settings: CompletionSettings = {
  statuses: [
    { id: "s1", name: "Not played", gameCount: 4 },
    { id: "s2", name: "Playing", gameCount: 1 },
    { id: "s3", name: "Beaten", gameCount: 0 },
  ],
  newGame: "s1",
  firstStart: "s2",
};

const names = () =>
  [...document.querySelectorAll("[data-status] .category-list__name")].map((el) => el.textContent);

beforeEach(async () => {
  vi.clearAllMocks();
  store.reset();
  vi.mocked(api.completionSettings).mockResolvedValue(settings);
  // Saving answers with what was sent, so the list shows it.
  vi.mocked(api.setCompletionSettings).mockImplementation((next) => Promise.resolve(next));
  await store.loadCompletion();
});

describe("CompletionSection", () => {
  it("shows names as text and renames through the edit button", async () => {
    render(() => <CompletionSection />);
    expect(names()).toEqual(["Not played", "Playing", "Beaten"]);
    expect(screen.queryByRole("textbox", { name: "Name of Playing" })).toBeNull();
    await fireEvent.click(screen.getByRole("button", { name: "Rename Playing" }));
    const input = screen.getByRole("textbox", { name: "Name of Playing" });
    await fireEvent.input(input, { target: { value: "Now playing" } });
    await fireEvent.keyDown(input, { key: "Enter" });
    expect(api.setCompletionSettings).toHaveBeenCalledWith(
      {
        ...settings,
        statuses: settings.statuses.with(1, { id: "s2", name: "Now playing", gameCount: 1 }),
      },
      {},
    );
    await vi.waitFor(() => expect(names()).toEqual(["Not played", "Now playing", "Beaten"]));
  });

  it("cancels renaming with Escape", async () => {
    render(() => <CompletionSection />);
    await fireEvent.click(screen.getByRole("button", { name: "Rename Beaten" }));
    const input = screen.getByRole("textbox", { name: "Name of Beaten" });
    await fireEvent.input(input, { target: { value: "Done" } });
    await fireEvent.keyDown(input, { key: "Escape" });
    expect(screen.queryByRole("textbox", { name: "Name of Beaten" })).toBeNull();
    expect(api.setCompletionSettings).not.toHaveBeenCalled();
  });

  it("moves a status with Alt+Up and has no arrow buttons", async () => {
    render(() => <CompletionSection />);
    expect(screen.queryByRole("button", { name: /up$/ })).toBeNull();
    await fireEvent.keyDown(screen.getByRole("button", { name: "Rename Beaten" }), {
      key: "ArrowUp",
      altKey: true,
    });
    const saved = vi.mocked(api.setCompletionSettings).mock.calls[0]?.[0];
    expect(saved?.statuses.map((s) => s.id)).toEqual(["s1", "s3", "s2"]);
    await vi.waitFor(() => expect(names()).toEqual(["Not played", "Beaten", "Playing"]));
  });
});
