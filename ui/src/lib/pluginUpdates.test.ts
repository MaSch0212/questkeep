import { describe, expect, it } from "vitest";
import type { PluginChanges } from "../bindings/PluginChanges";
import type { PluginUpdate } from "../bindings/PluginUpdate";
import {
  changeKinds,
  changeValues,
  countOutcomes,
  hasExtension,
  planBatch,
  sortUpdates,
  versionChange,
  type Prepared,
  type UpdateOutcome,
} from "./pluginUpdates";

const NONE: PluginChanges = {
  newHttp: [],
  newRead: [],
  newFolders: [],
  addsFilesystem: false,
  addsRegistry: false,
  newLauncher: [],
  newLogin: [],
  addsExtension: false,
  addsStore: false,
  needsReview: false,
};

const update = (id: string, name = id): PluginUpdate => ({
  id,
  name,
  installedVersion: "0.1.0",
  latestVersion: "0.2.0",
  notes: null,
});

function prepared(id: string, changes: Partial<PluginChanges>, extension = false): Prepared {
  return {
    update: update(id),
    // Only the fields the helpers read.
    preview: {
      changes: { ...NONE, ...changes },
      preview: { hasExtension: extension },
    } as unknown as Prepared["preview"],
  };
}

describe("changeKinds", () => {
  it("is empty for an update that asks for nothing new", () => {
    expect(changeKinds(NONE)).toEqual([]);
  });

  it("lists every kind in display order", () => {
    const all: PluginChanges = {
      newHttp: ["api.example.com"],
      newRead: ["~/games"],
      newFolders: [{ key: "inst", label: "Instances", hint: null, default: null }],
      addsFilesystem: true,
      addsRegistry: true,
      newLauncher: ["steam.exe"],
      newLogin: ["example.com"],
      addsExtension: true,
      addsStore: true,
      needsReview: true,
    };
    expect(changeKinds(all)).toEqual([
      "filesystem",
      "registry",
      "http",
      "login",
      "launcher",
      "read",
      "folders",
      "extension",
      "store",
    ]);
    expect(changeValues(all, "login")).toEqual(["example.com"]);
    expect(changeValues(all, "launcher")).toEqual(["steam.exe"]);
    expect(changeValues(all, "http")).toEqual(["api.example.com"]);
    expect(changeValues(all, "read")).toEqual(["~/games"]);
    expect(changeValues(all, "folders")).toEqual(["Instances"]);
    expect(changeValues(all, "extension")).toEqual([]);
  });
});

describe("planBatch", () => {
  it("keeps updates that ask for more out of the batch", () => {
    const plan = planBatch([
      prepared("a", {}),
      prepared("b", { newHttp: ["x.com"], needsReview: true }),
      prepared("c", {}),
    ]);
    expect(plan.ready.map((p) => p.update.id)).toEqual(["a", "c"]);
    expect(plan.review.map((p) => p.update.id)).toEqual(["b"]);
  });

  it("notices an extension among the ready ones", () => {
    expect(hasExtension([prepared("a", {})])).toBe(false);
    expect(hasExtension([prepared("a", {}), prepared("b", {}, true)])).toBe(true);
  });
});

const outcome = (status: UpdateOutcome["status"]): UpdateOutcome => ({
  id: status,
  name: status,
  versions: "",
  version: "",
  status,
  message: null,
});

describe("formatting and counting", () => {
  it("writes the version change with an arrow", () => {
    expect(versionChange(update("a"))).toBe("0.1.0 → 0.2.0");
  });

  it("counts outcomes by status", () => {
    expect(countOutcomes([outcome("updated"), outcome("failed"), outcome("updated")])).toEqual({
      updated: 2,
      failed: 1,
      review: 0,
    });
  });

  it("sorts updates by name, then id", () => {
    const sorted = sortUpdates([update("z", "beta"), update("y", "Alpha"), update("a", "beta")]);
    expect(sorted.map((u) => u.id)).toEqual(["y", "a", "z"]);
  });
});
