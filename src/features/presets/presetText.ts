import type { CodecDefaults, Preset, UserDefaults } from "@bindings";
import { defaultsFor, effective, settingsSummary } from "../../lib/codecs";
import { FORMAT_LABELS } from "../../lib/formats";
import {
  FACTORY_DESTINATION,
  FACTORY_METADATA,
  destinationLabel,
  metadataLabel,
} from "../../lib/policies";

/** "JPEG XL lossless + WebP": a preset's outputs, briefly. */
export function outputsLine(preset: Preset): string {
  if (preset.outputs.length === 0) return "No outputs yet";
  return preset.outputs
    .map((output) => {
      const lossless = (output.codec as { lossless?: boolean }).lossless === true;
      return `${FORMAT_LABELS[output.codec.format]}${lossless ? " lossless" : ""}`;
    })
    .join(" + ");
}

/** One line about a stored preset, for the preset menu. */
export function presetLine(preset: Preset, defaults: UserDefaults): string {
  const destination = destinationLabel(preset.destination ?? defaults.destination ?? FACTORY_DESTINATION);
  const metadata = metadataLabel(preset.metadata ?? defaults.metadata ?? FACTORY_METADATA).toLowerCase();
  return `${outputsLine(preset)} · ${destination.charAt(0).toLowerCase()}${destination.slice(1)} · ${metadata} metadata`;
}

/** Each output with its full settings, for the save dialog. */
export function outputDetails(preset: Preset, codecs: CodecDefaults[]): string[] {
  return preset.outputs.map((output) => {
    const codec = defaultsFor(codecs, output.codec.format);
    const summary = codec ? settingsSummary(effective(codec.inherited, output.codec)) : "";
    return `${FORMAT_LABELS[output.codec.format]}: ${summary}`;
  });
}
