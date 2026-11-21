import { describe, expect, it } from "vitest";
import {
  fromLocalInput,
  minutesToSeconds,
  pageCount,
  secondsToMinutes,
  toLocalInput,
} from "./sessions";

describe("sessions", () => {
  it("round-trips a timestamp through a local input value", () => {
    const iso = new Date(2026, 10, 3, 18, 5).toISOString();
    expect(toLocalInput(iso)).toBe("2026-11-03T18:05");
    expect(fromLocalInput("2026-11-03T18:05")).toBe(iso);
    expect(fromLocalInput("")).toBeNull();
  });

  it("converts minutes", () => {
    expect(minutesToSeconds("90")).toBe(5400);
    expect(minutesToSeconds("1,5")).toBe(90);
    expect(minutesToSeconds("-1")).toBeNull();
    expect(minutesToSeconds("")).toBeNull();
    expect(secondsToMinutes(5400)).toBe("90");
    expect(secondsToMinutes(90)).toBe("1.5");
  });

  it("counts pages", () => {
    expect(pageCount(0, 15)).toBe(1);
    expect(pageCount(15, 15)).toBe(1);
    expect(pageCount(16, 15)).toBe(2);
    expect(pageCount(3000, 15)).toBe(200);
  });
});
