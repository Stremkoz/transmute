//! Choosing which tool(s) convert a source to a target format.
//!
//! Preference, in order:
//! 1. One step over two (via a temporary PNG).
//! 2. Routes whose writer can apply the metadata policy natively.
//! 3. The format's specialist tool (cjxl for JPEG XL, cwebp for WebP, …).
//!
//! Every choice records why, and every compromise becomes a [`PlanNote`].

use crate::capability::{Capabilities, MetadataSupport, Toolbox};
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
}

/// What a route must achieve, beyond the format change.
pub struct Requirements {
    pub from: Format,
    pub to: Format,
    pub animation: Animation,
    pub metadata: MetadataPresence,
    pub policy: MetadataPolicy,
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
        // ImageMagick writes APNG via ffmpeg and resamples the frame timing
        // (found in M3), so ffmpeg directly is preferred.
        (Format::Png, true) => &[Ffmpeg, Magick],
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

pub fn find_route(tools: &Toolbox, req: &Requirements) -> Result<Routed, String> {
    let source_animated = matches!(req.animation, Animation::Animated { .. });
    let animated = source_animated && can_animate(req.to);

    let mut notes = Vec::new();
    if source_animated && !animated {
        let frames = match req.animation {
            Animation::Animated { frames } => frames,
            _ => None,
        };
        notes.push(PlanNote::FirstFrameOnly { frames });
    }

    let single: Vec<(ToolId, &Capabilities)> = writer_preference(req.to, animated)
        .iter()
        .filter_map(|&tool| tools.get(tool).map(|caps| (tool, caps)))
        .filter(|(_, caps)| converts(caps, req.from, req.to, animated))
        .collect();

    let (steps, writer_support) = if let Some(&(tool, caps)) = single
        .iter()
        .find(|(_, caps)| honours(caps.metadata, req, false))
        .or_else(|| single.first())
    {
        (vec![step(tool, req.from, req.to, animated)], caps.metadata)
    } else if animated {
        // Animation isn't routed through a still PNG intermediate.
        return Err(format!(
            "no installed tool can convert an animated {} to an animated {}",
            req.from.display_name(),
            req.to.display_name()
        ));
    } else {
        two_step(tools, req).ok_or_else(|| {
            format!(
                "no installed tool can convert {} to {}",
                req.from.display_name(),
                req.to.display_name()
            )
        })?
    };

    let writer = steps.last().expect("a route has at least one step").tool;
    if !honours(writer_support, req, steps.len() > 1) {
        notes.push(metadata_note(req, writer));
    }
    if let Some(preferred) = specialist(req.to, animated)
        && !tools.contains(preferred)
        && writer != preferred
    {
        notes.push(PlanNote::SpecialistMissing {
            preferred,
            used: writer,
        });
    }

    Ok(Routed {
        route: Route {
            reason: explain(&steps),
            steps,
        },
        notes,
    })
}

fn two_step(tools: &Toolbox, req: &Requirements) -> Option<(Vec<Step>, Option<MetadataSupport>)> {
    let decoders = writer_preference(INTERMEDIATE, false)
        .iter()
        .filter(|&&tool| {
            tools
                .get(tool)
                .is_some_and(|c| c.converts(req.from, INTERMEDIATE))
        });
    let encoders: Vec<(ToolId, &Capabilities)> = writer_preference(req.to, false)
        .iter()
        .filter_map(|&tool| tools.get(tool).map(|caps| (tool, caps)))
        .filter(|(_, caps)| caps.converts(INTERMEDIATE, req.to))
        .collect();

    let pairs: Vec<_> = decoders
        .flat_map(|&decoder| {
            encoders
                .iter()
                .map(move |&(encoder, caps)| (decoder, encoder, caps))
        })
        .collect();
    let &(decoder, encoder, caps) = pairs
        .iter()
        .find(|(_, _, caps)| honours(caps.metadata, req, true))
        .or_else(|| pairs.first())?;
    Some((
        vec![
            step(decoder, req.from, INTERMEDIATE, false),
            step(encoder, INTERMEDIATE, req.to, false),
        ],
        caps.metadata,
    ))
}

fn converts(caps: &Capabilities, from: Format, to: Format, animated: bool) -> bool {
    if animated {
        caps.animated_reads.contains(&from) && caps.animated_writes.contains(&to)
    } else {
        caps.converts(from, to)
    }
}

fn step(tool: ToolId, from: Format, to: Format, animated: bool) -> Step {
    Step {
        tool,
        from,
        to,
        animated,
    }
}

/// Whether the final writer can apply the metadata policy to this source by
/// itself. Through an intermediate PNG nothing can be carried (decoders
/// aren't characterised), so only removing metadata is possible.
fn honours(support: Option<MetadataSupport>, req: &Requirements, via_intermediate: bool) -> bool {
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

fn metadata_note(req: &Requirements, writer: ToolId) -> PlanNote {
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

    fn req(from: Format, to: Format) -> Requirements {
        Requirements {
            from,
            to,
            animation: Animation::Still,
            metadata: MetadataPresence::default(),
            policy: MetadataPolicy::PreserveAll,
        }
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
        let tools = Toolbox::new([(ToolId::Vips, caps(&[Format::Gif], &[Format::Jpeg]))]);
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
}
