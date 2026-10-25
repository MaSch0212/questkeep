import { fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { openUrl } from "@tauri-apps/plugin-opener";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AccountStatus } from "../../bindings/AccountStatus";
import type { Settings } from "../../bindings/Settings";
import * as api from "../../lib/api";
import { store } from "../../lib/store";
import { AccountsSection } from "./AccountsSection";
import { ArtworkSection } from "./ArtworkSection";
import { UpdatesSection } from "./UpdatesSection";
import { createSettingsModel } from "./useSettings";

vi.mock("../../lib/api");
vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => p,
  invoke: vi.fn<() => void>(),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));
vi.mock("@tauri-apps/plugin-opener", () => ({
  openPath: vi.fn<() => void>(),
  openUrl: vi.fn<() => void>(),
}));

const toasts = () => store.toasts().map((t) => t.message);

const gog = (over: Partial<AccountStatus> = {}): AccountStatus => ({
  provider: "gog",
  name: "GOG",
  signedIn: false,
  displayName: null,
  supportsCodeSignIn: true,
  codeUrl: "https://auth.gog.com/code",
  ...over,
});
const epic = (over: Partial<AccountStatus> = {}): AccountStatus => ({
  provider: "epic",
  name: "Epic Games",
  signedIn: true,
  displayName: "Ada",
  supportsCodeSignIn: false,
  codeUrl: null,
  ...over,
});

const xbox = (over: Partial<AccountStatus> = {}): AccountStatus => ({
  provider: "xbox",
  name: "Xbox",
  signedIn: false,
  displayName: null,
  supportsCodeSignIn: true,
  codeUrl: "https://login.live.com/oauth20_authorize.srf?client_id=1",
  ...over,
});

beforeEach(() => {
  vi.clearAllMocks();
  store.reset();
});

const steam = (over: Partial<AccountStatus> = {}): AccountStatus => ({
  provider: "steam",
  name: "Steam",
  signedIn: false,
  displayName: null,
  supportsCodeSignIn: false,
  codeUrl: null,
  ...over,
});

function renderAccounts() {
  return render(() => {
    // Created once: a JSX prop expression runs again on every read.
    const model = createSettingsModel();
    return <AccountsSection model={model} />;
  });
}

