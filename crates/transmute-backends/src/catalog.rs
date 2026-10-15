//! Capabilities per tool.
//!
//! Single-purpose tools (cjxl, cwebp, …) have fixed behaviour, described by
//! static tables. vips, ImageMagick and ffmpeg vary by build, so their
//! capabilities are parsed from the tool's own format/codec listings.

use transmute_core::capability::{Capabilities, MetadataKinds, MetadataSupport};
use transmute_core::format::Format;
use transmute_core::tool::ToolId;

use crate::version::Version;

/// Commands that list a tool's formats, for tools whose support depends on
/// how they were built. Each entry is one invocation.
pub fn listing_commands(tool: ToolId) -> &'static [&'static [&'static str]] {
    match tool {
        ToolId::Vips => &[&["-l", "foreign"]],
        ToolId::Magick => &[&["-list", "format"]],
        ToolId::Ffmpeg => &[
            &["-hide_banner", "-encoders"],
            &["-hide_banner", "-decoders"],
        ],
        _ => &[],
    }
}

/// Builds capabilities from the outputs of [`listing_commands`] (same order),
/// or from static tables for fixed-behaviour tools.
pub fn capabilities(
    tool: ToolId,
    version: Option<Version>,
    os: &str,
    listings: &[String],
) -> Capabilities {
    let listing = |index: usize| listings.get(index).map_or("", String::as_str);
    match tool {
        ToolId::Cjxl => cjxl(),
        ToolId::Djxl => djxl(),
        ToolId::Cwebp => cwebp(os),
        ToolId::Dwebp => dwebp(),
        ToolId::Gif2webp => gif2webp(),
        ToolId::Vips => vips(listing(0), version),
        ToolId::Magick => magick(listing(0)),
        ToolId::Ffmpeg => ffmpeg(listing(0), listing(1)),
        ToolId::Vipsheader | ToolId::Ffprobe => Capabilities {
            notes: vec!["Used to inspect files, not to convert them.".into()],
            ..Capabilities::default()
        },
    }
}

/// Notes derived from a tool's version output, e.g. ffmpeg's licence.
pub fn build_notes(tool: ToolId, version_output: &str) -> Vec<String> {
    if tool != ToolId::Ffmpeg {
        return Vec::new();
    }
    let configuration = version_output
        .lines()
        .find(|line| line.trim_start().starts_with("configuration:"))
        .unwrap_or("");
    let mut notes = Vec::new();
    if configuration.contains("--enable-nonfree") {
        notes.push("Non-free ffmpeg build: it cannot be redistributed.".into());
    } else if configuration.contains("--enable-gpl") {
        notes.push("GPL ffmpeg build: it can't be bundled with non-GPL software.".into());
    }
    notes
}

fn cjxl() -> Capabilities {
    Capabilities {
        reads: [Format::Jpeg, Format::Png, Format::Gif].into(),
        writes: [Format::Jxl].into(),
        animated_reads: [Format::Png, Format::Gif].into(),
        animated_writes: [Format::Jxl].into(),
        metadata: Some(MetadataSupport {
            preserves: MetadataKinds::ALL,
            // cjxl has no option to drop metadata.
            can_strip: false,
            selective: false,
        }),
        notes: vec!["Cannot strip metadata by itself.".into()],
    }
}

fn djxl() -> Capabilities {
    Capabilities {
        reads: [Format::Jxl].into(),
        writes: [Format::Png, Format::Jpeg].into(),
        ..Capabilities::default()
    }
}

fn cwebp(os: &str) -> Capabilities {
    // On Windows cwebp reads inputs through WIC, which only exposes the ICC
    // profile; EXIF and XMP are silently dropped (confirmed in the 1.x audit).
    let preserves = if os == "windows" {
        MetadataKinds::ICC_ONLY
    } else {
        MetadataKinds::ALL
    };
    let mut notes = Vec::new();
    if os == "windows" {
        notes.push("On Windows, only the colour profile is kept; EXIF and XMP are dropped.".into());
    }
    Capabilities {
        reads: [Format::Png, Format::Jpeg, Format::Tiff].into(),
        writes: [Format::Webp].into(),
        metadata: Some(MetadataSupport {
            preserves,
            can_strip: true,
            selective: true,
        }),
        notes,
        ..Capabilities::default()
    }
}

fn dwebp() -> Capabilities {
    Capabilities {
        reads: [Format::Webp].into(),
        writes: [Format::Png, Format::Tiff, Format::Bmp].into(),
        notes: vec!["Cannot decode animated WebP.".into()],
        ..Capabilities::default()
    }
}

