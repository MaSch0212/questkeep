import { cleanup } from "@solidjs/testing-library";
import "@testing-library/jest-dom/vitest";
import { afterEach } from "vitest";
import { setLanguage } from "./i18n";

// jsdom has no layout engine: provide the few browser APIs the UI relies on.
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub;
Element.prototype.scrollTo ??= () => {};

// Tests read the English texts; a test that switches the language must not leak it.
setLanguage("en");
afterEach(() => {
  cleanup();
  setLanguage("en");
});
