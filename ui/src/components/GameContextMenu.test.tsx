import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import * as api from "../lib/api";
import { store } from "../lib/store";
import { makeGame } from "../test-utils";
import { GameContextMenu } from "./GameContextMenu";

vi.mock("../lib/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => p,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));

beforeEach(() => {
  vi.clearAllMocks();
  store.reset();
  vi.mocked(api.launchProfiles).mockResolvedValue({ profiles: [], defaultId: null, playId: null });
});

describe("GameContextMenu", () => {
  it("marks a game as an app; apps have no completion status", async () => {
    vi.mocked(api.updateGame).mockResolvedValue(makeGame({ kind: "app" }));
    render(() => (
      <GameContextMenu game={makeGame()} anchor={{ x: 0, y: 0 }} onDone={() => undefined} />
    ));
    expect(screen.getByRole("menuitem", { name: /Completion status/ })).toBeInTheDocument();
    await fireEvent.click(screen.getByRole("menuitem", { name: "Mark as app" }));
    await waitFor(() =>
      expect(api.updateGame).toHaveBeenCalledWith("g1", expect.objectContaining({ kind: "app" })),
    );
  });

  it("offers Mark as game for an app and leaves out the status", () => {
    render(() => (
      <GameContextMenu
        game={makeGame({ kind: "app" })}
        anchor={{ x: 0, y: 0 }}
        onDone={() => undefined}
      />
    ));
    expect(screen.getByRole("menuitem", { name: "Mark as game" })).toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: /Completion status/ })).toBeNull();
  });
});
