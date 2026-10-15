//! Per-codec encoder settings.
//!
//! Each codec exposes its own real controls: there is deliberately no universal
//! quality scale (spec §4). Every settings struct has a *patch* twin whose
//! fields are all optional. Patches are what user defaults and presets store,
//! and they layer over the factory defaults (null means "inherit").
//!
//! Factory defaults are provisional until the M9 benchmarks.

use serde::{Deserialize, Serialize};

use crate::format::Format;
use crate::validation::Validator;

/// Defines a settings struct, its patch twin, `factory()` and `apply()`.
macro_rules! codec_settings {
    (
        $(#[$meta:meta])*
        $name:ident / $patch:ident {
            $( $(#[doc = $doc:literal])* $field:ident : $ty:ty = $default:expr ),* $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        pub struct $name {
            $( $(#[doc = $doc])* pub $field: $ty, )*
        }

        impl $name {
            pub fn factory() -> Self {
                Self { $( $field: $default, )* }
            }

            pub fn apply(&mut self, patch: &$patch) {
                let _ = patch; // codecs without settings have no fields to apply
                $( if let Some(value) = &patch.$field { self.$field = value.clone(); } )*
            }
        }

        #[doc = concat!("Partial [`", stringify!($name), "`]; unset fields inherit.")]
        #[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        pub struct $patch {
            $(
                $(#[doc = $doc])*
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub $field: Option<$ty>,
            )*
        }
    };
}

codec_settings! {
    JpegSettings / JpegPatch {
        /// libjpeg-style quality, 1–100.
        quality: u8 = 85,
        /// Progressive scans: usually smaller, decodes incrementally on the web.
        progressive: bool = true,
    }
}

codec_settings! {
    PngSettings / PngPatch {
        /// zlib compression level, 0 (fastest) – 9 (smallest). Always lossless.
        compression: u8 = 6,
    }
}

codec_settings! {
    WebpSettings / WebpPatch {
        /// Lossless mode ignores `quality`.
        lossless: bool = false,
        /// cwebp quality, 0–100.
        quality: u8 = 85,
        /// cwebp method, 0 (fastest) – 6 (smallest).
        method: u8 = 4,
    }
}

codec_settings! {
    JxlSettings / JxlPatch {
        /// Lossless mode ignores `distance`.
        lossless: bool = false,
        /// Butteraugli distance: ~1.0 is visually lossless, higher is smaller.
        distance: f32 = 1.0,
        /// Encoder effort, 1 (fastest) – 10 (smallest).
        effort: u8 = 7,
        /// In lossless mode, store JPEG inputs as a reversible recompression of
        /// the original bitstream instead of re-encoding the pixels.
        reconstruct_jpeg: bool = true,
    }
}

codec_settings! {
    AvifSettings / AvifPatch {
        /// Lossless mode ignores `quality`.
        lossless: bool = false,
        /// Quality, 1–100.
        quality: u8 = 60,
        /// Encoder effort, 0 (fastest) – 9 (smallest).
        effort: u8 = 4,
    }
}

codec_settings! {
    HeifSettings / HeifPatch {
        /// Lossless mode ignores `quality`.
        lossless: bool = false,
        /// Quality, 1–100.
        quality: u8 = 60,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TiffCompression {
    None,
    Lzw,
    Deflate,
    Zstd,
}

codec_settings! {
    TiffSettings / TiffPatch {
        compression: TiffCompression = TiffCompression::Lzw,
    }
}

codec_settings! {
    /// GIF output has no tunable settings yet; palette generation is automatic.
    GifSettings / GifPatch {}
}

codec_settings! {
    BmpSettings / BmpPatch {}
}

/// Fully specified encoder settings for one output format.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "format", rename_all = "camelCase")]
pub enum CodecSettings {
    Jpeg(JpegSettings),
    Png(PngSettings),
    Webp(WebpSettings),
    Jxl(JxlSettings),
    Avif(AvifSettings),
    Heif(HeifSettings),
    Tiff(TiffSettings),
    Gif(GifSettings),
    Bmp(BmpSettings),
}

/// Partial encoder settings for one output format. The `format` tag is what
/// selects the output format of a target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "format", rename_all = "camelCase")]
pub enum CodecPatch {
    #[serde(alias = "jpg")]
    Jpeg(JpegPatch),
    Png(PngPatch),
    Webp(WebpPatch),
    Jxl(JxlPatch),
    Avif(AvifPatch),
    #[serde(alias = "heic")]
    Heif(HeifPatch),
    #[serde(alias = "tif")]
    Tiff(TiffPatch),
    Gif(GifPatch),
    Bmp(BmpPatch),
}

/// Calls `$body` with the inner value of whichever variant `$value` holds.
macro_rules! for_each_codec {
    ($enum:ident, $value:expr, $inner:ident => $body:expr) => {
        match $value {
            $enum::Jpeg($inner) => $body,
            $enum::Png($inner) => $body,
            $enum::Webp($inner) => $body,
            $enum::Jxl($inner) => $body,
            $enum::Avif($inner) => $body,
            $enum::Heif($inner) => $body,
            $enum::Tiff($inner) => $body,
            $enum::Gif($inner) => $body,
            $enum::Bmp($inner) => $body,
        }
    };
}

impl CodecSettings {
    pub fn factory(format: Format) -> Self {
        match format {
            Format::Jpeg => Self::Jpeg(JpegSettings::factory()),
            Format::Png => Self::Png(PngSettings::factory()),
            Format::Webp => Self::Webp(WebpSettings::factory()),
            Format::Jxl => Self::Jxl(JxlSettings::factory()),
            Format::Avif => Self::Avif(AvifSettings::factory()),
            Format::Heif => Self::Heif(HeifSettings::factory()),
            Format::Tiff => Self::Tiff(TiffSettings::factory()),
            Format::Gif => Self::Gif(GifSettings::factory()),
            Format::Bmp => Self::Bmp(BmpSettings::factory()),
        }
    }

    pub fn format(&self) -> Format {
        match self {
            Self::Jpeg(_) => Format::Jpeg,
            Self::Png(_) => Format::Png,
            Self::Webp(_) => Format::Webp,
            Self::Jxl(_) => Format::Jxl,
            Self::Avif(_) => Format::Avif,
            Self::Heif(_) => Format::Heif,
            Self::Tiff(_) => Format::Tiff,
            Self::Gif(_) => Format::Gif,
            Self::Bmp(_) => Format::Bmp,
        }
    }

    /// Applies `patch` if it targets the same format; returns `false` (and
    /// changes nothing) on a format mismatch.
    pub fn apply(&mut self, patch: &CodecPatch) -> bool {
        match (self, patch) {
            (Self::Jpeg(s), CodecPatch::Jpeg(p)) => s.apply(p),
            (Self::Png(s), CodecPatch::Png(p)) => s.apply(p),
            (Self::Webp(s), CodecPatch::Webp(p)) => s.apply(p),
            (Self::Jxl(s), CodecPatch::Jxl(p)) => s.apply(p),
            (Self::Avif(s), CodecPatch::Avif(p)) => s.apply(p),
            (Self::Heif(s), CodecPatch::Heif(p)) => s.apply(p),
            (Self::Tiff(s), CodecPatch::Tiff(p)) => s.apply(p),
            (Self::Gif(s), CodecPatch::Gif(p)) => s.apply(p),
            (Self::Bmp(s), CodecPatch::Bmp(p)) => s.apply(p),
            _ => return false,
        }
        true
    }

    pub fn validate(&self, v: &mut Validator) {
        for_each_codec!(Self, self, settings => settings.validate(v));
    }

    /// The settings (by their preset field name) that shape the output when
    /// converting from `source`. Lossless mode makes quality irrelevant, for
    /// example. Routing uses this to tell whether a tool that can't apply
    /// some setting matters for this job.
    pub fn relevant_settings(&self, source: Format) -> Vec<&'static str> {
        match self {
            Self::Jpeg(_) => vec!["quality", "progressive"],
            Self::Png(_) => vec!["compression"],
            Self::Webp(s) if s.lossless => vec!["lossless", "method"],
            Self::Webp(_) => vec!["lossless", "quality", "method"],
            Self::Jxl(s) if s.lossless && s.reconstruct_jpeg && source == Format::Jpeg => {
                vec!["lossless", "reconstructJpeg"]
            }
            Self::Jxl(s) if s.lossless => vec!["lossless", "effort"],
            Self::Jxl(_) => vec!["lossless", "distance", "effort"],
            Self::Avif(s) if s.lossless => vec!["lossless", "effort"],
            Self::Avif(_) => vec!["lossless", "quality", "effort"],
            Self::Heif(s) if s.lossless => vec!["lossless"],
            Self::Heif(_) => vec!["lossless", "quality"],
            Self::Tiff(_) => vec!["compression"],
            Self::Gif(_) | Self::Bmp(_) => Vec::new(),
        }
    }
}

impl CodecPatch {
    /// An empty patch: the format's inherited settings, unchanged.
    pub fn empty(format: Format) -> Self {
        match format {
            Format::Jpeg => Self::Jpeg(JpegPatch::default()),
            Format::Png => Self::Png(PngPatch::default()),
            Format::Webp => Self::Webp(WebpPatch::default()),
            Format::Jxl => Self::Jxl(JxlPatch::default()),
            Format::Avif => Self::Avif(AvifPatch::default()),
            Format::Heif => Self::Heif(HeifPatch::default()),
            Format::Tiff => Self::Tiff(TiffPatch::default()),
            Format::Gif => Self::Gif(GifPatch::default()),
            Format::Bmp => Self::Bmp(BmpPatch::default()),
        }
    }

    pub fn format(&self) -> Format {
        match self {
            Self::Jpeg(_) => Format::Jpeg,
            Self::Png(_) => Format::Png,
            Self::Webp(_) => Format::Webp,
            Self::Jxl(_) => Format::Jxl,
            Self::Avif(_) => Format::Avif,
            Self::Heif(_) => Format::Heif,
            Self::Tiff(_) => Format::Tiff,
            Self::Gif(_) => Format::Gif,
            Self::Bmp(_) => Format::Bmp,
        }
    }

    /// Validates the values a patch sets by applying it to the factory
    /// defaults, so issues are reported against the patch's own fields.
    pub fn validate(&self, v: &mut Validator) {
        CodecSettings::factory_for_patch(self).validate(v);
    }
}

impl CodecSettings {
    fn factory_for_patch(patch: &CodecPatch) -> Self {
        let mut settings = Self::factory(patch.format());
        settings.apply(patch);
        settings
    }
}

trait ValidateSettings {
    fn validate(&self, v: &mut Validator);
}

impl ValidateSettings for JpegSettings {
    fn validate(&self, v: &mut Validator) {
        v.check_range("quality", self.quality, 1..=100);
    }
}

impl ValidateSettings for PngSettings {
    fn validate(&self, v: &mut Validator) {
        v.check_range("compression", self.compression, 0..=9);
    }
}

impl ValidateSettings for WebpSettings {
    fn validate(&self, v: &mut Validator) {
        v.check_range("quality", self.quality, 0..=100);
        v.check_range("method", self.method, 0..=6);
    }
}

impl ValidateSettings for JxlSettings {
    fn validate(&self, v: &mut Validator) {
        // A distance of 0 means lossless to cjxl; we model that with
        // `lossless` instead so the two can't disagree.
        if !self.distance.is_finite() || !(0.01..=25.0).contains(&self.distance) {
            v.at("distance", |v| {
                v.issue(format!(
                    "must be between 0.01 and 25 (got {}); use `lossless: true` for lossless output",
                    self.distance
                ));
            });
        }
        v.check_range("effort", self.effort, 1..=10);
    }
}

impl ValidateSettings for AvifSettings {
    fn validate(&self, v: &mut Validator) {
        v.check_range("quality", self.quality, 1..=100);
        v.check_range("effort", self.effort, 0..=9);
    }
}

impl ValidateSettings for HeifSettings {
    fn validate(&self, v: &mut Validator) {
        v.check_range("quality", self.quality, 1..=100);
    }
}

impl ValidateSettings for TiffSettings {
    fn validate(&self, _: &mut Validator) {}
}

impl ValidateSettings for GifSettings {
    fn validate(&self, _: &mut Validator) {}
}

impl ValidateSettings for BmpSettings {
    fn validate(&self, _: &mut Validator) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relevant_settings_name_real_fields() {
        for format in Format::ALL {
            let settings = CodecSettings::factory(format);
            let json = serde_json::to_value(&settings).unwrap();
            for source in Format::ALL {
                for name in settings.relevant_settings(source) {
                    assert!(json.get(name).is_some(), "{format:?} has no {name}");
                }
            }
        }
    }

    #[test]
    fn lossless_makes_quality_irrelevant() {
        let mut jxl = JxlSettings::factory();
        jxl.lossless = true;
        let jxl = CodecSettings::Jxl(jxl);
        assert_eq!(
            jxl.relevant_settings(Format::Jpeg),
            ["lossless", "reconstructJpeg"]
        );
        assert_eq!(jxl.relevant_settings(Format::Png), ["lossless", "effort"]);
        assert!(
            CodecSettings::factory(Format::Jxl)
                .relevant_settings(Format::Png)
                .contains(&"distance")
        );
    }

    #[test]
    fn factory_settings_are_valid_for_every_format() {
        for format in Format::ALL {
            let settings = CodecSettings::factory(format);
            assert_eq!(settings.format(), format);
            let mut v = Validator::new();
            settings.validate(&mut v);
            assert!(v.finish().is_ok(), "{format:?} factory settings invalid");
        }
    }

    #[test]
    fn patch_overrides_only_the_fields_it_sets() {
        let mut settings = CodecSettings::factory(Format::Webp);
        let applied = settings.apply(&CodecPatch::Webp(WebpPatch {
            quality: Some(70),
            ..WebpPatch::default()
        }));
        assert!(applied);
        let CodecSettings::Webp(webp) = settings else {
            panic!("format changed");
        };
        assert_eq!(webp.quality, 70);
        assert_eq!(webp.method, WebpSettings::factory().method);
    }

    #[test]
    fn patch_for_another_format_is_rejected() {
        let mut settings = CodecSettings::factory(Format::Webp);
        let before = settings.clone();
        assert!(!settings.apply(&CodecPatch::empty(Format::Jxl)));
        assert_eq!(settings, before);
    }

    #[test]
    fn patch_json_is_tagged_by_format_and_omits_unset_fields() {
        let patch = CodecPatch::Jxl(JxlPatch {
            distance: Some(1.5),
            ..JxlPatch::default()
        });
        let json = serde_json::to_string(&patch).unwrap();
        assert_eq!(json, r#"{"format":"jxl","distance":1.5}"#);
        assert_eq!(serde_json::from_str::<CodecPatch>(&json).unwrap(), patch);
    }

    #[test]
    fn patch_json_rejects_fields_from_other_codecs() {
        let err =
            serde_json::from_str::<CodecPatch>(r#"{"format":"webp","distance":1.0}"#).unwrap_err();
        assert!(
            err.to_string().contains("unknown field `distance`"),
            "{err}"
        );
    }

    #[test]
    fn patch_json_accepts_extension_aliases() {
        let patch: CodecPatch = serde_json::from_str(r#"{"format":"jpg","quality":90}"#).unwrap();
        assert_eq!(patch.format(), Format::Jpeg);
    }

    #[test]
    fn invalid_patch_values_are_reported_by_field() {
        let patch: CodecPatch =
            serde_json::from_str(r#"{"format":"jxl","distance":0.0,"effort":12}"#).unwrap();
        let mut v = Validator::new();
        patch.validate(&mut v);
        let issues = v.finish().unwrap_err().issues;
        let paths: Vec<_> = issues.iter().map(|i| i.path.as_str()).collect();
        assert_eq!(paths, ["distance", "effort"]);
    }
}
