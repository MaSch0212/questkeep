import { describe, expect, it } from "vitest";
import { makeGame, makeScript, RUNTIMES, SCRIPT_VARIABLES } from "../test-utils";
import { defaultQuery } from "./query";
import { blankScript, cleanInput, duplicateOf, scriptIssue } from "./scriptDraft";
import {
  actionScripts,
  appliedRows,
  commandPreview,
  effectiveRuntime,
  expandPlaceholders,
  filterSummary,
  groupVariables,
  insertionText,
  moveBy,
  moveItem,
  ownSummary,
  runsText,
  runtimeForPath,
  sampleGame,
  scopeText,
  slotKey,
  splitActions,
} from "./scripts";

const variable = (name: string) => {
  const found = SCRIPT_VARIABLES.find((v) => v.env === name);
  if (!found) throw new Error(name);
  return found;
};

describe("expandPlaceholders", () => {
  it("replaces known placeholders case-insensitively and leaves unknown ones", () => {
    expect(expandPlaceholders('"{InstallDir}\\saves" {gamename} {Nope}', SCRIPT_VARIABLES)).toBe(
      '"C:\\Games\\Hades\\saves" Hades {Nope}',
    );
  });
  it("leaves text without placeholders alone", () => {
    expect(expandPlaceholders("-Verbose", SCRIPT_VARIABLES)).toBe("-Verbose");
  });
});

describe("insertionText", () => {
  const v = variable("QK_GAME_NAME");
  it("uses the runtime's syntax in the editor", () => {
    expect(insertionText(v, "env", "pwsh", "editor")).toBe("$env:QK_GAME_NAME");
    expect(insertionText(v, "env", "windowsPowershell", "editor")).toBe("$env:QK_GAME_NAME");
    expect(insertionText(v, "env", "bash", "editor")).toBe("$QK_GAME_NAME");
    expect(insertionText(v, "env", "cmd", "editor")).toBe("%QK_GAME_NAME%");
    expect(insertionText(v, "env", "python", "editor")).toBe('os.environ["QK_GAME_NAME"]');
  });
  it("inserts a placeholder in fields and in placeholder syntax", () => {
    expect(insertionText(v, "env", "bash", "field")).toBe("{GameName}");
    expect(insertionText(v, "placeholder", "bash", "editor")).toBe("{GameName}");
  });
});

describe("groupVariables", () => {
  it("groups in panel order and filters by name or value", () => {
    expect(groupVariables(SCRIPT_VARIABLES, "").map((g) => g.group)).toEqual([
      "context",
      "game",
      "lists",
    ]);
    const found = groupVariables(SCRIPT_VARIABLES, "roguelike");
    expect(found).toHaveLength(1);
    expect(found[0]?.items[0]?.env).toBe("QK_GENRES");
    expect(groupVariables(SCRIPT_VARIABLES, "zzz")).toEqual([]);
  });
});

describe("commandPreview", () => {
  it("expands arguments and defaults the working folder to the install folder", () => {
    expect(
      commandPreview(
        { program: "C:\\Windows\\explorer.exe", args: '"{InstallDir}\\saves"', workingDir: null },
        SCRIPT_VARIABLES,
      ),
    ).toEqual({
      command: 'C:\\Windows\\explorer.exe "C:\\Games\\Hades\\saves"',
      workingDir: "C:\\Games\\Hades",
    });
  });
  it("quotes a program path with spaces and expands the working folder", () => {
    expect(
      commandPreview(
        { program: "C:\\Program Files\\x.exe", args: "", workingDir: "{InstallDir}\\bin" },
        SCRIPT_VARIABLES,
      ),
    ).toEqual({ command: '"C:\\Program Files\\x.exe"', workingDir: "C:\\Games\\Hades\\bin" });
  });
});

