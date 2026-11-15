import { describe, expect, it } from "vitest";
import { closeKind, launcherClosePolicy, withLauncherClose } from "./launcherClose";

describe("launcher close", () => {
  it("defaults to never", () => {
    expect(launcherClosePolicy(null, "steam")).toBe("never");
    expect(launcherClosePolicy({ launcherClose: {} }, "steam")).toBe("never");
    expect(launcherClosePolicy({ launcherClose: { steam: "always" } }, "steam")).toBe("always");
  });

  it("stores a choice and drops the default", () => {
    const set = withLauncherClose({ epic: "always" }, "steam", "ifStartedByGame");
    expect(set).toEqual({ epic: "always", steam: "ifStartedByGame" });
    expect(withLauncherClose(set, "steam", "never")).toEqual({ epic: "always" });
    expect(withLauncherClose({}, "steam", "never")).toEqual({});
  });

  it("tells how a launcher is ended", () => {
    const action = {
      kind: "file",
      target: "steam.exe",
      args: "-shutdown",
      workingDir: null,
      tracking: "default",
      trackingPath: null,
    } as const;
    expect(closeKind({ command: action })).toBe("command");
    expect(closeKind({ closeWindows: { processNames: ["x.exe"] } })).toBe("closeWindows");
    expect(closeKind("forceQuit")).toBe("forceQuit");
  });
});
