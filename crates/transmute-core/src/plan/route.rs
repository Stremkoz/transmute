//! Choosing which tool(s) convert a source to a target format.
//!
//! Preference, in order:
//! 1. One step over two (via a temporary PNG).
//! 2. Routes whose writer can apply the metadata policy natively.
//! 3. Routes whose writer can apply every relevant codec setting.
//! 4. The format's specialist tool (cjxl for JPEG XL, cwebp for WebP, …).
//!
//! Every choice records why, and every compromise becomes a [`PlanNote`].

use crate::capability::{Capabilities, MetadataKinds, Toolbox};
use crate::codec::CodecSettings;
use crate::format::Format;
use crate::inspect::{Animation, MetadataPresence, Presence};
use crate::metadata;
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
    /// Metadata the route can't treat as the policy asks.
    metadata_gaps: MetadataGaps,
    /// Relevant settings the final writer can't apply.
    ignored_settings: Vec<String>,
}

impl Choice {
    /// Privacy first: honouring the metadata policy outranks applying every
    /// setting.
    fn rank(&self) -> (bool, bool) {
        (
            self.metadata_gaps.is_empty(),
            self.ignored_settings.is_empty(),
        )
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
    notes.extend(choice.metadata_gaps.notes(req, writer.tool));
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
        metadata_gaps: if restore_jpeg && req.policy == MetadataPolicy::PreserveAll {
            MetadataGaps::default()
        } else if req.policy != MetadataPolicy::PreserveAll
            && recompresses_jpeg(caps, req.settings, req.from, req.to)
        {
            // The JPEG's metadata is reduced before recompression
            // (`MetadataHandling::clean_source`), and JPEG is rewritable.
            MetadataGaps::default()
        } else {
            metadata_gaps(caps, req, false)
        },
        ignored_settings,
    }
}

/// True when this tool would store a JPEG losslessly inside a JPEG XL, its
/// metadata included in the reconstruction data.
pub(crate) fn recompresses_jpeg(
    caps: &Capabilities,
    settings: &CodecSettings,
    from: Format,
    to: Format,
) -> bool {
    from == Format::Jpeg
        && to == Format::Jxl
        && settings
            .relevant_settings(from)
            .contains(&"reconstructJpeg")
        && caps.ignores(to, &["reconstructJpeg"]).is_empty()
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
                metadata_gaps: metadata_gaps(caps, req, true),
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

/// Metadata a route can't treat as the policy asks, and why.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct MetadataGaps {
    /// Kinds the tools and Transmute's own rewriting can't handle.
    tool: MetadataKinds,
    /// Kinds to keep that the output format can't hold.
    format: MetadataKinds,
    /// Copyright fields can't be separated from the rest of the EXIF.
    copyright: bool,
}

impl MetadataGaps {
    fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    fn notes(&self, req: &Requirements<'_>, writer: ToolId) -> Vec<PlanNote> {
        let mut notes = Vec::new();
        if self.format != MetadataKinds::NONE {
            notes.push(PlanNote::MetadataLost {
                format: req.to,
                kinds: self.format,
            });
        }
        if self.tool != MetadataKinds::NONE {
            notes.push(PlanNote::MetadataNotHonoured {
                policy: req.policy,
                tool: writer,
                kinds: self.tool,
            });
        }
        if self.copyright {
            notes.push(PlanNote::CopyrightFilteringUnavailable);
        }
        notes
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Exif,
    Xmp,
    Icc,
}

fn has(kinds: MetadataKinds, kind: Kind) -> bool {
    match kind {
        Kind::Exif => kinds.exif,
        Kind::Xmp => kinds.xmp,
        Kind::Icc => kinds.icc,
    }
}

fn with(mut kinds: MetadataKinds, kind: Kind) -> MetadataKinds {
    match kind {
        Kind::Exif => kinds.exif = true,
        Kind::Xmp => kinds.xmp = true,
        Kind::Icc => kinds.icc = true,
    }
    kinds
}

/// What a policy asks for one kind (see the table in `crate::metadata`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Want {
    Keep,
    Remove,
    /// EXIF reduced to copyright fields (and orientation).
    Copyright,
}

fn want(policy: MetadataPolicy, kind: Kind) -> Want {
    match (policy, kind) {
        (MetadataPolicy::PreserveAll, _) => Want::Keep,
        (MetadataPolicy::StripAll, _) => Want::Remove,
        (_, Kind::Icc) => Want::Keep,
        (MetadataPolicy::CopyrightOnly, Kind::Exif) => Want::Copyright,
        _ => Want::Remove,
    }
}