fn gif2webp() -> Capabilities {
    Capabilities {
        reads: [Format::Gif].into(),
        writes: [Format::Webp].into(),
        animated_reads: [Format::Gif].into(),
        animated_writes: [Format::Webp].into(),
        metadata: Some(MetadataSupport {
            preserves: MetadataKinds {
                exif: false,
                xmp: true,
                icc: true,
            },
            can_strip: true,
            selective: true,
        }),
        notes: Vec::new(),
    }
}

/// Parses `vips -l foreign`: each operation is listed with its nickname in
/// parentheses, e.g. `VipsForeignSaveWebpFile (webpsave), save as WebP, …`.
fn vips(listing: &str, version: Option<Version>) -> Capabilities {
    let nicknames: Vec<&str> = listing
        .lines()
        .filter_map(|line| {
            let start = line.find('(')? + 1;
            let end = start + line[start..].find(')')?;
            Some(&line[start..end])
        })
        .collect();
    let has = |nickname: &str| nicknames.contains(&nickname);

    let mut caps = Capabilities::default();
    for (loader, saver, formats) in [
        ("jpegload", "jpegsave", &[Format::Jpeg][..]),
        ("pngload", "pngsave", &[Format::Png]),
        ("webpload", "webpsave", &[Format::Webp]),
        ("jxlload", "jxlsave", &[Format::Jxl]),
        ("heifload", "heifsave", &[Format::Avif, Format::Heif]),
        ("tiffload", "tiffsave", &[Format::Tiff]),
        ("gifload", "gifsave", &[Format::Gif]),
    ] {
        if has(loader) {
            caps.reads.extend(formats);
        }
        if has(saver) {
            caps.writes.extend(formats);
        }
    }
    if has("magickload") {
        caps.reads.insert(Format::Bmp);
    }
    // vips loads and saves every frame of GIF and WebP when asked.
    for format in [Format::Gif, Format::Webp] {
        if caps.reads.contains(&format) {
            caps.animated_reads.insert(format);
        }
        if caps.writes.contains(&format) {
            caps.animated_writes.insert(format);
        }
    }
    if caps.writes.contains(&Format::Heif) {
        caps.notes
            .push("HEIC output depends on the libheif build and hasn't been verified yet.".into());
    }

    // The `keep` option for choosing which metadata survives arrived in 8.15;
    // older versions can only keep everything or strip everything.
    let selective = version.is_some_and(|v| v >= Version::new(8, 15, 0));
    caps.metadata = Some(MetadataSupport {
        preserves: MetadataKinds::ALL,
        can_strip: true,
        selective,
    });
    caps
}

/// Parses `magick -list format`, whose rows look like
/// `      GIF* GIF       rw+   CompuServe graphics interchange format`.
/// The mode column is r (read), w (write), + (multiple images per file).
fn magick(listing: &str) -> Capabilities {
    let mut caps = Capabilities::default();
    for line in listing.lines() {
        let mut columns = line.split_whitespace();
        let (Some(name), Some(_module), Some(mode)) =
            (columns.next(), columns.next(), columns.next())
        else {
            continue;
        };
        let name = name.trim_end_matches('*');
        let Some((format, animated)) = magick_format(name) else {
            continue;
        };
        let mode = mode.as_bytes();
        if mode.len() != 3 {
            continue;
        }
        let (reads, writes, multi) = (mode[0] == b'r', mode[1] == b'w', mode[2] == b'+');
        if reads {
            caps.reads.insert(format);
        }
        if writes {
            caps.writes.insert(format);
        }
        if animated && multi {
            if reads {
                caps.animated_reads.insert(format);
            }
            if writes {
                caps.animated_writes.insert(format);
            }
        }
    }
    caps.metadata = Some(MetadataSupport {
        preserves: MetadataKinds::ALL,
        can_strip: true,
        selective: true,
    });
    caps
}

/// Maps an ImageMagick format name to ours, and whether "multiple images"
/// means animation for it (for TIFF it means pages).
fn magick_format(name: &str) -> Option<(Format, bool)> {
    Some(match name {
        "JPEG" | "JPG" => (Format::Jpeg, false),
        "PNG" => (Format::Png, false),
        "APNG" => (Format::Png, true),
        "WEBP" => (Format::Webp, true),
        "JXL" => (Format::Jxl, true),
        "AVIF" => (Format::Avif, false),
        "HEIC" | "HEIF" => (Format::Heif, false),
        "TIFF" | "TIF" => (Format::Tiff, false),
        "GIF" => (Format::Gif, true),
        "BMP" => (Format::Bmp, false),
        _ => return None,
    })
}

