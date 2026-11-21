import { afterEach, describe, expect, it } from "vitest";
import { installFocusRing } from "./focusRing";

const on = () => document.documentElement.hasAttribute("data-keyboard-focus");

describe("focus ring", () => {
  let uninstall: (() => void) | undefined;
  afterEach(() => uninstall?.());

  it("turns on with Tab, not with Shift, and off with a pointer press", () => {
    uninstall = installFocusRing();
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Shift", shiftKey: true }));
    expect(on()).toBe(false);
    document.dispatchEvent(new KeyboardEvent("keydown", { key: "Tab", shiftKey: true }));
    expect(on()).toBe(true);
    document.dispatchEvent(new Event("pointerdown"));
    expect(on()).toBe(false);
  });
});
