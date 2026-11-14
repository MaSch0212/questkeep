import { fireEvent, render, screen } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { TOOLTIP_DELAY_MS } from "../lib/tooltip";
import { TooltipLayer } from "./Tooltip";

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

function renderButtons() {
  render(() => (
    <>
      <button type="button" data-tooltip="Add game">
        <span>+</span>
      </button>
      <button type="button" data-tooltip="Settings">
        S
      </button>
      <p>plain</p>
      <TooltipLayer />
    </>
  ));
}

describe("TooltipLayer", () => {
  it("shows after a short rest, at once while warm, and hides on leaving", async () => {
    renderButtons();
    const add = screen.getByRole("button", { name: "+" });
    await fireEvent.pointerOver(add.querySelector("span") as Element);
    expect(screen.queryByRole("tooltip")).toBeNull();
    await vi.advanceTimersByTimeAsync(TOOLTIP_DELAY_MS);
    expect(screen.getByRole("tooltip")).toHaveTextContent("Add game");
    expect(add).toHaveAttribute("aria-describedby", "qk-tooltip");

    await fireEvent.pointerOver(screen.getByRole("button", { name: "S" }));
    expect(screen.getByRole("tooltip")).toHaveTextContent("Settings");
    expect(add).not.toHaveAttribute("aria-describedby");

    await fireEvent.pointerOver(screen.getByText("plain"));
    expect(screen.queryByRole("tooltip")).toBeNull();
  });

  it("stays away while the element's popup is open", async () => {
    renderButtons();
    const settings = screen.getByRole("button", { name: "S" });
    await fireEvent.pointerOver(settings);
    await vi.advanceTimersByTimeAsync(TOOLTIP_DELAY_MS);
    expect(screen.getByRole("tooltip")).toBeInTheDocument();
    settings.setAttribute("aria-expanded", "true");
    await vi.advanceTimersByTimeAsync(0);
    expect(screen.queryByRole("tooltip")).toBeNull();
  });

  it("follows text changes and hides on a click", async () => {
    renderButtons();
    const settings = screen.getByRole("button", { name: "S" });
    await fireEvent.pointerOver(settings);
    await vi.advanceTimersByTimeAsync(TOOLTIP_DELAY_MS);
    settings.dataset.tooltip = "Settings (open)";
    await vi.advanceTimersByTimeAsync(0);
    expect(screen.getByRole("tooltip")).toHaveTextContent("Settings (open)");
    await fireEvent.pointerDown(settings);
    expect(screen.queryByRole("tooltip")).toBeNull();
  });
});
