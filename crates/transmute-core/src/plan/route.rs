//! Choosing which tool(s) convert a source to a target format.
//!
//! Preference, in order:
//! 1. One step over two (via a temporary PNG).
//! 2. Routes whose writer can apply the metadata policy natively.
//! 3. Routes whose writer can apply every relevant codec setting.
//! 4. The format's specialist tool (cjxl for JPEG XL, cwebp for WebP, …).
//!
//! Every choice records why, and every compromise becomes a [`PlanNote`].

use crate::capability::{Capabilities, MetadataSupport, Toolbox};
use crate::codec::CodecSettings;
use crate::format::Format;
use crate::inspect::{Animation, MetadataPresence, Presence};
use crate::output::MetadataPolicy;
use crate::tool::ToolId;

use super::PlanNote;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Route {
    pub steps: Vec<Step>,
    /// Human-readable explanation of the choice.
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    pub tool: ToolId,
    pub from: Format,
    pub to: Format,
    /// Carry every frame through this step.
    pub animated: bool,
    /// The input is animated but the output can't be: take only the first
    /// frame, explicitly (some tools would otherwise write one file per frame).
    pub first_frame: bool,
    /// Restore the original JPEG stored in a recompressed JPEG XL, instead of
    /// encoding the pixels again.
    pub restore_jpeg: bool,
}

/// What a route must achieve, beyond the format change.
pub struct Requirements<'a> {
    pub from: Format,
    pub to: Format,
    pub animation: Animation,
    pub metadata: MetadataPresence,
    pub policy: MetadataPolicy,
    pub settings: &'a CodecSettings,
    /// The source is a JPEG XL holding a losslessly recompressed JPEG.
    pub jpeg_reconstruction: bool,
    /// Bits per channel of the source, when known.
    pub bit_depth: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Routed {
    pub route: Route,
    pub notes: Vec<PlanNote>,
}

/// Formats that can store an animation.
pub fn can_animate(format: Format) -> bool {
    matches!(
        format,
        Format::Gif | Format::Webp | Format::Png | Format::Jxl | Format::Avif
    )
}

/// Writers to try for `to`, best first. Decoders such as djxl and dwebp are
/// listed first where they apply: they only read their own format, so they
/// are only chosen for that input.
fn writer_preference(to: Format, animated: bool) -> &'static [ToolId] {
    use ToolId::*;
    match (to, animated) {
        (Format::Jxl, _) => &[Cjxl, Vips, Magick, Ffmpeg],
        (Format::Webp, false) => &[Cwebp, Vips, Magick, Ffmpeg],
        (Format::Webp, true) => &[Gif2webp, Vips, Magick, Ffmpeg],
        (Format::Avif | Format::Heif, _) => &[Vips, Magick],
        (Format::Jpeg, _) => &[Djxl, Vips, Magick, Ffmpeg],
        (Format::Png, false) => &[Djxl, Dwebp, Vips, Magick, Ffmpeg],
        // ImageMagick writes APNG through ffmpeg and resamples the frame
        // timing (found in M3), so it isn't used for animated PNG at all.
        (Format::Png, true) => &[Djxl, Ffmpeg],
        (Format::Tiff, _) => &[Dwebp, Vips, Magick, Ffmpeg],
        (Format::Gif, _) => &[Vips, Magick, Ffmpeg],
        (Format::Bmp, _) => &[Dwebp, Magick, Ffmpeg],
    }
}

/// The dedicated encoder for a format, worth mentioning when it's missing.
fn specialist(to: Format, animated: bool) -> Option<ToolId> {
    match (to, animated) {
        (Format::Jxl, _) => Some(ToolId::Cjxl),
        (Format::Webp, false) => Some(ToolId::Cwebp),
        _ => None,
    }
}

const INTERMEDIATE: Format = Format::Png;

/// One possible route, with how well it serves the job.
struct Choice {
    steps: Vec<Step>,
    /// The final writer applies the metadata policy by itself.
    metadata_honoured: bool,
    /// Relevant settings the final writer can't apply.
    ignored_settings: Vec<String>,
}

impl Choice {
    /// Privacy first: honouring the metadata policy outranks applying every
    /// setting.
    fn rank(&self) -> (bool, bool) {
        (self.metadata_honoured, self.ignored_settings.is_empty())
    }
}

