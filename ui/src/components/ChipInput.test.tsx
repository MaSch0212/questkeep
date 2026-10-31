import { fireEvent, render, screen } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { describe, expect, it } from "vitest";
import { ChipInput } from "./ChipInput";

function Harness(props: { initial?: string[] }) {
  const [values, setValues] = createSignal(props.initial ?? []);
  return (
    <>
      <ChipInput
        label="Genres"
        values={values()}
        suggestions={["Action", "Adventure", "Puzzle"]}
        onChange={setValues}
      />
      <output data-testid="values">{values().join("|")}</output>
    </>
  );
}

const values = () => screen.getByTestId("values").textContent;

describe("ChipInput", () => {
  it("lists matching suggestions under the box and adds the chosen one", async () => {
    render(() => <Harness initial={["Puzzle"]} />);
    const input = screen.getByRole("combobox", { name: "Genres" });
    await fireEvent.focus(input);
    // Values already added are not offered again.
    expect(screen.getAllByRole("option").map((o) => o.textContent)).toEqual([
      "Action",
      "Adventure",
    ]);
    await fireEvent.input(input, { target: { value: "adv" } });
    expect(screen.getAllByRole("option").map((o) => o.textContent)).toEqual(["Adventure"]);
    await fireEvent.keyDown(input, { key: "ArrowDown" });
    expect(input).toHaveAttribute("aria-activedescendant");
    await fireEvent.keyDown(input, { key: "Enter" });
    expect(values()).toBe("Puzzle|Adventure");
  });

  it("adds typed text, picks with a click and closes on Escape", async () => {
    render(() => <Harness />);
    const input = screen.getByRole("combobox", { name: "Genres" });
    await fireEvent.input(input, { target: { value: "Racing" } });
    await fireEvent.keyDown(input, { key: "Enter" });
    expect(values()).toBe("Racing");
    await fireEvent.input(input, { target: { value: "act" } });
    await fireEvent.click(screen.getByRole("option", { name: "Action" }));
    expect(values()).toBe("Racing|Action");
    await fireEvent.input(input, { target: { value: "a" } });
    expect(screen.queryAllByRole("option").length).toBeGreaterThan(0);
    await fireEvent.keyDown(input, { key: "Escape" });
    expect(screen.queryAllByRole("option")).toHaveLength(0);
  });
});
