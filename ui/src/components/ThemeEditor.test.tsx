import { fireEvent, render, screen, waitFor, within } from "@solidjs/testing-library";
import { open } from "@tauri-apps/plugin-dialog";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Appearance } from "../bindings/Appearance";
import type { Settings } from "../bindings/Settings";
import type { UserTheme } from "../bindings/UserTheme";
import * as api from "../lib/api";
import { store } from "../lib/store";
import { PREVIEW_DELAY_MS, themeEditor } from "../lib/themeEditor";
import { applyAppearance, cachedTheme, getTheme } from "../lib/themes";
import { setSavedUserThemes, userThemes } from "../lib/userThemeStore";
import { emptyOverrides } from "../lib/userThemes";
import { ThemeEditor } from "./ThemeEditor";

vi.mock("../lib/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => p,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn<() => void>() }));

const root = document.documentElement;
const themeCss = () => document.getElementById("qk-theme")?.textContent ?? "";
const toasts = () => store.toasts().map((t) => t.message);

const APPLIED: Appearance = {
  theme: "fjord",
  scheme: "dark",
  params: { fjord: { accent: "#123456" } },
};

function settings(appearance: Appearance = APPLIED): Settings {
  return {
    language: null,
    showStartPage: true,
    showStatistics: true,
    priceSites: [],
    appearance,
  } as Partial<Settings> as Settings;
}

const neon = (over: Partial<UserTheme> = {}): UserTheme => ({
  id: "user-neon",
  name: "Neon",
  base: "fjord",
  overrides: { ...emptyOverrides(), dark: { "--qk-color-bg": "#010203" } },
  css: "",
  ...over,
});

const savedSettings = () => vi.mocked(api.saveSettings).mock.lastCall?.[0];

beforeEach(() => {
  vi.clearAllMocks();
  store.reset();
  localStorage.clear();
  applyAppearance(APPLIED);
  vi.mocked(api.getSettings).mockResolvedValue(settings());
  vi.mocked(api.saveSettings).mockResolvedValue(undefined);
  vi.mocked(api.saveUserTheme).mockResolvedValue(undefined);
});

afterEach(() => {
  vi.useRealTimers();
  themeEditor.close();
  setSavedUserThemes([]);
});

describe("ThemeEditor", () => {
  it("opens on a new theme made from another and previews it without saving anything", () => {
    expect(themeEditor.create("fjord")).toBe(true);
    render(() => <ThemeEditor />);
    expect(screen.getByRole("complementary", { name: "Theme editor" })).toBeInTheDocument();
    expect(screen.getByLabelText("Name")).toHaveValue("Fjord custom");
    expect(screen.getByLabelText("Based on")).toHaveValue("fjord");
    expect(root.dataset.theme).toBe("user-fjord-custom");
    expect(themeCss()).toContain(':root[data-theme="user-fjord-custom"][data-scheme="dark"]');
    // The parameter values of the theme it was made from come along.
    expect(root.style.getPropertyValue("--qk-param-accent")).toBe("#123456");
    // Nothing is saved, and the next start would not show the draft.
    expect(api.saveSettings).not.toHaveBeenCalled();
    expect(api.saveUserTheme).not.toHaveBeenCalled();
    expect(cachedTheme()?.theme).toBe("fjord");
    // Save is offered for a new theme; exporting needs a saved one.
    expect(screen.getByRole("button", { name: "Save" })).not.toBeDisabled();
    expect(screen.getByRole("button", { name: "Export as plugin…" })).toBeDisabled();
    expect(screen.getByText("Save the theme before exporting it.")).toBeInTheDocument();
  });

  it("shows what a derived color comes out as, with its expression as a tooltip", () => {
    themeEditor.create("fjord");
    render(() => <ThemeEditor />);
    // Written as a plain color in the base theme: shown as it is, nothing to explain.
    const plain = screen.getByLabelText(/^Background/, { selector: "input[type=text]" });
    expect(plain).toHaveAttribute("placeholder", "#131a23");
    expect(plain).not.toHaveAttribute("data-tooltip");
    // Derived from the accent (the parameter is #123456): the result, and how it is made.
    const accent = document.getElementById("theme-var-qk-color-accent") as HTMLElement;
    expect(accent).toHaveAttribute("placeholder", "#123456");
    expect(accent.getAttribute("data-tooltip")).toContain("var(--qk-param-accent");
    // A see-through color keeps its alpha.
    const divider = document.getElementById("theme-var-qk-color-accent-divider") as HTMLElement;
    expect(divider.getAttribute("placeholder")).toMatch(/^#[0-9a-f]{8}$/);
    expect(divider.getAttribute("data-tooltip")).toContain("color-mix(");
    // The swatch shows the same color (without the alpha a color input cannot hold).
    expect(screen.getByLabelText("Accent divider: pick a color")).toHaveValue(
      (divider.getAttribute("placeholder") ?? "").slice(0, 7),
    );
  });

  it("keeps a size's placeholder as it is written", async () => {
    themeEditor.create("fjord");
    render(() => <ThemeEditor />);
    await fireEvent.input(screen.getByRole("searchbox", { name: "Search variables" }), {
      target: { value: "sidebar width" },
    });
    const field = screen.getByLabelText(/^Sidebar width/);
    expect(field).toHaveAttribute("placeholder", "220px");
    expect(field).not.toHaveAttribute("data-tooltip");
  });

  it("applies a picked color at once, marks the variable and resets it", async () => {
    themeEditor.create("fjord");
    render(() => <ThemeEditor />);
    const picker = screen.getByLabelText("Background: pick a color");
    expect(picker).toHaveValue("#131a23");
    const field = screen.getByLabelText(/^Background/, { selector: "input[type=text]" });
    expect(field).toHaveValue("");
    expect(field).toHaveAttribute("placeholder", "#131a23");
    const reset = screen.getByRole("button", { name: "Reset Background" });
    expect(reset).toBeDisabled();

    await fireEvent.input(picker, { target: { value: "#ff0000" } });
    // A color goes to the edited mode only.
    expect(themeCss()).toMatch(/\[data-scheme="dark"\] \{\n {2}--qk-color-bg: #ff0000;/);
    expect(themeEditor.draft().overrides).toEqual({
      both: {},
      light: {},
      dark: { "--qk-color-bg": "#ff0000" },
    });
    expect(field).toHaveValue("#ff0000");
    const row = field.closest("li") as HTMLElement;
    expect(within(row).getByText("Dark")).toBeInTheDocument();
    expect(screen.getByText("1 changed")).toBeInTheDocument();
    expect(themeEditor.isDirty()).toBe(true);

    await fireEvent.click(reset);
    expect(themeCss()).not.toContain("#ff0000");
    expect(field).toHaveValue("");
    expect(reset).toBeDisabled();
    expect(themeEditor.isDirty()).toBe(false);
  });

  it("applies typed values after a pause, steps lengths with the arrow keys", async () => {
    themeEditor.create("fjord");
    render(() => <ThemeEditor />);
    // Component sizes are collapsed; a search shows matching variables of every group.
    expect(screen.queryByLabelText(/^Sidebar width/)).toBeNull();
    await fireEvent.input(screen.getByRole("searchbox", { name: "Search variables" }), {
      target: { value: "sidebar width" },
    });
    const field = screen.getByLabelText(/^Sidebar width/);
    expect(field).toHaveAttribute("placeholder", "220px");
    expect(screen.queryByLabelText(/^Background/, { selector: "input[type=text]" })).toBeNull();

    vi.useFakeTimers();
    await fireEvent.input(field, { target: { value: "300px" } });
    expect(themeCss()).not.toContain("--qk-sidebar-width");
    await vi.advanceTimersByTimeAsync(PREVIEW_DELAY_MS);
    // A size goes to both modes.
    expect(themeCss()).toMatch(/\[data-theme\] \{\n {2}--qk-sidebar-width: 300px;/);
    expect(themeEditor.draft().overrides.both).toEqual({ "--qk-sidebar-width": "300px" });

    await fireEvent.keyDown(field, { key: "ArrowUp" });
    expect(field).toHaveValue("301px");
    expect(themeCss()).toContain("--qk-sidebar-width: 301px;");
    await fireEvent.keyDown(field, { key: "ArrowDown", shiftKey: true });
    expect(field).toHaveValue("291px");

    // What could end the declaration is refused and said so.
    await fireEvent.input(field, { target: { value: "1px; } body { display: none" } });
    await vi.advanceTimersByTimeAsync(PREVIEW_DELAY_MS);
    expect(field).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByRole("alert")).toHaveTextContent("Values cannot contain { } ; or </.");
    expect(themeCss()).not.toContain("display: none");
    expect(themeEditor.draft().overrides.both).toEqual({ "--qk-sidebar-width": "291px" });

    await fireEvent.input(screen.getByRole("searchbox"), { target: { value: "zzz" } });
    expect(screen.getByText("No variable matches.")).toBeInTheDocument();
  });

  it("edits one mode at a time and shows that mode", async () => {
    themeEditor.create("fjord");
    render(() => <ThemeEditor />);
    const mode = screen.getByRole("radiogroup", { name: "Mode being edited" });
    expect(within(mode).getByRole("radio", { name: "Dark" })).toBeChecked();
    await fireEvent.click(within(mode).getByRole("radio", { name: "Light" }));
    expect(root.dataset.scheme).toBe("light");
    expect(screen.getByLabelText("Background: pick a color")).toHaveValue("#edf1f5");
    await fireEvent.input(screen.getByLabelText("Background: pick a color"), {
      target: { value: "#ffeedd" },
    });
    expect(themeEditor.draft().overrides.light).toEqual({ "--qk-color-bg": "#ffeedd" });

    // "Both modes" sends colors to both as well.
    await fireEvent.change(screen.getByLabelText("Changes apply to"), {
      target: { value: "both" },
    });
    await fireEvent.input(screen.getByLabelText("Text: pick a color"), {
      target: { value: "#000000" },
    });
    expect(themeEditor.draft().overrides.both).toEqual({ "--qk-color-text": "#000000" });
    const row = screen.getByLabelText(/^Text/, { selector: "#theme-var-qk-color-text" });
    expect(within(row.closest("li") as HTMLElement).getByText("Both")).toBeInTheDocument();
    expect(api.saveSettings).not.toHaveBeenCalled();
  });

  it("says when the theme sets the accent itself", async () => {
    themeEditor.create("fjord");
    render(() => <ThemeEditor />);
    expect(screen.queryByText(/sets the accent color itself/)).toBeNull();
    await fireEvent.input(screen.getByLabelText("Accent: pick a color"), {
      target: { value: "#ff00ff" },
    });
    expect(screen.getByText(/sets the accent color itself/)).toBeInTheDocument();
  });

  it("applies custom CSS only inside the theme and warns about a stray brace", async () => {
    themeEditor.create("fjord");
    render(() => <ThemeEditor />);
    const css = screen.getByLabelText("Custom CSS");
    vi.useFakeTimers();
    await fireEvent.input(css, { target: { value: ".btn { text-transform: uppercase; }" } });
    await vi.advanceTimersByTimeAsync(PREVIEW_DELAY_MS);
    expect(themeCss()).toContain(
      ':root[data-theme="user-fjord-custom"] {\n.btn { text-transform: uppercase; }\n}',
    );
    await fireEvent.input(css, { target: { value: "} body { display: none; }" } });
    await vi.advanceTimersByTimeAsync(PREVIEW_DELAY_MS);
    expect(themeCss()).not.toContain("display: none");
    expect(screen.getByRole("alert")).toHaveTextContent("There is a } too many");
  });

  it("discards changes and restores the previous look exactly on close", async () => {
    const before = { css: themeCss(), accent: root.style.getPropertyValue("--qk-param-accent") };
    themeEditor.create("fjord");
    const view = render(() => <ThemeEditor />);
    await fireEvent.input(screen.getByLabelText("Background: pick a color"), {
      target: { value: "#ff0000" },
    });
    await fireEvent.click(screen.getByRole("radio", { name: "Light" }));
    await fireEvent.click(screen.getByRole("button", { name: "Discard changes" }));
    expect(themeCss()).not.toContain("#ff0000");
    expect(themeEditor.isDirty()).toBe(false);
    expect(screen.getByRole("button", { name: "Discard changes" })).toBeDisabled();

    // Nothing to lose: Close closes right away.
    await fireEvent.click(screen.getAllByRole("button", { name: "Close" })[0] as HTMLElement);
    expect(themeEditor.isOpen()).toBe(false);
    view.unmount();
    expect(root.dataset.theme).toBe("fjord");
    expect(root.dataset.scheme).toBe("dark");
    expect(themeCss()).toBe(before.css);
    expect(root.style.getPropertyValue("--qk-param-accent")).toBe(before.accent);
    expect(getTheme("user-fjord-custom")).toBeUndefined();
    expect(api.saveSettings).not.toHaveBeenCalled();
    expect(api.saveUserTheme).not.toHaveBeenCalled();
  });

  it("asks before closing with unsaved changes; Escape does not close", async () => {
    themeEditor.create("fjord");
    render(() => <ThemeEditor />);
    const panel = screen.getByRole("complementary");
    await fireEvent.keyDown(screen.getByLabelText("Name"), { key: "Escape" });
    expect(themeEditor.isOpen()).toBe(true);
    await fireEvent.input(screen.getByLabelText("Background: pick a color"), {
      target: { value: "#ff0000" },
    });
    await fireEvent.click(
      within(panel).getAllByRole("button", { name: "Close" })[0] as HTMLElement,
    );
    const dialog = screen.getByRole("dialog", { name: "Unsaved theme changes" });
    expect(dialog).toHaveTextContent("“Fjord custom” has changes that are not saved.");
    await fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(themeEditor.isOpen()).toBe(true);
    expect(themeCss()).toContain("#ff0000");

    await fireEvent.click(
      within(panel).getAllByRole("button", { name: "Close" })[0] as HTMLElement,
    );
    await fireEvent.click(
      within(screen.getByRole("dialog")).getByRole("button", { name: "Discard changes" }),
    );
    expect(themeEditor.isOpen()).toBe(false);
    expect(root.dataset.theme).toBe("fjord");
    expect(api.saveUserTheme).not.toHaveBeenCalled();
  });

  it("saves a new theme under an id from its name and selects it", async () => {
    themeEditor.create("fjord");
    render(() => <ThemeEditor />);
    await fireEvent.input(screen.getByLabelText("Name"), { target: { value: " Neon " } });
    await fireEvent.input(screen.getByLabelText("Background: pick a color"), {
      target: { value: "#010203" },
    });
    await fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(api.saveUserTheme).toHaveBeenCalledWith(neon()));
    await waitFor(() =>
      expect(savedSettings()?.appearance).toEqual({
        theme: "user-neon",
        scheme: "dark",
        // The accent the user had on Fjord stays with the new theme.
        params: { fjord: { accent: "#123456" }, "user-neon": { accent: "#123456" } },
      }),
    );
    await waitFor(() => expect(toasts()).toContain("Saved the theme “Neon”"));
    expect(userThemes()).toEqual([neon()]);
    expect(getTheme("user-neon")?.origin).toBe("user");
    expect(getTheme("user-fjord-custom")).toBeUndefined();
    // The editor goes on with the saved theme.
    expect(themeEditor.isOpen()).toBe(true);
    expect(themeEditor.isDirty()).toBe(false);
    expect(themeEditor.isNew()).toBe(false);
    expect(root.dataset.theme).toBe("user-neon");
    expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Export as plugin…" })).not.toBeDisabled();
    expect(themeEditor.settingsSaved()).toBeGreaterThan(0);

    // Closing now keeps the selected theme on the page.
    themeEditor.close();
    expect(root.dataset.theme).toBe("user-neon");
    expect(themeCss()).toContain("--qk-color-bg: #010203;");
  });

  it("saves an existing theme in place, or as a copy that gets selected", async () => {
    setSavedUserThemes([neon()]);
    expect(themeEditor.edit("user-neon")).toBe(true);
    render(() => <ThemeEditor />);
    expect(screen.getByLabelText("Name")).toHaveValue("Neon");
    expect(root.dataset.theme).toBe("user-neon");
    await fireEvent.input(screen.getByLabelText("Background: pick a color"), {
      target: { value: "#0a0b0c" },
    });
    await fireEvent.click(screen.getByRole("button", { name: "Save" }));
    const changed = neon({
      overrides: { ...emptyOverrides(), dark: { "--qk-color-bg": "#0a0b0c" } },
    });
    await waitFor(() => expect(api.saveUserTheme).toHaveBeenCalledWith(changed));
    await waitFor(() => expect(toasts()).toContain("Saved the theme “Neon”"));
    // Saving in place leaves the settings alone.
    expect(api.saveSettings).not.toHaveBeenCalled();
    expect(userThemes()).toEqual([changed]);

    await fireEvent.click(screen.getByRole("button", { name: "Save as copy" }));
    const copy = { ...changed, id: "user-neon-copy", name: "Neon copy" };
    await waitFor(() => expect(api.saveUserTheme).toHaveBeenLastCalledWith(copy));
    await waitFor(() => expect(savedSettings()?.appearance.theme).toBe("user-neon-copy"));
    expect(userThemes().map((t) => t.id)).toEqual(["user-neon", "user-neon-copy"]);
    expect(themeEditor.draft().id).toBe("user-neon-copy");
    expect(screen.getByLabelText("Name")).toHaveValue("Neon copy");
  });

  it("keeps the draft when saving fails, and wants a name", async () => {
    themeEditor.create("fjord");
    render(() => <ThemeEditor />);
    vi.mocked(api.saveUserTheme).mockRejectedValueOnce("Disk full");
    await fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(toasts()).toContain("Disk full"));
    expect(themeEditor.isNew()).toBe(true);
    expect(userThemes()).toEqual([]);
    await fireEvent.input(screen.getByLabelText("Name"), { target: { value: "   " } });
    await fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(toasts()).toContain("Give the theme a name."));
    expect(api.saveUserTheme).toHaveBeenCalledTimes(1);
  });

  it("exports the saved theme as a plugin into a chosen folder", async () => {
    setSavedUserThemes([neon()]);
    themeEditor.edit("user-neon");
    render(() => <ThemeEditor />);
    vi.mocked(open).mockResolvedValueOnce(null);
    await fireEvent.click(screen.getByRole("button", { name: "Export as plugin…" }));
    await waitFor(() => expect(open).toHaveBeenCalledTimes(1));
    expect(api.exportUserTheme).not.toHaveBeenCalled();

    vi.mocked(open).mockResolvedValueOnce("D:/themes");
    vi.mocked(api.exportUserTheme).mockResolvedValue("D:/themes/neon-theme");
    await fireEvent.click(screen.getByRole("button", { name: "Export as plugin…" }));
    await waitFor(() => expect(toasts()).toContain("Exported to D:/themes/neon-theme"));
    const [id, folder, exported] = vi.mocked(api.exportUserTheme).mock.lastCall ?? [];
    expect(id).toBe("user-neon");
    expect(folder).toBe("D:/themes");
    expect(exported?.css).toContain(':root[data-theme="neon-theme"][data-scheme="dark"]');
    expect(exported?.css).toContain("--qk-color-bg: #010203;");
    expect(exported?.css).not.toContain("user-neon");
    expect(exported?.params).toEqual([
      expect.objectContaining({ key: "accent", label: "Accent color", type: "color" }),
    ]);
    expect(exported?.preview).toMatchObject({ dark: expect.arrayContaining(["#010203"]) });
  });

  it("docks to the other side with a button", async () => {
    themeEditor.create("fjord");
    render(() => <ThemeEditor />);
    const panel = screen.getByRole("complementary");
    expect(panel).not.toHaveClass("theme-editor--left");
    await fireEvent.click(screen.getByRole("button", { name: "Dock left" }));
    expect(panel).toHaveClass("theme-editor--left");
    await fireEvent.click(screen.getByRole("button", { name: "Dock right" }));
    expect(panel).not.toHaveClass("theme-editor--left");
  });

  it("does not open over unsaved changes", () => {
    setSavedUserThemes([neon()]);
    themeEditor.create("fjord");
    themeEditor.update({ css: ".a {}" }, true);
    expect(themeEditor.edit("user-neon")).toBe(false);
    expect(themeEditor.create("ember")).toBe(false);
    expect(toasts()).toContain("Save or discard the theme you are editing first.");
    expect(themeEditor.draft().base).toBe("fjord");
    themeEditor.discard();
    // Without changes the editor just moves on to the other theme.
    expect(themeEditor.edit("user-neon")).toBe(true);
    expect(themeEditor.draft().id).toBe("user-neon");
    expect(getTheme("user-fjord-custom")).toBeUndefined();
    expect(themeEditor.edit("user-missing")).toBe(false);
  });

  it("builds on the default theme while the base theme is missing", () => {
    setSavedUserThemes([neon({ base: "plugin.gone" })]);
    expect(getTheme("user-neon")?.params.map((p) => p.key)).toContain("density");
    themeEditor.edit("user-neon");
    render(() => <ThemeEditor />);
    expect(screen.getByLabelText("Based on")).toHaveValue("plugin.gone");
    expect(screen.getByText(/“plugin.gone” is not available right now/)).toBeInTheDocument();
  });
});