/// The best-ranked choice; ties go to the earliest (the preference order).
fn best(choices: Vec<Choice>) -> Option<Choice> {
    let mut best: Option<Choice> = None;
    for choice in choices {
        if best.as_ref().is_none_or(|b| choice.rank() > b.rank()) {
            best = Some(choice);
        }
    }
    best
}

pub fn find_route(tools: &Toolbox, req: &Requirements<'_>) -> Result<Routed, String> {
    let source_animated = matches!(req.animation, Animation::Animated { .. });
    let animated = source_animated && can_animate(req.to);
    let first_frame = source_animated && !animated;

    let mut notes = Vec::new();
    if first_frame {
        let frames = match req.animation {
            Animation::Animated { frames } => frames,
            _ => None,
        };
        notes.push(PlanNote::FirstFrameOnly { frames });
    }

    let single = writer_preference(req.to, animated)
        .iter()
        .filter_map(|&tool| tools.get(tool).map(|caps| (tool, caps)))
        .filter(|(_, caps)| reads_source(caps, req))
        .filter(|(_, caps)| converts(caps, req.from, req.to, animated, first_frame))
        .map(|(tool, caps)| one_step(tool, caps, req, animated, first_frame))
        .collect();
    let choice = match best(single) {
        Some(choice) => choice,
        // Animation isn't routed through a still PNG intermediate.
        None if animated => {
            return Err(format!(
                "no installed tool can convert an animated {} to an animated {}",
                req.from.display_name(),
                req.to.display_name()
            ));
        }
        None => best(two_steps(tools, req, first_frame)).ok_or_else(|| {
            let what = if first_frame {
                "the first frame of an animated "
            } else {
                ""
            };
            format!(
                "no installed tool can convert {what}{} to {}",
                req.from.display_name(),
                req.to.display_name()
            )
        })?,
    };

    let writer = *choice.steps.last().expect("a route has at least one step");
    if !choice.metadata_honoured {
        notes.push(metadata_note(req, writer.tool));
    }
    if writer.restore_jpeg {
        notes.push(PlanNote::JpegRestored);
    }
    if !choice.ignored_settings.is_empty() {
        notes.push(PlanNote::SettingsIgnored {
            tool: writer.tool,
            settings: choice.ignored_settings,
        });
    }
    if let Some(preferred) = specialist(req.to, animated)
        && !tools.contains(preferred)
        && writer.tool != preferred
    {
        notes.push(PlanNote::SpecialistMissing {
            preferred,
            used: writer.tool,
        });
    }

    Ok(Routed {
        route: Route {
            reason: explain(&choice.steps),
            steps: choice.steps,
        },
        notes,
    })
}

fn one_step(
    tool: ToolId,
    caps: &Capabilities,
    req: &Requirements<'_>,
    animated: bool,
    first_frame: bool,
) -> Choice {
    let restore_jpeg = caps.restores_jpeg
        && req.jpeg_reconstruction
        && req.from == Format::Jxl
        && req.to == Format::Jpeg;
    // Restoring the original JPEG involves no encoder settings at all.
    let ignored_settings = if restore_jpeg {
        Vec::new()
    } else {
        caps.ignores(req.to, &req.settings.relevant_settings(req.from))
    };
    Choice {
        steps: vec![Step {
            tool,
            from: req.from,
            to: req.to,
            animated,
            first_frame,
            restore_jpeg,
        }],
        // The restored file is the original JPEG, metadata included.
        metadata_honoured: (restore_jpeg && req.policy == MetadataPolicy::PreserveAll)
            || honours(caps.metadata, req, false),
        ignored_settings,
    }
}

/// Every decoder-to-PNG, PNG-to-encoder pairing, in preference order.
fn two_steps(tools: &Toolbox, req: &Requirements<'_>, first_frame: bool) -> Vec<Choice> {
    let available = |format: Format| {
        writer_preference(format, false)
            .iter()
            .filter_map(|&tool| tools.get(tool).map(|caps| (tool, caps)))
    };
    let still = |tool, from, to, first_frame| Step {
        tool,
        from,
        to,
        animated: false,
        first_frame,
        restore_jpeg: false,
    };
    let relevant = req.settings.relevant_settings(INTERMEDIATE);

    let mut choices = Vec::new();
    let decoders = available(INTERMEDIATE)
        .filter(|(_, caps)| reads_source(caps, req))
        .filter(|(_, caps)| converts(caps, req.from, INTERMEDIATE, false, first_frame));
    for (decoder, _) in decoders {
        let encoders = available(req.to)
            .filter(|(_, caps)| converts(caps, INTERMEDIATE, req.to, false, false));
        for (encoder, caps) in encoders {
            choices.push(Choice {
                steps: vec![
                    still(decoder, req.from, INTERMEDIATE, first_frame),
                    still(encoder, INTERMEDIATE, req.to, false),
                ],
                metadata_honoured: honours(caps.metadata, req, true),
                ignored_settings: caps.ignores(req.to, &relevant),
            });
        }
    }
    choices
}

