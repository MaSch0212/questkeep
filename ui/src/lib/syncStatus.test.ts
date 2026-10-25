import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  applySyncEvent,
  createSyncTracker,
  IDLE_SYNC,
  isSyncBusy,
  syncEntryText,
  syncLabel,
  syncResultText,
  type SyncEvent,
  type SyncState,
} from "./syncStatus";

const queued = (...providers: string[]): SyncEvent => ({ type: "scansQueued", providers });
const started = (provider: string): SyncEvent => ({ type: "scanStarted", provider });
const scanned = (provider: string, added = 0, updated = 0): SyncEvent => ({
  type: "libraryScanned",
  provider,
  summary: { added, updated, uninstalled: 0 },
});
const failed = (provider: string, message = "boom"): SyncEvent => ({
  type: "scanFailed",
  provider,
  message,
});

function run(events: SyncEvent[], from: SyncState = IDLE_SYNC): SyncState {
  return events.reduce(applySyncEvent, from);
}

describe("applySyncEvent", () => {
  it("queued stores wait, started ones run, then they are done or failed", () => {
    let state = run([queued("steam", "epic", "gog")]);
    expect(state.active).toBe(true);
    expect(state.entries.map((e) => e.status)).toEqual(["waiting", "waiting", "waiting"]);
    state = run([started("steam"), scanned("steam", 2, 1), started("epic"), failed("epic")], state);
    expect(state.entries.map((e) => [e.provider, e.status])).toEqual([
      ["steam", "done"],
      ["epic", "failed"],
      ["gog", "waiting"],
    ]);
    expect(state.entries[0]?.summary).toEqual({ added: 2, updated: 1, uninstalled: 0 });
    expect(state.entries[1]?.message).toBe("boom");
    expect(isSyncBusy(state)).toBe(true);
    state = run([started("gog"), scanned("gog")], state);
    expect(isSyncBusy(state)).toBe(false);
  });

  it("a single scan without a queue event is a run of one", () => {
    const state = run([started("steam")]);
    expect(state.entries).toEqual([{ provider: "steam", status: "running" }]);
    expect(syncLabel(state)).toBe("Syncing 1 of 1…");
  });

  it("starts a fresh run after the previous one ended", () => {
    const ended: SyncState = {
      entries: [{ provider: "steam", status: "done" }],
      active: false,
      finishedAt: 5,
    };
    const state = run([started("epic")], ended);
    expect(state.entries.map((e) => e.provider)).toEqual(["epic"]);
    expect(state.finishedAt).toBeNull();
  });

  it("queuing a store again within a run makes it wait again", () => {
    const state = run([started("steam"), scanned("steam"), queued("steam")]);
    expect(state.entries).toEqual([{ provider: "steam", status: "waiting" }]);
  });
});

describe("texts", () => {
  it("labels progress while busy", () => {
    let state = run([queued("a", "b", "c")]);
    expect(syncLabel(state)).toBe("Syncing 1 of 3…");
    state = run([started("a"), scanned("a"), started("b")], state);
    expect(syncLabel(state)).toBe("Syncing 2 of 3…");
    state = run([scanned("b"), started("c"), scanned("c")], state);
    expect(syncLabel(state)).toBe("Sync status");
    expect(syncLabel(IDLE_SYNC)).toBe("Sync status");
  });

  it("summarizes the run", () => {
    expect(syncResultText(run([scanned("a", 3, 1), scanned("b", 1, 0)]))).toBe(
      "Sync finished: 4 added, 1 updated",
    );
    expect(syncResultText(run([scanned("a"), scanned("b")]))).toBe("Sync finished: no changes");
    expect(syncResultText(run([scanned("a", 1, 2), failed("b"), failed("c")]))).toBe(
      "Sync finished: 1 added, 2 updated, 2 failed",
    );
    expect(syncResultText(run([failed("a")]))).toBe("Sync finished: no changes, 1 failed");
  });

  it("describes each store", () => {
    const state = run([queued("a", "b", "c", "d")]);
    const next = run([started("b"), scanned("c", 2, 5), failed("d", "No network")], state);
    expect(next.entries.map(syncEntryText)).toEqual([
      "Waiting",
      "Scanning…",
      "+2, ~5",
      "No network",
    ]);
  });
});

function setup() {
  const onFinished = vi.fn<(state: SyncState) => void>();
  const onChange = vi.fn<(state: SyncState) => void>();
  const tracker = createSyncTracker({ onChange, onFinished });
  return { tracker, onFinished, onChange };
}

describe("createSyncTracker", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("ignores events that are not about scans", () => {
    const { tracker, onChange } = setup();
    expect(tracker.handle({ type: "gamesChanged", ids: [] })).toBe(false);
    expect(onChange).not.toHaveBeenCalled();
  });

  it("ends the run 1.5 s after the last store finished, once", () => {
    const { tracker, onFinished } = setup();
    tracker.handle(queued("steam", "epic"));
    tracker.handle(started("steam"));
    tracker.handle(scanned("steam", 1));
    vi.advanceTimersByTime(5000);
    expect(onFinished).not.toHaveBeenCalled(); // epic is still waiting
    tracker.handle(started("epic"));
    tracker.handle(scanned("epic", 0, 2));
    vi.advanceTimersByTime(1499);
    expect(onFinished).not.toHaveBeenCalled();
    expect(tracker.state().active).toBe(true);
    vi.advanceTimersByTime(1);
    expect(onFinished).toHaveBeenCalledTimes(1);
    const final = onFinished.mock.calls[0]?.[0];
    expect(syncResultText(final ?? IDLE_SYNC)).toBe("Sync finished: 1 added, 2 updated");
    expect(tracker.state().active).toBe(false);
    expect(tracker.state().finishedAt).not.toBeNull();
    vi.advanceTimersByTime(10_000);
    expect(onFinished).toHaveBeenCalledTimes(1);
  });

  it("joins back-to-back single scans into one run", () => {
    const { tracker, onFinished } = setup();
    tracker.handle(started("steam"));
    tracker.handle(scanned("steam", 1));
    vi.advanceTimersByTime(1000);
    tracker.handle(started("epic"));
    tracker.handle(scanned("epic", 1));
    vi.advanceTimersByTime(1000);
    expect(onFinished).not.toHaveBeenCalled();
    vi.advanceTimersByTime(500);
    expect(onFinished).toHaveBeenCalledTimes(1);
    expect(onFinished.mock.calls[0]?.[0].entries).toHaveLength(2);
  });

  it("a scan after the run ended is a new run with its own message", () => {
    const { tracker, onFinished } = setup();
    tracker.handle(started("steam"));
    tracker.handle(scanned("steam"));
    vi.advanceTimersByTime(1500);
    tracker.handle(started("steam"));
    tracker.handle(failed("steam"));
    vi.advanceTimersByTime(1500);
    expect(onFinished).toHaveBeenCalledTimes(2);
    expect(syncResultText(onFinished.mock.calls[1]?.[0] ?? IDLE_SYNC)).toBe(
      "Sync finished: no changes, 1 failed",
    );
  });

  it("reset cancels a pending end", () => {
    const { tracker, onFinished } = setup();
    tracker.handle(started("steam"));
    tracker.handle(scanned("steam"));
    tracker.reset();
    vi.advanceTimersByTime(5000);
    expect(onFinished).not.toHaveBeenCalled();
    expect(tracker.state()).toEqual(IDLE_SYNC);
  });
});