/// What can't be honoured for this source, given the final writer's own
/// metadata support and Transmute's container rewriting
/// (`crate::metadata`), which works from the original source. Through an
/// intermediate PNG the writer carries nothing of the source's.
fn metadata_gaps(
    caps: &Capabilities,
    req: &Requirements<'_>,
    via_intermediate: bool,
) -> MetadataGaps {
    let support = caps.metadata;
    let ours = metadata::rewrites(req.to);
    let readable = metadata::extracts(req.from);
    let holds = metadata::holds(req.to);
    let carried = if via_intermediate {
        MetadataKinds::NONE
    } else {
        caps.carries(req.from, req.to)
    };
    // Outputs we rewrite are encoded keeping everything; others are told
    // the policy, which they can only apply partially if not selective.
    let tool_removes = support
        .is_some_and(|s| s.can_strip && (req.policy == MetadataPolicy::StripAll || s.selective));
    // Our copy comes from the source if we can read it, else from what the
    // writer carried.
    let available = |kind| has(readable, kind) || has(carried, kind);

    let mut gaps = MetadataGaps::default();
    for (kind, presence) in [
        (Kind::Exif, req.metadata.exif),
        (Kind::Xmp, req.metadata.xmp),
        (Kind::Icc, req.metadata.icc),
    ] {
        if presence == Presence::Absent {
            continue;
        }
        let ok = match want(req.policy, kind) {
            Want::Keep if !has(holds, kind) => {
                gaps.format = with(gaps.format, kind);
                continue;
            }
            Want::Keep => has(carried, kind) || (has(ours, kind) && has(readable, kind)),
            Want::Remove => has(ours, kind) || tool_removes,
            Want::Copyright => {
                if !(has(ours, kind) && available(kind)) {
                    gaps.copyright = true;
                }
                continue;
            }
        };
        if !ok {
            gaps.tool = with(gaps.tool, kind);
        }
    }
    gaps
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
    use crate::capability::MetadataSupport;

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

    fn metadata_notes(routed: &Routed) -> usize {
        routed
            .notes
            .iter()
            .filter(|n| {
                matches!(
                    n,
                    PlanNote::MetadataNotHonoured { .. }
                        | PlanNote::MetadataLost { .. }
                        | PlanNote::CopyrightFilteringUnavailable
                )
            })
            .count()
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
        // TIFF output isn't rewritten by Transmute, so the tool decides.
        let mut vips = caps(&[Format::Png], &[Format::Tiff]);
        vips.metadata = Some(MetadataSupport {
            preserves: MetadataKinds::ICC_ONLY,
            can_strip: true,
            selective: true,
        });
        let tools = Toolbox::new([
            (ToolId::Vips, vips),
            (ToolId::Magick, caps(&[Format::Png], &[Format::Tiff])),
        ]);
        let mut r = req(Format::Png, Format::Tiff);
        r.metadata.exif = Presence::Present;
        let routed = find_route(&tools, &r).unwrap();
        assert_eq!(
            tools_used(&routed),
            [ToolId::Magick],
            "this vips would drop the EXIF"
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
        r.metadata = no_metadata();
        r.policy = MetadataPolicy::StripAll;
        // EXIF in a JPEG XL is a container box Transmute removes itself…
        r.metadata.exif = Presence::Present;
        assert!(find_route(&tools, &r).unwrap().notes.is_empty());
        // …but the colour profile is inside the codestream.
        r.metadata.icc = Presence::Present;
        let routed = find_route(&tools, &r).unwrap();
        assert_eq!(
            routed.notes,
            [PlanNote::MetadataNotHonoured {
                policy: MetadataPolicy::StripAll,
                tool: ToolId::Cjxl,
                kinds: MetadataKinds::ICC_ONLY,
            }]
        );
    }

    #[test]
    fn metadata_a_tool_cant_read_from_the_source_is_noted() {
        // ImageMagick skips JPEG XL's compressed EXIF and XMP boxes.
        let mut magick = caps(&[Format::Jxl, Format::Jpeg], &[Format::Avif]);
        magick.metadata_unread = std::collections::BTreeMap::from([(
            Format::Jxl,
            MetadataKinds {
                exif: true,
                xmp: true,
                icc: false,
            },
        )]);
        let tools = Toolbox::new([(ToolId::Magick, magick)]);
        let mut r = req(Format::Jxl, Format::Avif);
        r.metadata = no_metadata();
        r.metadata.exif = Presence::Present;
        assert_eq!(metadata_notes(&find_route(&tools, &r).unwrap()), 1);
        // From a JPEG, the same tool carries it.
        r.from = Format::Jpeg;
        assert_eq!(metadata_notes(&find_route(&tools, &r).unwrap()), 0);
    }

    #[test]
    fn rewritable_outputs_get_metadata_from_the_source() {
        // ffmpeg keeps nothing, but a WebP's blocks are set from the JPEG.
        let mut ffmpeg = caps(&[Format::Jpeg, Format::Tiff], &[Format::Webp]);
        ffmpeg.metadata = Some(MetadataSupport {
            preserves: MetadataKinds::NONE,
            can_strip: true,
            selective: false,
        });
        let tools = Toolbox::new([(ToolId::Ffmpeg, ffmpeg)]);
        let mut r = req(Format::Jpeg, Format::Webp);
        r.metadata.exif = Presence::Present;
        for policy in [
            MetadataPolicy::PreserveAll,
            MetadataPolicy::CopyrightOnly,
            MetadataPolicy::ColorProfileOnly,
        ] {
            r.policy = policy;
            assert_eq!(
                metadata_notes(&find_route(&tools, &r).unwrap()),
                0,
                "{policy:?}"
            );
        }
        // A TIFF's EXIF can't be read, and ffmpeg didn't carry it.
        r.from = Format::Tiff;
        r.policy = MetadataPolicy::PreserveAll;
        assert_eq!(metadata_notes(&find_route(&tools, &r).unwrap()), 1);
    }

    #[test]
    fn formats_without_room_for_metadata_say_so() {
        let tools = Toolbox::new([(ToolId::Vips, caps(&[Format::Jpeg], &[Format::Gif]))]);
        let mut r = req(Format::Jpeg, Format::Gif);
        r.metadata = no_metadata();
        r.metadata.exif = Presence::Present;
        assert_eq!(
            find_route(&tools, &r).unwrap().notes,
            [PlanNote::MetadataLost {
                format: Format::Gif,
                kinds: MetadataKinds {
                    exif: true,
                    xmp: false,
                    icc: false
                }
            }]
        );
        r.policy = MetadataPolicy::StripAll;
        assert!(find_route(&tools, &r).unwrap().notes.is_empty());
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
    fn jpeg_from_jxl_goes_to_a_tool_that_flattens_transparency() {
        // djxl drops alpha when writing JPEG; a JPEG XL may be transparent.
        let djxl = ignoring(
            caps(&[Format::Jxl], &[Format::Jpeg]),
            Format::Jpeg,
            &["progressive", "background"],
        );
        let tools = Toolbox::new([
            (ToolId::Djxl, djxl),
            (ToolId::Vips, caps(&[Format::Jxl], &[Format::Jpeg])),
        ]);
        let routed = find_route(&tools, &req(Format::Jxl, Format::Jpeg)).unwrap();
        assert_eq!(tools_used(&routed), [ToolId::Vips]);
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
            caps(&[Format::Png], &[Format::Tiff]),
            Format::Tiff,
            &["compression"],
        );
        keeps_metadata.metadata = Some(MetadataSupport {
            preserves: MetadataKinds::ALL,
            can_strip: true,
            selective: true,
        });
        let mut drops_exif = caps(&[Format::Png], &[Format::Tiff]);
        drops_exif.metadata = Some(MetadataSupport {
            preserves: MetadataKinds::ICC_ONLY,
            can_strip: true,
            selective: true,
        });
        let tools = Toolbox::new([(ToolId::Vips, drops_exif), (ToolId::Magick, keeps_metadata)]);
        let mut r = req(Format::Png, Format::Tiff);
        r.metadata.exif = Presence::Present;
        let routed = find_route(&tools, &r).unwrap();
        assert_eq!(tools_used(&routed), [ToolId::Magick]);
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

        // Stripping still restores, then Transmute strips the JPEG.
        r.policy = MetadataPolicy::StripAll;
        let routed = find_route(&tools, &r).unwrap();
        assert_eq!(tools_used(&routed), [ToolId::Djxl]);
        assert!(
            !routed
                .notes
                .iter()
                .any(|n| matches!(n, PlanNote::MetadataNotHonoured { .. }))
        );
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