describe("AccountsSection", () => {
  beforeEach(() => {
    vi.mocked(api.getSettings).mockResolvedValue(makeSettings());
  });

  it("renders each store's status", async () => {
    vi.mocked(api.accountStatus).mockResolvedValue([gog(), epic()]);
    renderAccounts();
    expect(await screen.findByText("Signed in as Ada")).toBeInTheDocument();
    expect(screen.getByText("Not signed in")).toBeInTheDocument();
    expect(screen.getByText("1 of 2 accounts connected")).toBeInTheDocument();
    expect(screen.getByRole("group", { name: "Epic Games" })).toHaveClass("account--connected");
    expect(screen.getByRole("group", { name: "GOG" })).not.toHaveClass("account--connected");
    expect(screen.getByRole("button", { name: "Sign in to GOG" })).toBeInTheDocument();
    // Account actions sit in the collapsed part of the row.
    expect(screen.queryByRole("button", { name: "Sign out of Epic Games" })).toBeNull();
    const toggle = screen.getByRole("button", { name: "Epic Games settings" });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    await fireEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("button", { name: "Sign out of Epic Games" })).toBeInTheDocument();
  });

  it("shows a busy state while signing in, then toasts and refreshes", async () => {
    let finish: ((ok: boolean) => void) | undefined;
    vi.mocked(api.accountStatus).mockResolvedValueOnce([gog()]);
    vi.mocked(api.signIn).mockReturnValue(new Promise<boolean>((r) => (finish = r)));
    renderAccounts();
    await fireEvent.click(await screen.findByRole("button", { name: "Sign in to GOG" }));
    expect(api.signIn).toHaveBeenCalledWith("gog");
    expect(screen.getByRole("button", { name: "Sign in to GOG" })).toBeDisabled();
    expect(screen.getByText("Waiting for sign-in…")).toBeInTheDocument();
    expect(screen.getByText(/Finish signing in in the window that opened/)).toBeInTheDocument();
    vi.mocked(api.accountStatus).mockResolvedValue([gog({ signedIn: true, displayName: "Bob" })]);
    finish?.(true);
    expect(await screen.findByText("Signed in as Bob")).toBeInTheDocument();
    expect(toasts()).toContain("Signed in to GOG. Importing your library…");
  });

  it("does nothing when the sign-in window was closed", async () => {
    vi.mocked(api.accountStatus).mockResolvedValue([gog()]);
    vi.mocked(api.signIn).mockResolvedValue(false);
    renderAccounts();
    await fireEvent.click(await screen.findByRole("button", { name: "Sign in to GOG" }));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Sign in to GOG" })).not.toBeDisabled(),
    );
    expect(toasts()).toEqual([]);
    expect(api.accountStatus).toHaveBeenCalledTimes(1);
  });

  it("submits a trimmed code from the fallback", async () => {
    vi.mocked(api.accountStatus).mockResolvedValue([gog()]);
    vi.mocked(api.signInWithCode).mockResolvedValue(gog({ signedIn: true }));
    renderAccounts();
    await fireEvent.click(await screen.findByRole("button", { name: "GOG settings" }));
    await fireEvent.click(
      await screen.findByRole("button", { name: /Sign in with a code instead/ }),
    );
    await fireEvent.click(screen.getByRole("button", { name: "Open sign-in page" }));
    expect(openUrl).toHaveBeenCalledWith("https://auth.gog.com/code");
    await fireEvent.input(screen.getByLabelText("GOG sign-in code"), {
      target: { value: "  abc123  " },
    });
    await fireEvent.click(screen.getByRole("button", { name: "Submit code" }));
    await waitFor(() => expect(api.signInWithCode).toHaveBeenCalledWith("gog", "abc123"));
    await waitFor(() => expect(toasts()).toContain("Signed in to GOG. Importing your library…"));
  });

  it("lists Xbox from the account status and accepts the pasted redirect address", async () => {
    vi.mocked(api.accountStatus).mockResolvedValue([xbox()]);
    vi.mocked(api.signInWithCode).mockResolvedValue(xbox({ signedIn: true }));
    renderAccounts();
    expect(await screen.findByRole("group", { name: "Xbox" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Sign in to Xbox" })).toBeInTheDocument();
    await fireEvent.click(await screen.findByRole("button", { name: "Xbox settings" }));
    await fireEvent.click(screen.getByRole("button", { name: /Sign in with a code instead/ }));
    expect(screen.getByText(/The browser ends on a blank page/)).toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: "Open sign-in page" }));
    expect(openUrl).toHaveBeenCalledWith(
      "https://login.live.com/oauth20_authorize.srf?client_id=1",
    );
    const address = "https://login.live.com/oauth20_desktop.srf?code=M.C507_BAY.2.U.abcd1234";
    await fireEvent.input(screen.getByLabelText("Xbox sign-in code"), {
      target: { value: address },
    });
    await fireEvent.click(screen.getByRole("button", { name: "Submit code" }));
    await waitFor(() => expect(api.signInWithCode).toHaveBeenCalledWith("xbox", address));
    await waitFor(() => expect(toasts()).toContain("Signed in to Xbox. Importing your library…"));
  });

  it("shows errors as toasts", async () => {
    vi.mocked(api.accountStatus).mockResolvedValue([gog()]);
    vi.mocked(api.signIn).mockRejectedValue("Login failed");
    renderAccounts();
    await fireEvent.click(await screen.findByRole("button", { name: "Sign in to GOG" }));
    await waitFor(() => expect(toasts()).toContain("Login failed"));
  });

  it("signs out after an inline confirmation", async () => {
    vi.mocked(api.accountStatus).mockResolvedValueOnce([epic()]);
    vi.mocked(api.signOut).mockResolvedValue(epic({ signedIn: false }));
    renderAccounts();
    await fireEvent.click(await screen.findByRole("button", { name: "Epic Games settings" }));
    await fireEvent.click(await screen.findByRole("button", { name: "Sign out of Epic Games" }));
    expect(api.signOut).not.toHaveBeenCalled();
    vi.mocked(api.accountStatus).mockResolvedValue([epic({ signedIn: false, displayName: null })]);
    await fireEvent.click(screen.getByRole("button", { name: "Confirm sign out" }));
    await waitFor(() => expect(api.signOut).toHaveBeenCalledWith("epic"));
    expect(await screen.findByText("Not signed in")).toBeInTheDocument();
    expect(toasts()).toContain("Signed out of Epic Games");
  });

  it("opens and closes a row by clicking anywhere on its head", async () => {
    vi.mocked(api.accountStatus).mockResolvedValue([epic()]);
    renderAccounts();
    const toggle = await screen.findByRole("button", { name: "Epic Games settings" });
    await fireEvent.click(screen.getByText("Signed in as Ada"));
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    // Buttons on the head keep their own action.
    await fireEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    await fireEvent.click(screen.getByText("Epic Games"));
    expect(toggle).toHaveAttribute("aria-expanded", "true");
  });

  it("has no chevron, but keeps its place, when a row has nothing to show", async () => {
    vi.mocked(api.accountStatus).mockResolvedValue([
      {
        ...epic({ signedIn: false, displayName: null }),
        provider: "battlenet",
        name: "Battle.net",
      },
    ]);
    renderAccounts();
    const row = await screen.findByRole("group", { name: "Battle.net" });
    expect(screen.queryByRole("button", { name: "Battle.net settings" })).toBeNull();
    expect(row.querySelector(".account__toggle")).not.toBeNull();
    expect(screen.getByRole("button", { name: "Sign in to Battle.net" })).toBeInTheDocument();
  });

  it("offers Steam either a sign-in or a Web API key", async () => {
    vi.mocked(api.accountStatus).mockResolvedValue([steam()]);
    vi.mocked(api.getSettings).mockResolvedValue(
      makeSettings({ providers: { epic: { token: "keep-me" } } }),
    );
    vi.mocked(api.saveSettings).mockResolvedValue(undefined);
    vi.mocked(api.scanLibrary).mockResolvedValue({ added: 1, updated: 0, uninstalled: 0 });
    renderAccounts();
    await fireEvent.click(await screen.findByRole("button", { name: "Steam settings" }));
    await fireEvent.click(
      await screen.findByRole("button", { name: /Use a Steam Web API key instead/ }),
    );
    expect(screen.getByRole("button", { name: "Save Steam key" })).toBeDisabled();
    await fireEvent.input(screen.getByLabelText("Steam Web API key"), {
      target: { value: " KEY123 " },
    });
    await fireEvent.click(screen.getByRole("button", { name: "Save Steam key" }));
    await waitFor(() =>
      expect(api.saveSettings).toHaveBeenCalledWith(
        makeSettings({
          providers: { epic: { token: "keep-me" }, steam: { apiKey: "KEY123" } },
        }),
      ),
    );
    // With a key saved, the row counts as connected and offers removing it, not signing in.
    expect(await screen.findByText("Using a Steam Web API key")).toBeInTheDocument();
    expect(screen.getByText("1 of 1 account connected")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Sign in to Steam" })).toBeNull();
    expect(screen.queryByRole("button", { name: /Use a Steam Web API key instead/ })).toBeNull();
    await fireEvent.click(screen.getByRole("button", { name: "Remove key" }));
    await waitFor(() => expect(toasts()).toContain("Steam key removed"));
    expect(await screen.findByRole("button", { name: "Sign in to Steam" })).toBeInTheDocument();
  });
});