/// Parses `ffmpeg -encoders` and `-decoders`, whose rows look like
/// ` V....D libwebp_anim         libwebp WebP image (codec webp)`.
fn ffmpeg(encoders: &str, decoders: &str) -> Capabilities {
    let mut caps = Capabilities::default();
    for name in codec_names(encoders) {
        let (format, animated) = match name {
            "mjpeg" => (Format::Jpeg, false),
            "png" => (Format::Png, false),
            "apng" => (Format::Png, true),
            "libwebp" => (Format::Webp, false),
            "libwebp_anim" => (Format::Webp, true),
            "gif" => (Format::Gif, true),
            "bmp" => (Format::Bmp, false),
            "tiff" => (Format::Tiff, false),
            "libjxl" => (Format::Jxl, false),
            "libjxl_anim" => (Format::Jxl, true),
            _ => continue,
        };
        caps.writes.insert(format);
        if animated {
            caps.animated_writes.insert(format);
        }
    }
    for name in codec_names(decoders) {
        let (format, animated) = match name {
            "mjpeg" => (Format::Jpeg, false),
            "png" => (Format::Png, false),
            "apng" => (Format::Png, true),
            // ffmpeg's native WebP decoder is still-image only.
            "webp" => (Format::Webp, false),
            "gif" => (Format::Gif, true),
            "bmp" => (Format::Bmp, false),
            "tiff" => (Format::Tiff, false),
            "libjxl" | "jpegxl" => (Format::Jxl, false),
            "libjxl_anim" | "jpegxl_anim" => (Format::Jxl, true),
            _ => continue,
        };
        caps.reads.insert(format);
        if animated {
            caps.animated_reads.insert(format);
        }
    }
    caps.metadata = Some(MetadataSupport {
        // Image metadata isn't reliably carried through ffmpeg's filters.
        preserves: MetadataKinds::NONE,
        can_strip: true,
        selective: false,
    });
    caps
}

