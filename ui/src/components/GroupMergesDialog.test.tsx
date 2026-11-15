import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { GroupMerge } from "../bindings/GroupMerge";
import * as api from "../lib/api";
import { store } from "../lib/store";
import { GroupMergesSection } from "./settings/GroupMergesSection";
import { GroupMergesDialog } from "./GroupMergesDialog";

vi.mock("../lib/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => p,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));

const shooters: GroupMerge = {
  id: "m1",
  name: "Shooters",
  members: [
    { key: "fps", label: "FPS" },
    { key: "tps", label: "Third-person shooter" },
  ],
};

async function withMerges(merges: Partial<Record<string, GroupMerge[]>>) {
  vi.mocked(api.setGroupMerges).mockResolvedValueOnce(merges);
  await store.saveGroupMerges("genre", merges.genre ?? []);
}

beforeEach(() => {
  vi.clearAllMocks();
  store.reset();
});

describe("GroupMergesDialog", () => {
  it("lists the field's combined groups and opens one for editing", async () => {
    await withMerges({ genre: [shooters] });
    render(() => <GroupMergesDialog field="genre" onClose={vi.fn<() => void>()} />);
    expect(screen.getByText("Shooters")).toBeInTheDocument();
    expect(screen.getByText("FPS, Third-person shooter")).toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: "Edit Shooters" }));
    expect(store.mergeDialog()).toEqual({ field: "genre", keys: [], mergeId: "m1" });
  });

  it("splits a combined group up", async () => {
    await withMerges({ genre: [shooters] });
    vi.mocked(api.setGroupMerges).mockResolvedValueOnce({ genre: [] });
    render(() => <GroupMergesDialog field="genre" onClose={vi.fn<() => void>()} />);
    await fireEvent.click(screen.getByRole("button", { name: "Split up Shooters" }));
    expect(api.setGroupMerges).toHaveBeenLastCalledWith("genre", []);
    await waitFor(() => expect(screen.getByText("No combined groups yet.")).toBeInTheDocument());
  });

  it("starts a new combination for its field", async () => {
    render(() => <GroupMergesDialog field="series" onClose={vi.fn<() => void>()} />);
    await fireEvent.click(screen.getByRole("button", { name: "Combine groups…" }));
    expect(store.mergeDialog()).toEqual({ field: "series", keys: [], mergeId: null });
  });
});

describe("GroupMergesSection", () => {
  it("shows the combined groups of every field", async () => {
    await withMerges({ genre: [shooters], series: [] });
    render(() => <GroupMergesSection />);
    expect(screen.getByRole("heading", { name: "Genre" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Series" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Edit Shooters" })).toBeInTheDocument();
  });

  it("combines groups of a chosen grouping from the whole library", async () => {
    render(() => <GroupMergesSection />);
    await fireEvent.change(screen.getByRole("combobox", { name: "Grouping" }), {
      target: { value: "series" },
    });
    await fireEvent.click(screen.getByRole("button", { name: "Combine groups…" }));
    expect(store.mergeDialog()).toEqual({
      field: "series",
      keys: [],
      mergeId: null,
      wholeLibrary: true,
    });
  });
});
