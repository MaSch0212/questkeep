import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Category } from "../bindings/Category";
import * as api from "../lib/api";
import { defaultQuery } from "../lib/query";
import { store } from "../lib/store";
import { makeGame } from "../test-utils";
import { CategoryFilterDialog } from "./CategoryFilterDialog";

vi.mock("../lib/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => p,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));

const category: Category = {
  id: "c1",
  name: "Installed",
  gameCount: 1,
  sources: [],
  autoFilter: null,
};

beforeEach(() => {
  vi.clearAllMocks();
  store.reset();
  vi.mocked(api.listCategories).mockResolvedValue([category]);
  vi.mocked(api.listGames).mockResolvedValue([
    makeGame({ id: "a", name: "Hades", categories: ["c1"] }),
    makeGame({ id: "b", name: "Celeste" }),
  ]);
});

describe("CategoryFilterDialog", () => {
  it("previews the matching games and saves the filter", async () => {
    vi.mocked(api.setCategoryFilter).mockResolvedValue({
      ...category,
      autoFilter: defaultQuery(),
    });
    const onClose = vi.fn<() => void>();
    render(() => <CategoryFilterDialog category={category} onClose={onClose} />);
    const dialog = screen.getByRole("dialog", { name: "Auto filter for Installed" });
    const save = within(dialog).getByRole("button", { name: "Save and apply" });
    // No filter yet: it would match everything.
    expect(save).toBeDisabled();
    const installed = within(dialog).getByRole("radiogroup", { name: "Installed" });
    await fireEvent.click(within(installed).getByRole("radio", { name: "Yes" }));
    expect(await within(dialog).findByText("2 games match")).toBeInTheDocument();
    expect(vi.mocked(api.listGames).mock.lastCall?.[0]).toMatchObject({ installed: "yes" });
    expect(within(dialog).getByText("Hades").closest("li")).toHaveTextContent("Has the category");
    expect(save).toBeEnabled();
    await fireEvent.click(save);
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(api.setCategoryFilter).toHaveBeenCalledWith(
      "c1",
      expect.objectContaining({ installed: "yes" }),
    );
  });

  it("removes an existing filter", async () => {
    vi.mocked(api.setCategoryFilter).mockResolvedValue(category);
    const auto = { ...category, autoFilter: { ...defaultQuery(), favorite: "yes" as const } };
    const onClose = vi.fn<() => void>();
    render(() => <CategoryFilterDialog category={auto} onClose={onClose} />);
    await fireEvent.click(screen.getByRole("button", { name: "Remove filter" }));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(api.setCategoryFilter).toHaveBeenCalledWith("c1", null);
  });
});
