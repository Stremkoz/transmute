import type { CodecSettings } from "@bindings";
import { describe, expect, it } from "vitest";
import {
  CODEC_FIELDS,
  chosenSettings,
  effective,
  isDefaultPatch,
  isSet,
  losslessChoiceText,
  modeOf,
  settingsSummary,
  withField,
} from "./codecs";
import { namePartProblem, previewName } from "./policies";

const JXL: CodecSettings = { format: "jxl", lossless: false, distance: 1.5, reconstructJpeg: true };

const WEBP: CodecSettings = { format: "webp", lossless: false, autoLossless: false, quality: 85, method: 4 };

describe("WebP's Auto mode", () => {
  const mode = CODEC_FIELDS.webp[0]!;

  it("is a third mode over two fields", () => {
    const auto = withField({ format: "webp" }, "lossless", "auto");
    expect(auto).toEqual({ format: "webp", lossless: false, autoLossless: true });
    expect(modeOf(effective(WEBP, auto), mode)).toBe("auto");
    expect(settingsSummary(effective(WEBP, auto))).toBe("Auto lossless · quality 85 · method 4");

    // Lossless overrides Auto; Lossy has to turn an inherited Auto off.
    expect(withField(auto, "lossless", true)).toEqual({ format: "webp", lossless: true });
    expect(withField(auto, "lossless", false)).toEqual({ format: "webp", lossless: false, autoLossless: false });
    expect(modeOf(effective({ ...WEBP, autoLossless: true }, { format: "webp", lossless: true }), mode)).toBe("lossless");

    // Going back to the default clears both, and either one counts as set.
    expect(withField(auto, "lossless", undefined)).toEqual({ format: "webp" });
    expect(isSet({ format: "webp", autoLossless: true }, "lossless")).toBe(true);
  });

  it("says what it kept", () => {
    expect(losslessChoiceText({ keptLossless: true, lossyBytes: 1000, losslessBytes: 1060 })).toBe(
      "Auto kept lossless: 6% larger than lossy",
    );
    expect(losslessChoiceText({ keptLossless: true, lossyBytes: 1000, losslessBytes: 800 })).toBe(
      "Auto kept lossless: 20% smaller than lossy",
    );
    expect(losslessChoiceText({ keptLossless: false, lossyBytes: 1000, losslessBytes: 5300 })).toBe(
      "Auto kept lossy: lossless was 5.3× the size",
    );
    const auto = { ...WEBP, autoLossless: true };
    expect(settingsSummary(chosenSettings(auto, { keptLossless: true, lossyBytes: 1, losslessBytes: 1 }))).toBe(
      "Lossless · method 4",
    );
    expect(settingsSummary(chosenSettings(auto, null))).toBe("Auto lossless · quality 85 · method 4");
  });
});

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