function makeSettings(overrides: Partial<Settings> = {}): Settings {
  return {
    scanOnStartup: true,
    downloadCovers: true,
    fetchMetadata: true,
    steamGridDbApiKey: null,
    igdbClientId: null,
    igdbClientSecret: null,
    closeToTray: true,
    minimizeToTray: false,
    startWithSystem: false,
    startMinimized: true,
    providers: { epic: { token: "keep-me" }, steam: { apiKey: "K" } },
    ...overrides,
  };
}

function ArtworkHarness() {
  const model = createSettingsModel();
  return <ArtworkSection model={model} />;
}

describe("ArtworkSection", () => {
  beforeEach(() => {
    vi.mocked(api.saveSettings).mockResolvedValue(undefined);
  });

  it("saves the key and keeps everything else", async () => {
    vi.mocked(api.getSettings).mockResolvedValue(makeSettings());
    render(() => <ArtworkHarness />);
    const input = await screen.findByLabelText("SteamGridDB API key");
    expect(input).toHaveAttribute("type", "password");
    expect(screen.getByText(/credential store/)).toBeInTheDocument();
    await fireEvent.input(input, { target: { value: "  sgdb  " } });
    await fireEvent.click(screen.getByRole("button", { name: "Save artwork settings" }));
    await waitFor(() =>
      expect(api.saveSettings).toHaveBeenCalledWith(makeSettings({ steamGridDbApiKey: "sgdb" })),
    );
  });

  it("saves an empty input as null", async () => {
    vi.mocked(api.getSettings).mockResolvedValue(makeSettings({ steamGridDbApiKey: "old" }));
    render(() => <ArtworkHarness />);
    const input = await screen.findByLabelText("SteamGridDB API key");
    await waitFor(() => expect(input).toHaveValue("old"));
    await fireEvent.input(input, { target: { value: "" } });
    await fireEvent.click(screen.getByRole("button", { name: "Save artwork settings" }));
    await waitFor(() =>
      expect(api.saveSettings).toHaveBeenCalledWith(makeSettings({ steamGridDbApiKey: null })),
    );
  });

  it("reveals the key on request and opens the key page", async () => {
    vi.mocked(api.getSettings).mockResolvedValue(makeSettings());
    render(() => <ArtworkHarness />);
    const input = await screen.findByLabelText("SteamGridDB API key");
    await fireEvent.click(screen.getByRole("button", { name: "Show SteamGridDB key" }));
    expect(input).toHaveAttribute("type", "text");
    await fireEvent.click(
      screen.getByRole("button", { name: "https://www.steamgriddb.com/profile/preferences/api" }),
    );
    expect(openUrl).toHaveBeenCalledWith("https://www.steamgriddb.com/profile/preferences/api");
  });
});