/// False when the source is deeper than this tool can read.
fn reads_source(caps: &Capabilities, req: &Requirements<'_>) -> bool {
    let deep = req.bit_depth.is_some_and(|bits| bits > 8);
    !(deep && caps.shallow_reads.contains(&req.from))
}

fn converts(
    caps: &Capabilities,
    from: Format,
    to: Format,
    animated: bool,
    first_frame: bool,
) -> bool {
    if animated {
        caps.animated_reads.contains(&from) && caps.animated_writes.contains(&to)
    } else if first_frame {
        caps.first_frame_reads.contains(&from) && caps.writes.contains(&to)
    } else {
        caps.converts(from, to)
    }
}

/// Whether the final writer can apply the metadata policy to this source by
/// itself. Through an intermediate PNG nothing can be carried (decoders
/// aren't characterised), so only removing metadata is possible.
fn honours(
    support: Option<MetadataSupport>,
    req: &Requirements<'_>,
    via_intermediate: bool,
) -> bool {
    let m = req.metadata;
    let has = |p: Presence| p != Presence::Absent;
    let (exif, xmp, icc) = (has(m.exif), has(m.xmp), has(m.icc));
    if !exif && !xmp && !icc {
        return true; // nothing to keep or remove
    }
    let Some(support) = support else {
        return false;
    };
    let keeps = support.preserves;
    match req.policy {
        MetadataPolicy::PreserveAll => {
            !via_intermediate && (!exif || keeps.exif) && (!xmp || keeps.xmp) && (!icc || keeps.icc)
        }
        MetadataPolicy::StripAll => support.can_strip,
        MetadataPolicy::ColorProfileOnly => {
            if icc {
                !via_intermediate && keeps.icc && (support.selective || (!exif && !xmp))
            } else {
                support.can_strip
            }
        }
        // Keeping copyright fields while dropping location and device data
        // means editing EXIF/XMP tags, which no tool does on its own.
        MetadataPolicy::CopyrightOnly => !exif && !xmp && (!via_intermediate && keeps.icc),
    }
}

fn metadata_note(req: &Requirements<'_>, writer: ToolId) -> PlanNote {
    if req.policy == MetadataPolicy::CopyrightOnly {
        PlanNote::CopyrightFilteringUnavailable
    } else {
        PlanNote::MetadataNotHonoured {
            policy: req.policy,
            tool: writer,
        }
    }
}

