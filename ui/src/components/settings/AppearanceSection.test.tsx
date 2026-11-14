import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Appearance } from "../../bindings/Appearance";
import type { Settings } from "../../bindings/Settings";
import { setLanguage } from "../../i18n";
import * as api from "../../lib/api";
import { store } from "../../lib/store";
import {
  applyAppearance,
  DEFAULT_APPEARANCE,
  registerTheme,
  unregisterTheme,
  type ThemeDef,
} from "../../lib/themes";
import { AppearanceSection } from "./AppearanceSection";
import { createSettingsModel } from "./useSettings";

vi.mock("../../lib/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => p,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));

const root = document.documentElement;

function settings(appearance: Partial<Appearance> = {}): Settings {
  // Only the appearance matters here; the section passes everything else through.
  return {
    language: null,
    showStartPage: true,
    showStatistics: true,
    priceSites: [],
    appearance: { ...DEFAULT_APPEARANCE, ...appearance },
  } as Partial<Settings> as Settings;
}

function renderSection(appearance: Partial<Appearance> = {}) {
  vi.mocked(api.getSettings).mockResolvedValue(settings(appearance));
  return render(() => {
    const model = createSettingsModel();
    return <AppearanceSection model={model} />;
  });
}

const lastSaved = () => vi.mocked(api.saveSettings).mock.lastCall?.[0].appearance;

const neon: ThemeDef = {
  id: "plugin.neon",
  name: "Neon",
  description: "Bright lights.",
  author: "Ada",
  css: "",
  origin: "plugin",
  params: [
    { key: "glow", label: "Glow", type: "toggle", default: false, hint: "Adds a glow." },
    { key: "secret", label: "Secret", type: "toggle", default: false, advanced: true },
  ],
};

beforeEach(() => {
  vi.clearAllMocks();
  store.reset();
  localStorage.clear();
  vi.mocked(api.saveSettings).mockResolvedValue(undefined);
  applyAppearance({ ...DEFAULT_APPEARANCE, scheme: "dark" });
});

afterEach(() => {
  vi.useRealTimers();
  unregisterTheme("plugin.neon");
});

describe("AppearanceSection", () => {
  it("switches the mode and applies it to the page", async () => {
    renderSection({ scheme: "dark" });
    const mode = await screen.findByRole("radiogroup", { name: "Mode" });
    expect(within(mode).getByRole("radio", { name: "Dark" })).toBeChecked();
    await fireEvent.click(within(mode).getByRole("radio", { name: "Light" }));
    expect(root.dataset.scheme).toBe("light");
    await waitFor(() => expect(lastSaved()?.scheme).toBe("light"));
    expect(within(mode).getByRole("radio", { name: "Light" })).toBeChecked();
  });

  it("lists the themes as a radio group and switches the theme", async () => {
    renderSection({ scheme: "dark" });
    const picker = await screen.findByRole("radiogroup", { name: "Theme" });
    const cards = within(picker).getAllByRole("radio");
    expect(cards.length).toBeGreaterThanOrEqual(7);
    expect(within(picker).getByRole("radio", { name: "Questkeep" })).toBeChecked();
    expect(within(picker).getByRole("radio", { name: "Fjord" })).toHaveAccessibleDescription(
      "Cool blue-grey surfaces with an icy blue accent.",
    );
    // Every radio shares one name, so the arrow keys move between the themes.
    expect(new Set(cards.map((c) => c.getAttribute("name"))).size).toBe(1);
    // The preview shows the theme's own colors for the current scheme.
    const preview = within(picker)
      .getByRole("radio", { name: "Fjord" })
      .closest("label")
      ?.querySelector<HTMLElement>(".theme-card__preview");
    expect(preview?.style.getPropertyValue("--qk-preview-bg")).toBe("#131a23");
    expect(preview?.style.getPropertyValue("--qk-preview-accent")).toBe("#6cc5e8");

    await fireEvent.click(within(picker).getByRole("radio", { name: "Fjord" }));
    expect(root.dataset.theme).toBe("fjord");
    await waitFor(() => expect(lastSaved()?.theme).toBe("fjord"));
    // The options are now Fjord's.
    expect(screen.getByRole("group", { name: "Fjord options" })).toBeInTheDocument();
    expect(screen.queryByLabelText("Corners")).toBeNull();
    expect(screen.getByLabelText("Accent color")).toHaveValue("#6cc5e8");
  });

  it("changes a select parameter, shows it as changed and resets it", async () => {
    renderSection({ scheme: "dark" });
    const corners = await screen.findByLabelText("Corners");
    expect(corners).toHaveValue("default");
    const reset = screen.getByRole("button", { name: "Reset Corners" });
    expect(reset).toBeDisabled();
    await fireEvent.change(corners, { target: { value: "round" } });
    expect(root.getAttribute("data-param-corners")).toBe("round");
    await waitFor(() => expect(lastSaved()?.params).toEqual({ questkeep: { corners: "round" } }));
    expect(reset).not.toBeDisabled();
    await fireEvent.click(reset);
    expect(root.getAttribute("data-param-corners")).toBe("default");
    await waitFor(() => expect(lastSaved()?.params).toEqual({}));
    expect(reset).toBeDisabled();
  });

  it("applies a dragged color at once and saves it after a pause", async () => {
    renderSection({ scheme: "dark" });
    const color = await screen.findByLabelText("Accent color");
    expect(color).toHaveValue("#7c9cff");
    vi.useFakeTimers();
    await fireEvent.input(color, { target: { value: "#ff8800" } });
    await fireEvent.input(color, { target: { value: "#ff9900" } });
    expect(root.style.getPropertyValue("--qk-param-accent")).toBe("#ff9900");
    expect(screen.getByLabelText("Accent color as a hex value")).toHaveValue("#ff9900");
    expect(api.saveSettings).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(200);
    expect(api.saveSettings).toHaveBeenCalledTimes(1);
    expect(lastSaved()?.params).toEqual({ questkeep: { accent: "#ff9900" } });
  });

  it("takes a typed hex color and rejects anything else", async () => {
    renderSection({ scheme: "dark" });
    const hex = await screen.findByLabelText("Accent color as a hex value");
    await fireEvent.change(hex, { target: { value: "not a color" } });
    expect(hex).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByRole("alert")).toHaveTextContent("Enter a color like #7c9cff.");
    expect(api.saveSettings).not.toHaveBeenCalled();
    await fireEvent.change(hex, { target: { value: "F80" } });
    expect(screen.queryByRole("alert")).toBeNull();
    await waitFor(() => expect(lastSaved()?.params).toEqual({ questkeep: { accent: "#ff8800" } }));
    expect(screen.getByLabelText("Accent color")).toHaveValue("#ff8800");
  });

  it("changes a number with the slider and shows the value", async () => {
    renderSection({ scheme: "dark" });
    const size = await screen.findByLabelText("Text size (%)");
    expect(size).toHaveAttribute("min", "85");
    expect(size).toHaveAttribute("max", "130");
    vi.useFakeTimers();
    await fireEvent.input(size, { target: { value: "115" } });
    expect(root.style.getPropertyValue("--qk-param-text-size")).toBe("115");
    expect(size.closest(".param-row")?.querySelector("output")).toHaveTextContent("115");
    await vi.advanceTimersByTimeAsync(200);
    expect(lastSaved()?.params).toEqual({ questkeep: { "text-size": 115 } });
  });

  it("keeps advanced parameters in a collapsed disclosure and resets everything", async () => {
    renderSection({
      scheme: "dark",
      params: { questkeep: { flat: true, corners: "sharp" }, fjord: { accent: "#123456" } },
    });
    const flat = await screen.findByRole("checkbox", {
      name: "Reduce transparency and shadows",
    });
    const details = flat.closest("details");
    expect(details).not.toBeNull();
    expect(details).not.toHaveAttribute("open");
    expect(details?.querySelector("summary")).toHaveTextContent("Advanced");
    expect(screen.getByLabelText("Corners").closest("details")).toBeNull();
    expect(flat).toBeChecked();

    await fireEvent.click(screen.getByRole("button", { name: "Reset all to theme defaults" }));
    // Only this theme's values go; other themes keep theirs.
    await waitFor(() => expect(lastSaved()?.params).toEqual({ fjord: { accent: "#123456" } }));
    expect(flat).not.toBeChecked();
    expect(root.getAttribute("data-param-flat")).toBe("0");
    expect(screen.getByRole("button", { name: "Reset all to theme defaults" })).toBeDisabled();
  });

  it("shows plugin themes with a badge, their own texts and parameters", async () => {
    registerTheme(neon);
    renderSection({ scheme: "dark", theme: "plugin.neon" });
    const card = await screen.findByRole("radio", { name: "Neon" });
    expect(card).toBeChecked();
    const label = card.closest("label") as HTMLElement;
    expect(within(label).getByText("Plugin")).toBeInTheDocument();
    expect(within(label).getByText("by Ada")).toBeInTheDocument();
    expect(card).toHaveAccessibleDescription("Bright lights.");
    const glow = screen.getByRole("checkbox", { name: "Glow" });
    expect(glow).toHaveAccessibleDescription("Adds a glow.");
    await fireEvent.click(glow);
    await waitFor(() => expect(lastSaved()?.params).toEqual({ "plugin.neon": { glow: true } }));
    expect(root.getAttribute("data-param-glow")).toBe("1");
  });

  it("says so when the chosen theme is missing, and keeps the choice", async () => {
    renderSection({ scheme: "dark", theme: "plugin.neon" });
    expect(await screen.findByText(/The theme “plugin.neon” is not available/)).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Questkeep" })).toBeChecked();
    await fireEvent.change(screen.getByLabelText("Density"), { target: { value: "compact" } });
    await waitFor(() => expect(lastSaved()?.params).toEqual({ questkeep: { density: "compact" } }));
    expect(lastSaved()?.theme).toBe("plugin.neon");
    // The theme arrives (its plugin loaded): it is selected without further ado.
    registerTheme(neon);
    expect(await screen.findByRole("radio", { name: "Neon" })).toBeChecked();
    expect(screen.queryByText(/is not available/)).toBeNull();
  });

  it("goes back to the saved look when saving fails", async () => {
    vi.mocked(api.saveSettings).mockRejectedValue("Disk full");
    renderSection({ scheme: "dark" });
    await fireEvent.click(await screen.findByRole("radio", { name: "Ember" }));
    expect(root.dataset.theme).toBe("ember");
    await waitFor(() => expect(store.toasts().map((t) => t.message)).toContain("Disk full"));
    await waitFor(() => expect(root.dataset.theme).toBe("questkeep"));
    expect(screen.getByRole("radio", { name: "Questkeep" })).toBeChecked();
  });

  it("is translated, theme names aside", async () => {
    setLanguage("de");
    renderSection({ scheme: "dark" });
    expect(await screen.findByRole("radiogroup", { name: "Modus" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Dunkel" })).toBeChecked();
    expect(screen.getByRole("radio", { name: "High contrast" })).toBeInTheDocument();
    expect(screen.getByLabelText("Akzentfarbe")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Ecken zurücksetzen" })).toBeInTheDocument();
  });
});
