import { describe, expect, it } from "vitest";
import { formatBytes, formatDuration, plural } from "./units";

describe("units", () => {
  it("formats sizes in decimal units", () => {
    expect(formatBytes(999)).toBe("999 B");
    expect(formatBytes(1_400_000)).toBe("1.4 MB");
    expect(formatBytes(240_000_000)).toBe("240 MB");
  });

  it("formats durations at a readable precision", () => {
    expect(formatDuration(300)).toBe("under 1 s");
    expect(formatDuration(4_400)).toBe("4 s");
    expect(formatDuration(192_000)).toBe("3 min 12 s");
    expect(formatDuration(3_900_000)).toBe("1 h 05 min");
  });

  it("pluralises", () => {
    expect(plural(1, "image")).toBe("1 image");
    expect(plural(1200, "image")).toBe(`${(1200).toLocaleString()} images`);
  });
});
