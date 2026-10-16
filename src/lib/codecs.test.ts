import type { CodecSettings } from "@bindings";
import { describe, expect, it } from "vitest";
import { effective, isDefaultPatch, settingsSummary, withField } from "./codecs";
import { namePartProblem, previewName } from "./policies";

const JXL: CodecSettings = { format: "jxl", lossless: false, distance: 1.5, reconstructJpeg: true };

describe("codec settings", () => {
  it("layers an output's patch over what it inherits", () => {
    const settings = effective(JXL, { format: "jxl", distance: 1 });
    expect(settings).toMatchObject({ lossless: false, distance: 1, reconstructJpeg: true });
    expect(settingsSummary(settings)).toBe("Lossy · distance 1 · effort automatic");
  });

  it("treats distance 0 as lossless, keeping a usable distance", () => {
    const patch = withField({ format: "jxl" }, "distance", 0);
    expect(patch).toEqual({ format: "jxl", lossless: true });
    expect(effective(JXL, patch)).toMatchObject({ lossless: true, distance: 1.5 });
    expect(settingsSummary(effective(JXL, patch))).toBe("Lossless · effort automatic · keep JPEGs exactly");
  });

  it("removes a field to inherit it again", () => {
    const patch = withField({ format: "webp", quality: 70 }, "quality", undefined);
    expect(patch).toEqual({ format: "webp" });
    expect(isDefaultPatch(patch)).toBe(true);
  });

  it("mentions a JPEG background only when it isn't white", () => {
    const jpeg: CodecSettings = { format: "jpeg", quality: 85, progressive: true, background: "#ffffff" };
    expect(settingsSummary(effective(jpeg, { format: "jpeg" }))).toBe("Quality 85 · progressive");
    expect(settingsSummary(effective(jpeg, { format: "jpeg", background: "#000000" }))).toBe(
      "Quality 85 · progressive · background #000000",
    );
  });
});

describe("file names", () => {
  const rule = { prefix: "web_", suffix: "_{date}", dateStyle: "iso" as const, dateSource: "batchStart" as const };

  it("previews a rule with sample values", () => {
    expect(previewName(rule, "beach", "jpg", "webp")).toBe("web_beach_2026-10-15.webp");
    expect(previewName(undefined, "beach", "jpg", "jxl")).toBe("beach.jxl");
    expect(previewName({ ...rule, prefix: "{{x}}", suffix: "" }, "a", "png", "avif")).toBe("{x}a.avif");
  });

  it("rejects what the backend would", () => {
    expect(namePartProblem("_{date}")).toBeNull();
    expect(namePartProblem("a/b")).toMatch(/can't contain/);
    expect(namePartProblem("{when}")).toMatch(/Unknown token/);
    expect(namePartProblem("_{da")).toMatch(/Braces/);
    expect(namePartProblem("{{literal}}")).toBeNull();
  });
});