describe("UpdatesSection", () => {
  it("reports an up-to-date app", async () => {
    vi.mocked(api.checkUpdate).mockResolvedValue(null);
    render(() => <UpdatesSection />);
    await fireEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    expect(await screen.findByText("You're up to date.")).toBeInTheDocument();
  });

  it("offers an available update with notes and installs after confirming", async () => {
    vi.mocked(api.checkUpdate).mockResolvedValue({
      version: "1.2.0",
      notes: "Line one\nLine two",
      date: null,
    });
    vi.mocked(api.installUpdate).mockResolvedValue(undefined);
    render(() => <UpdatesSection />);
    await fireEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    expect(await screen.findByText("Version 1.2.0 is available")).toBeInTheDocument();
    expect(screen.getByText(/Line one/)).toBeInTheDocument();
    await fireEvent.click(screen.getByRole("button", { name: /Install and restart/ }));
    expect(api.installUpdate).not.toHaveBeenCalled();
    await fireEvent.click(screen.getByRole("button", { name: "Install and restart now" }));
    await waitFor(() => expect(api.installUpdate).toHaveBeenCalledTimes(1));
  });

  it("shows check failures", async () => {
    vi.mocked(api.checkUpdate).mockRejectedValue("No network");
    render(() => <UpdatesSection />);
    await fireEvent.click(screen.getByRole("button", { name: "Check for updates" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("No network");
  });
});