/// Codec names from an ffmpeg codec listing, skipping the legend above the
/// `------` separator.
fn codec_names(listing: &str) -> impl Iterator<Item = &str> {
    listing
        .lines()
        .skip_while(|line| !line.trim_start().starts_with("------"))
        .skip(1)
        .filter_map(|line| line.split_whitespace().nth(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    const VIPS_LISTING: &str = "\
          VipsForeignLoadNsgifFile (gifload), load GIF with libnsgif (.gif), priority=50, is_a
          VipsForeignLoadPngFile (pngload), load png from file (.png), priority=200, is_a
          VipsForeignLoadJpegFile (jpegload), load jpeg from file (.jpg, .jpeg, .jpe, .jfif), priority=50
          VipsForeignLoadWebpFile (webpload), load webp from file (.webp), priority=200
          VipsForeignLoadHeifFile (heifload), load a HEIF image (.heic, .heif, .avif), priority=0
          VipsForeignLoadMagickFile (magickload), load file with ImageMagick, nocache, priority=-100
          VipsForeignSaveCgifFile (gifsave), save as gif, nocache (.gif), priority=0, rgb alpha
          VipsForeignSavePngFile (pngsave), save image to file as png, nocache (.png), priority=0
          VipsForeignSaveJpegFile (jpegsave), save as jpeg, nocache (.jpg, .jpeg, .jpe, .jfif), priority=0
          VipsForeignSaveWebpFile (webpsave), save as WebP, nocache (.webp), priority=0, rgb alpha
          VipsForeignSaveHeifFile (heifsave), save image in HEIF format, nocache (.heic, .heif, .avif)
          VipsForeignSaveJxlFile (jxlsave), save image in JPEG-XL format, nocache (.jxl), priority=0";

    const MAGICK_LISTING: &str = "\
   Format  Module    Mode  Description
-------------------------------------------------------------------------------
     APNG  VIDEO     rw+   Animated Portable Network Graphics
     AVIF  HEIC      rw+   AV1 Image File Format (1.19.7)
      BMP* BMP       rw-   Microsoft Windows bitmap image
      GIF* GIF       rw+   CompuServe graphics interchange format
     HEIC  HEIC      r--   High Efficiency Image Format (1.19.7)
     JPEG* JPEG      rw-   Joint Photographic Experts Group JFIF format (libjpeg-turbo 3.1.0)
      PNG* PNG       rw-   Portable Network Graphics (libpng 1.6.47)
     TIFF* TIFF      rw+   Tagged Image File Format (LIBTIFF, Version 4.7.0)
     WEBP* WEBP      rw+   WebP Image Format (libwebp 1.5.0 [0210])";

    const FFMPEG_ENCODERS: &str = "\
Encoders:
 V..... = Video
 ------
 V....D apng                 APNG (Animated Portable Network Graphics) image
 V....D gif                  GIF (Graphics Interchange Format)
 VFS..D mjpeg                MJPEG (Motion JPEG)
 VF...D png                  PNG (Portable Network Graphics) image
 V....D libwebp_anim         libwebp WebP image (codec webp)
 V....D libwebp              libwebp WebP image (codec webp)
 A....D aac                  AAC (Advanced Audio Coding)";

    const FFMPEG_DECODERS: &str = "\
Decoders:
 ------
 VF...D apng                 APNG (Animated Portable Network Graphics) image
 V....D gif                  GIF (Graphics Interchange Format)
 VF...D webp                 WebP image";

    #[test]
    fn vips_capabilities_follow_its_operation_list() {
        let caps = vips(VIPS_LISTING, Some(Version::new(8, 18, 1)));
        assert_eq!(
            caps.writes,
            [
                Format::Jpeg,
                Format::Png,
                Format::Webp,
                Format::Jxl,
                Format::Avif,
                Format::Heif,
                Format::Gif
            ]
            .into()
        );
        // jxlload and tiffload aren't in this listing.
        assert!(!caps.reads.contains(&Format::Jxl));
        assert!(caps.reads.contains(&Format::Bmp), "via magickload");
        assert_eq!(caps.animated_writes, [Format::Webp, Format::Gif].into());
        assert!(caps.metadata.unwrap().selective);
    }

    #[test]
    fn old_vips_cannot_keep_metadata_selectively() {
        let caps = vips(VIPS_LISTING, Some(Version::new(8, 14, 0)));
        assert!(!caps.metadata.unwrap().selective);
        let unknown = vips(VIPS_LISTING, None);
        assert!(!unknown.metadata.unwrap().selective);
    }

    #[test]
    fn magick_mode_column_decides_read_write_and_animation() {
        let caps = magick(MAGICK_LISTING);
        assert!(caps.reads.contains(&Format::Heif));
        assert!(
            !caps.writes.contains(&Format::Heif),
            "HEIC is r-- in this build"
        );
        assert!(caps.writes.contains(&Format::Avif));
        assert_eq!(
            caps.animated_writes,
            [Format::Png, Format::Webp, Format::Gif].into()
        );
        assert!(
            !caps.animated_writes.contains(&Format::Tiff),
            "TIFF + means pages, not animation"
        );
    }

    #[test]
    fn ffmpeg_codecs_map_to_formats() {
        let caps = ffmpeg(FFMPEG_ENCODERS, FFMPEG_DECODERS);
        assert_eq!(
            caps.writes,
            [Format::Jpeg, Format::Png, Format::Webp, Format::Gif].into()
        );
        assert_eq!(
            caps.animated_writes,
            [Format::Png, Format::Webp, Format::Gif].into()
        );
        assert!(caps.reads.contains(&Format::Webp));
        assert!(
            !caps.animated_reads.contains(&Format::Webp),
            "native decoder is still-only"
        );
    }

    #[test]
    fn cwebp_on_windows_only_keeps_the_colour_profile() {
        let windows = cwebp("windows").metadata.unwrap();
        assert_eq!(windows.preserves, MetadataKinds::ICC_ONLY);
        let linux = cwebp("linux").metadata.unwrap();
        assert_eq!(linux.preserves, MetadataKinds::ALL);
    }

    #[test]
    fn cjxl_cannot_strip_metadata() {
        assert!(!cjxl().metadata.unwrap().can_strip);
    }

    #[test]
    fn ffmpeg_licence_is_noted() {
        let gpl = "ffmpeg version 7.1\nconfiguration: --enable-gpl --enable-libx264\n";
        assert_eq!(build_notes(ToolId::Ffmpeg, gpl).len(), 1);
        assert!(
            build_notes(
                ToolId::Ffmpeg,
                "ffmpeg version 7.1\nconfiguration: --enable-shared\n"
            )
            .is_empty()
        );
        assert!(build_notes(ToolId::Cjxl, gpl).is_empty());
    }

    #[test]
    fn every_capability_set_is_consistent() {
        let listings = [VIPS_LISTING.to_owned(), MAGICK_LISTING.to_owned()];
        for tool in ToolId::ALL {
            let caps = capabilities(tool, None, "windows", &listings);
            assert!(caps.animated_reads.is_subset(&caps.reads), "{tool:?}");
            assert!(caps.animated_writes.is_subset(&caps.writes), "{tool:?}");
        }
    }
}
