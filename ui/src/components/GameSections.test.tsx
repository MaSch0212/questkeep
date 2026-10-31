import { render, screen } from "@solidjs/testing-library";
import { describe, expect, it, vi } from "vitest";
import { makeGame } from "../test-utils";
import {
  AchievementsCard,
  ActivityCard,
  NewsCard,
  ScoresCard,
  TimeToBeatCard,
} from "./GameSections";

vi.mock("../lib/api");
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn<() => void>(),
  convertFileSrc: (p: string) => p,
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn<() => void>() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn<() => void>() }));

describe("game page sections", () => {
  it("hide without data", () => {
    const game = makeGame();
    const { container } = render(() => (
      <>
        <ScoresCard game={game} />
        <TimeToBeatCard game={game} />
        <AchievementsCard achievements={null} />
        <ActivityCard activity={{ weeks: [{ start: "2026-10-26", seconds: 0 }], sessions: 0 }} />
        <NewsCard news={{ items: [], allUrl: null }} />
      </>
    ));
    expect(container.querySelectorAll("section")).toHaveLength(0);
  });

  it("show scores, time to beat with its source and achievements", () => {
    const game = makeGame({
      criticScore: { value: 91, source: "Metacritic" },
      timeToBeat: { main: 27 * 3600, extra: null, complete: 105 * 3600 },
      fieldSources: { timeToBeat: "hltb" },
      playtimeSeconds: 10 * 3600,
    });
    render(() => (
      <>
        <ScoresCard game={game} />
        <TimeToBeatCard game={game} />
        <AchievementsCard
          achievements={{
            total: 4,
            unlocked: [
              { name: "Rare", description: null, icon: null, unlockedAt: 1, percent: 2.5 },
            ],
          }}
        />
      </>
    ));
    expect(screen.getByRole("img", { name: "91 of 100" })).toBeTruthy();
    expect(screen.getByText("Metacritic")).toBeTruthy();
    expect(screen.getByText("27h")).toBeTruthy();
    expect(screen.getByText("From HowLongToBeat")).toBeTruthy();
    expect(screen.getByText("You · 10h")).toBeTruthy();
    expect(screen.getByText("1 of 4")).toBeTruthy();
    expect(screen.getByText(/Rarest you have · 2\.5% of players/)).toBeTruthy();
  });
});