describe("runtimes", () => {
  it("picks the runtime from the extension among installed ones", () => {
    expect(runtimeForPath("C:\\a\\x.ps1", RUNTIMES)).toBe("windowsPowershell");
    expect(runtimeForPath("/a/x.sh", RUNTIMES)).toBe("bash");
    expect(runtimeForPath("x.PY", RUNTIMES)).toBe("python");
    expect(runtimeForPath("x.cmd", RUNTIMES)).toBe("cmd"); // not installed: running says so
    expect(runtimeForPath("x.txt", RUNTIMES)).toBe("windowsPowershell");
    expect(runtimeForPath("x.txt", RUNTIMES, false)).toBeNull();
    expect(runtimeForPath("x.sh", [])).toBe("bash");
  });
  it("prefers PowerShell 7 only when 5.1 is missing", () => {
    const pwshOnly = [{ id: "pwsh" as const, name: "PowerShell 7", path: "", extension: "ps1" }];
    expect(runtimeForPath("x.ps1", pwshOnly)).toBe("pwsh");
  });
  it("uses a draft's own runtime when installed", () => {
    const base = { source: "inline" as const, program: "", runtime: "bash" as const };
    expect(effectiveRuntime(base, RUNTIMES)).toBe("bash");
    expect(effectiveRuntime({ ...base, runtime: "cmd" }, RUNTIMES)).toBe("windowsPowershell");
    expect(effectiveRuntime({ ...base, source: "program" }, RUNTIMES)).toBeNull();
  });
});

describe("moving", () => {
  it("moves an item to another place", () => {
    expect(moveItem(["a", "b", "c"], 0, 2)).toEqual(["b", "c", "a"]);
    expect(moveItem(["a", "b", "c"], 2, 0)).toEqual(["c", "a", "b"]);
  });
  it("ignores impossible moves", () => {
    expect(moveBy(["a", "b"], 0, -1)).toEqual(["a", "b"]);
    expect(moveBy(["a", "b"], 1, 1)).toEqual(["a", "b"]);
    expect(moveBy(["a", "b", "c"], 1, 1)).toEqual(["a", "c", "b"]);
  });
  it("keys slots", () => {
    expect(slotKey({ type: "gameScripts" })).toBe("gameScripts");
    expect(slotKey({ type: "script", id: "x" })).toBe("x");
  });
});

const name = (_field: string, key: string) => key;

describe("summaries", () => {
  it("summarizes a filter with the first two parts and a count of the rest", () => {
    expect(filterSummary(defaultQuery(), name)).toBeNull();
    expect(filterSummary({ ...defaultQuery(), installed: "yes" }, name)).toBe("Installed");
    expect(
      filterSummary(
        { ...defaultQuery(), installed: "yes", favorite: "yes", played: "played" },
        name,
      ),
    ).toBe("Installed, Favorite +1");
  });
  it("says where a script applies", () => {
    expect(scopeText({ filter: null, gameId: null }, null)).toBe("All games");
    expect(scopeText({ filter: defaultQuery(), gameId: null }, "Installed")).toBe(
      "Filter: Installed",
    );
    expect(scopeText({ filter: null, gameId: "g1" }, null)).toBe("This game");
  });
  it("says what a script runs", () => {
    expect(runsText(makeScript(), RUNTIMES)).toBe(
      "Windows PowerShell 5.1 · inline script · before start",
    );
    expect(
      runsText(
        makeScript({
          source: "linked",
          runtime: "python",
          program: "C:\\t\\show.py",
          timing: "onceRunning",
        }),
        RUNTIMES,
      ),
    ).toBe("Python 3.12 · C:\\t\\show.py (linked) · once running");
    expect(
      runsText(
        makeScript({
          kind: "action",
          source: "program",
          program: "explorer.exe",
          args: '"{InstallDir}"',
        }),
        RUNTIMES,
      ),
    ).toBe('explorer.exe "{InstallDir}"');
    expect(ownSummary(makeScript({ kind: "post" }), RUNTIMES)).toBe(
      "Post · Windows PowerShell 5.1 · inline script",
    );
  });
});

describe("actions", () => {
  const list = ["a", "b", "c"].map((id) => makeScript({ id, kind: "action" }));
  it("keeps enabled actions only", () => {
    const applied = [
      makeScript({ id: "p", kind: "pre" }),
      makeScript({ id: "off", kind: "action", enabled: false }),
      ...list,
    ];
    expect(actionScripts(applied).map((s) => s.id)).toEqual(["a", "b", "c"]);
  });
  it("shows two as buttons and the rest under More", () => {
    const { shown, more } = splitActions(list);
    expect(shown.map((s) => s.id)).toEqual(["a", "b"]);
    expect(more.map((s) => s.id)).toEqual(["c"]);
    expect(splitActions(list.slice(0, 1)).more).toEqual([]);
  });
});