fn explain(steps: &[Step]) -> String {
    match steps {
        [only] => format!(
            "{} converts {} to {}{}",
            only.tool.program_name(),
            only.from.display_name(),
            only.to.display_name(),
            if only.animated {
                ", keeping every frame"
            } else {
                ""
            }
        ),
        [first, second] => format!(
            "no single tool converts {} to {}: {} decodes to PNG, then {} encodes",
            first.from.display_name(),
            second.to.display_name(),
            first.tool.program_name(),
            second.tool.program_name()
        ),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::MetadataKinds;

    fn caps(reads: &[Format], writes: &[Format]) -> Capabilities {
        Capabilities {
            reads: reads.iter().copied().collect(),
            writes: writes.iter().copied().collect(),
            metadata: Some(MetadataSupport {
                preserves: MetadataKinds::ALL,
                can_strip: true,
                selective: true,
            }),
            ..Capabilities::default()
        }
    }

    fn req(from: Format, to: Format) -> Requirements<'static> {
        Requirements {
            from,
            to,
            animation: Animation::Still,
            metadata: MetadataPresence::default(),
            policy: MetadataPolicy::PreserveAll,
            settings: Box::leak(Box::new(CodecSettings::factory(to))),
            jpeg_reconstruction: false,
            bit_depth: None,
        }
    }

    fn ignoring(mut caps: Capabilities, format: Format, settings: &[&str]) -> Capabilities {
        caps.ignored_settings
            .insert(format, settings.iter().map(|s| (*s).to_owned()).collect());
        caps
    }

    fn no_metadata() -> MetadataPresence {
        MetadataPresence {
            exif: Presence::Absent,
            xmp: Presence::Absent,
            icc: Presence::Absent,
        }
    }

    fn tools_used(routed: &Routed) -> Vec<ToolId> {
        routed.route.steps.iter().map(|s| s.tool).collect()
    }

    #[test]
    fn specialist_wins_when_installed() {
        let tools = Toolbox::new([
            (ToolId::Vips, caps(&[Format::Png], &[Format::Jxl])),
            (
                ToolId::Cjxl,
                caps(&[Format::Png, Format::Jpeg], &[Format::Jxl]),
            ),
        ]);
        let routed = find_route(&tools, &req(Format::Png, Format::Jxl)).unwrap();
        assert_eq!(tools_used(&routed), [ToolId::Cjxl]);
        assert!(routed.notes.is_empty());
    }

    #[test]
    fn missing_specialist_is_noted() {
        let tools = Toolbox::new([(ToolId::Vips, caps(&[Format::Png], &[Format::Jxl]))]);
        let routed = find_route(&tools, &req(Format::Png, Format::Jxl)).unwrap();
        assert_eq!(tools_used(&routed), [ToolId::Vips]);
        assert_eq!(
            routed.notes,
            [PlanNote::SpecialistMissing {
                preferred: ToolId::Cjxl,
                used: ToolId::Vips
            }]
        );
    }

    #[test]
    fn falls_back_to_two_steps_through_png() {
        let tools = Toolbox::new([
            (ToolId::Vips, caps(&[Format::Heif], &[Format::Png])),
            (ToolId::Cwebp, caps(&[Format::Png], &[Format::Webp])),
        ]);
        let mut r = req(Format::Heif, Format::Webp);
        r.metadata = no_metadata();
        let routed = find_route(&tools, &r).unwrap();
        assert_eq!(tools_used(&routed), [ToolId::Vips, ToolId::Cwebp]);
        assert!(routed.route.reason.contains("decodes to PNG"));
    }

    #[test]
    fn prefers_a_writer_that_keeps_the_metadata() {
        let mut cwebp = caps(&[Format::Png], &[Format::Webp]);
        cwebp.metadata = Some(MetadataSupport {
            preserves: MetadataKinds::ICC_ONLY,
            can_strip: true,
            selective: true,
        });
        let tools = Toolbox::new([
            (ToolId::Cwebp, cwebp),
            (ToolId::Vips, caps(&[Format::Png], &[Format::Webp])),
        ]);
        let mut r = req(Format::Png, Format::Webp);
        r.metadata.exif = Presence::Present;
        let routed = find_route(&tools, &r).unwrap();
        assert_eq!(
            tools_used(&routed),
            [ToolId::Vips],
            "cwebp would drop the EXIF"
        );
    }

    #[test]
    fn unhonourable_metadata_is_noted_not_hidden() {
        let mut cjxl = caps(&[Format::Jpeg], &[Format::Jxl]);
        cjxl.metadata = Some(MetadataSupport {
            preserves: MetadataKinds::ALL,
            can_strip: false,
            selective: false,
        });
        let tools = Toolbox::new([(ToolId::Cjxl, cjxl)]);
        let mut r = req(Format::Jpeg, Format::Jxl);
        r.metadata.exif = Presence::Present;
        r.policy = MetadataPolicy::StripAll;
        let routed = find_route(&tools, &r).unwrap();
        assert_eq!(
            routed.notes,
            [PlanNote::MetadataNotHonoured {
                policy: MetadataPolicy::StripAll,
                tool: ToolId::Cjxl
            }]
        );
    }

    #[test]
    fn animated_source_needs_an_animation_capable_route() {
        let mut magick = caps(&[Format::Gif], &[Format::Webp]);
        magick.animated_reads = [Format::Gif].into();
        let still_only = Toolbox::new([(ToolId::Magick, caps(&[Format::Gif], &[Format::Webp]))]);
        let mut r = req(Format::Gif, Format::Webp);
        r.animation = Animation::Animated { frames: Some(3) };
        let err = find_route(&still_only, &r).unwrap_err();
        assert!(err.contains("animated GIF to an animated WebP"), "{err}");

        magick.animated_writes = [Format::Webp].into();
        let animated = Toolbox::new([(ToolId::Magick, magick)]);
        let routed = find_route(&animated, &r).unwrap();
        assert!(routed.route.steps[0].animated);
    }

    #[test]
    fn animated_source_to_still_format_keeps_first_frame_with_a_note() {
        let mut vips = caps(&[Format::Gif], &[Format::Jpeg]);
        vips.animated_reads = [Format::Gif].into();
        vips.first_frame_reads = [Format::Gif].into();
        let tools = Toolbox::new([(ToolId::Vips, vips)]);
        let mut r = req(Format::Gif, Format::Jpeg);
        r.animation = Animation::Animated { frames: Some(12) };
        r.metadata = no_metadata();
        let routed = find_route(&tools, &r).unwrap();
        assert_eq!(
            routed.notes,
            [PlanNote::FirstFrameOnly { frames: Some(12) }]
        );
        assert!(!routed.route.steps[0].animated);
    }

    #[test]
    fn copyright_only_with_exif_is_flagged() {
        let tools = Toolbox::new([(ToolId::Vips, caps(&[Format::Jpeg], &[Format::Avif]))]);
        let mut r = req(Format::Jpeg, Format::Avif);
        r.metadata.exif = Presence::Present;
        r.policy = MetadataPolicy::CopyrightOnly;
        let routed = find_route(&tools, &r).unwrap();
        assert_eq!(routed.notes, [PlanNote::CopyrightFilteringUnavailable]);
    }

    #[test]
    fn no_tools_means_no_route() {
        assert!(find_route(&Toolbox::default(), &req(Format::Png, Format::Webp)).is_err());
    }

    #[test]
    fn prefers_a_writer_that_applies_the_settings() {
        let djxl = ignoring(
            caps(&[Format::Jxl], &[Format::Png]),
            Format::Png,
            &["compression"],
        );
        let tools = Toolbox::new([
            (ToolId::Djxl, djxl),
            (ToolId::Vips, caps(&[Format::Jxl], &[Format::Png])),
        ]);
        let routed = find_route(&tools, &req(Format::Jxl, Format::Png)).unwrap();
        assert_eq!(tools_used(&routed), [ToolId::Vips]);
        assert!(routed.notes.is_empty());
    }

    #[test]
    fn ignored_settings_are_noted_when_unavoidable() {
        let djxl = ignoring(
            caps(&[Format::Jxl], &[Format::Jpeg]),
            Format::Jpeg,
            &["progressive"],
        );
        let tools = Toolbox::new([(ToolId::Djxl, djxl)]);
        let routed = find_route(&tools, &req(Format::Jxl, Format::Jpeg)).unwrap();
        assert_eq!(
            routed.notes,
            [PlanNote::SettingsIgnored {
                tool: ToolId::Djxl,
                settings: vec!["progressive".into()]
            }]
        );
    }

    #[test]
    fn settings_that_dont_apply_to_this_job_are_not_noted() {
        // vips can't recompress a JPEG losslessly, but that only matters
        // for lossless JXL from a JPEG source.
        let vips = ignoring(
            caps(&[Format::Png, Format::Jpeg], &[Format::Jxl]),
            Format::Jxl,
            &["reconstructJpeg"],
        );
        let tools = Toolbox::new([(ToolId::Vips, vips)]);
        let lossy = find_route(&tools, &req(Format::Jpeg, Format::Jxl)).unwrap();
        assert!(
            !lossy
                .notes
                .iter()
                .any(|n| matches!(n, PlanNote::SettingsIgnored { .. }))
        );

        let mut lossless = req(Format::Jpeg, Format::Jxl);
        lossless.settings = Box::leak(Box::new(CodecSettings::Jxl(crate::codec::JxlSettings {
            lossless: true,
            ..crate::codec::JxlSettings::factory()
        })));
        let routed = find_route(&tools, &lossless).unwrap();
        assert!(routed.notes.contains(&PlanNote::SettingsIgnored {
            tool: ToolId::Vips,
            settings: vec!["reconstructJpeg".into()]
        }));
    }

    #[test]
    fn metadata_outranks_settings() {
        let mut keeps_metadata = ignoring(
            caps(&[Format::Png], &[Format::Webp]),
            Format::Webp,
            &["quality"],
        );
        keeps_metadata.metadata = Some(MetadataSupport {
            preserves: MetadataKinds::ALL,
            can_strip: true,
            selective: true,
        });
        let mut drops_exif = caps(&[Format::Png], &[Format::Webp]);
        drops_exif.metadata = Some(MetadataSupport {
            preserves: MetadataKinds::ICC_ONLY,
            can_strip: true,
            selective: true,
        });
        let tools = Toolbox::new([(ToolId::Cwebp, drops_exif), (ToolId::Vips, keeps_metadata)]);
        let mut r = req(Format::Png, Format::Webp);
        r.metadata.exif = Presence::Present;
        let routed = find_route(&tools, &r).unwrap();
        assert_eq!(tools_used(&routed), [ToolId::Vips]);
    }

    #[test]
    fn recompressed_jpeg_is_restored_when_possible() {
        let mut djxl = ignoring(
            caps(&[Format::Jxl], &[Format::Jpeg]),
            Format::Jpeg,
            &["progressive"],
        );
        djxl.restores_jpeg = true;
        let tools = Toolbox::new([(ToolId::Djxl, djxl)]);
        let mut r = req(Format::Jxl, Format::Jpeg);
        r.metadata = no_metadata();
        r.jpeg_reconstruction = true;
        let routed = find_route(&tools, &r).unwrap();
        assert!(routed.route.steps[0].restore_jpeg);
        assert_eq!(routed.notes, [PlanNote::JpegRestored]);

        r.jpeg_reconstruction = false;
        let routed = find_route(&tools, &r).unwrap();
        assert!(!routed.route.steps[0].restore_jpeg);
    }

    #[test]
    fn restoring_keeps_the_original_metadata() {
        // djxl isn't characterised for metadata, and the JXL's ICC presence
        // is unknown; the restored JPEG still carries its own metadata.
        let mut djxl = caps(&[Format::Jxl], &[Format::Jpeg]);
        djxl.metadata = None;
        djxl.restores_jpeg = true;
        let tools = Toolbox::new([
            (ToolId::Djxl, djxl),
            (ToolId::Vips, caps(&[Format::Jxl], &[Format::Jpeg])),
        ]);
        let mut r = req(Format::Jxl, Format::Jpeg);
        r.jpeg_reconstruction = true;
        let routed = find_route(&tools, &r).unwrap();
        assert_eq!(tools_used(&routed), [ToolId::Djxl]);

        // Stripping is another matter: re-encode with a tool that can.
        r.policy = MetadataPolicy::StripAll;
        let routed = find_route(&tools, &r).unwrap();
        assert_eq!(tools_used(&routed), [ToolId::Vips]);
    }

    #[test]
    fn deep_sources_avoid_shallow_readers() {
        let mut vips = caps(&[Format::Avif], &[Format::Png]);
        vips.shallow_reads = [Format::Avif].into();
        let tools = Toolbox::new([
            (ToolId::Vips, vips),
            (ToolId::Magick, caps(&[Format::Avif], &[Format::Png])),
        ]);
        let mut r = req(Format::Avif, Format::Png);
        r.metadata = no_metadata();
        r.bit_depth = Some(8);
        assert_eq!(tools_used(&find_route(&tools, &r).unwrap()), [ToolId::Vips]);
        r.bit_depth = Some(10);
        assert_eq!(
            tools_used(&find_route(&tools, &r).unwrap()),
            [ToolId::Magick]
        );
    }

    #[test]
    fn first_frame_needs_a_tool_that_can_pick_it() {
        let mut djxl = caps(&[Format::Jxl], &[Format::Png]);
        djxl.animated_reads = [Format::Jxl].into();
        let mut r = req(Format::Jxl, Format::Jpeg);
        r.animation = Animation::Animated { frames: Some(3) };
        r.metadata = no_metadata();
        // djxl would write every frame, so it can't produce the still.
        let only_djxl = Toolbox::new([(ToolId::Djxl, djxl.clone())]);
        let err = find_route(&only_djxl, &r).unwrap_err();
        assert!(err.contains("first frame of an animated JPEG XL"), "{err}");

        let mut magick = caps(&[Format::Jxl], &[Format::Jpeg]);
        magick.animated_reads = [Format::Jxl].into();
        magick.first_frame_reads = [Format::Jxl].into();
        let tools = Toolbox::new([(ToolId::Djxl, djxl), (ToolId::Magick, magick)]);
        let routed = find_route(&tools, &r).unwrap();
        assert_eq!(tools_used(&routed), [ToolId::Magick]);
        assert!(routed.route.steps[0].first_frame);
    }
}