describe("script drafts", () => {
  it("starts blank per kind", () => {
    const action = blankScript("action", "g1");
    expect(action).toMatchObject({ id: null, gameId: "g1", icon: "zap" });
    expect(action.options.wait).toBe(false);
    expect(blankScript("pre").options).toMatchObject({ wait: true, timeoutSeconds: 60 });
  });
  it("names what stops a save", () => {
    const draft = blankScript("pre");
    expect(scriptIssue(draft, RUNTIMES)).toBe("Give the script a name.");
    expect(scriptIssue({ ...draft, name: "x" }, RUNTIMES)).toBe("Choose a program.");
    expect(scriptIssue({ ...draft, name: "x", program: "a.exe" }, RUNTIMES)).toBeNull();
    expect(scriptIssue({ ...draft, name: "x", source: "linked" }, RUNTIMES)).toBe(
      "Choose a script file.",
    );
    expect(scriptIssue({ ...draft, name: "x", source: "inline" }, [])).toBe("Choose a runtime.");
    expect(
      scriptIssue({ ...draft, name: "x", program: "a.exe", filter: defaultQuery() }, RUNTIMES),
    ).toBe("Choose at least one filter.");
  });
  it("sends only the fields of the chosen kind and source", () => {
    const input = cleanInput(
      {
        ...blankScript("pre"),
        name: " Hi ",
        source: "inline",
        program: "stale.exe",
        content: "echo",
        icon: "zap",
        workingDir: "  ",
        timing: "onceRunning",
      },
      RUNTIMES,
    );
    expect(input).toMatchObject({
      name: "Hi",
      program: "",
      runtime: "windowsPowershell",
      content: "echo",
      icon: null,
      workingDir: null,
      timing: "onceRunning",
    });
    const action = cleanInput(
      {
        ...blankScript("action"),
        name: "A",
        program: "a.exe",
        content: "old",
        timing: "onceRunning",
      },
      RUNTIMES,
    );
    expect(action).toMatchObject({
      content: null,
      runtime: null,
      timing: "beforeStart",
      icon: "zap",
    });
  });
  it("duplicates as a new script", () => {
    expect(duplicateOf(makeScript(), "Copy")).toMatchObject({ id: null, name: "Copy" });
  });
});

const own = (id: string, kind: "pre" | "post" | "action") =>
  makeScript({ id, kind, name: id, gameId: "g1" });

describe("appliedRows", () => {
  it("folds the game's own scripts that run in a row into one line, per kind", () => {
    const rows = appliedRows([
      makeScript({ id: "g-pre", kind: "pre", name: "g-pre" }),
      own("o1", "pre"),
      own("o2", "pre"),
      makeScript({ id: "g-pre2", kind: "pre", name: "g-pre2" }),
      makeScript({ id: "g-post", kind: "post", name: "g-post" }),
      own("o3", "action"),
    ]);
    expect(
      rows.map((r) => [r.kind, r.script?.name ?? r.own?.map((s) => s.name).join("+")]),
    ).toEqual([
      ["pre", "g-pre"],
      ["pre", "o1+o2"],
      ["pre", "g-pre2"],
      ["post", "g-post"],
      ["action", "o3"],
    ]);
  });
  it("lists pre before post before actions whatever the order given", () => {
    const rows = appliedRows([
      makeScript({ id: "a", kind: "action" }),
      makeScript({ id: "p", kind: "post" }),
      makeScript({ id: "r", kind: "pre" }),
    ]);
    expect(rows.map((r) => r.kind)).toEqual(["pre", "post", "action"]);
  });
});

describe("sampleGame", () => {
  const games = [
    makeGame({ id: "a", lastPlayed: null }),
    makeGame({ id: "b", lastPlayed: "2026-01-01T00:00:00Z" }),
    makeGame({ id: "c", lastPlayed: "2026-03-01T00:00:00Z" }),
  ];
  it("is the edited game when there is one", () => {
    expect(sampleGame(games, "a")?.id).toBe("a");
    expect(sampleGame(games, "zzz")).toBeUndefined();
  });
  it("is the game played last, else the first one", () => {
    expect(sampleGame(games, null)?.id).toBe("c");
    expect(sampleGame([games[0] as (typeof games)[number]], null)?.id).toBe("a");
    expect(sampleGame([], null)).toBeUndefined();
  });
});
