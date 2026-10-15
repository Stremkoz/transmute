//! Where each output goes and what it's called.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Local, NaiveDate};

use crate::format::Format;
use crate::inventory::{RootKind, SourceEntry, SourceRoot};
use crate::naming::{DateSource, DateStyle, NamingRule, Segment, Token, parse_template};
use crate::output::Destination;

/// Everything naming needs to know about one source in the batch.
pub struct NameContext<'a> {
    pub entry: &'a SourceEntry,
    pub format: Format,
    /// 1-based position of the source in the batch.
    pub counter: usize,
    /// Digits to pad `{counter}` to: enough for the batch size.
    pub counter_width: usize,
    pub batch_date: NaiveDate,
}

/// The folder an output goes into.
pub fn output_dir(
    entry: &SourceEntry,
    root: &SourceRoot,
    destination: &Destination,
    format: Format,
) -> PathBuf {
    let source_dir = entry.path.parent().map(Path::to_owned).unwrap_or_default();
    match destination {
        Destination::BesideSource => source_dir,
        Destination::Subfolder => source_dir.join(format.output_extension()),
        Destination::Custom {
            root: custom_root,
            per_format_subfolders,
        } => {
            let mut dir = custom_root.clone();
            if *per_format_subfolders {
                dir.push(format.output_extension());
            }
            // A dropped folder lands under its own name, so two dropped
            // folders that both contain `x.png` don't merge into one.
            if root.kind == RootKind::Folder
                && let Some(name) = root.path.file_name()
            {
                dir.push(name);
            }
            if let Some(relative_dir) = entry.relative_path.parent() {
                dir.push(relative_dir);
            }
            dir
        }
    }
}

/// The output's file name: the source basename, with any opt-in prefix and
/// suffix, and the output format's extension.
pub fn output_file_name(naming: &NamingRule, ctx: &NameContext<'_>) -> String {
    let stem = ctx
        .entry
        .path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    format!(
        "{}{stem}{}.{}",
        render(&naming.prefix, naming, ctx),
        render(&naming.suffix, naming, ctx),
        ctx.format.output_extension()
    )
}

fn render(template: &str, naming: &NamingRule, ctx: &NameContext<'_>) -> String {
    // Templates are validated when presets load; an invalid one renders empty
    // rather than producing a surprising name.
    let segments = parse_template(template).unwrap_or_default();
    segments
        .iter()
        .map(|segment| match segment {
            Segment::Literal(text) => text.clone(),
            Segment::Token(Token::Format) => ctx.format.output_extension().to_owned(),
            Segment::Token(Token::OriginalExt) => {
                crate::extension::extension_of(&ctx.entry.path).unwrap_or_default()
            }
            Segment::Token(Token::Date) => {
                format_date(date_for(naming.date_source, ctx), naming.date_style)
            }
            Segment::Token(Token::Counter) => {
                format!("{:0width$}", ctx.counter, width = ctx.counter_width)
            }
        })
        .collect()
}

fn date_for(source: DateSource, ctx: &NameContext<'_>) -> NaiveDate {
    match source {
        DateSource::BatchStart => ctx.batch_date,
        DateSource::SourceModified => ctx
            .entry
            .modified
            .map(|time| DateTime::<Local>::from(time).date_naive())
            .unwrap_or(ctx.batch_date),
    }
}

fn format_date(date: NaiveDate, style: DateStyle) -> String {
    let pattern = match style {
        DateStyle::Compact => "%Y%m%d",
        DateStyle::Short => "%y%m%d",
        DateStyle::Iso => "%Y-%m-%d",
    };
    date.format(pattern).to_string()
}

/// A path's identity for collision checks.
///
/// Rebuilding the path from its components normalises separators (`/` and
/// `\` are the same on Windows) and drops `.` segments, so one file can't hide
/// behind two spellings. Windows and macOS file systems are case-insensitive
/// by default, so `A.webp` and `a.webp` are one file there.
pub fn collision_key(path: &Path) -> String {
    let normalized: PathBuf = path.components().collect();
    let text = normalized.to_string_lossy();
    if cfg!(any(windows, target_os = "macos")) {
        text.to_lowercase()
    } else {
        text.into_owned()
    }
}

/// `name.ext` → `name (n).ext`, for the rename collision policy.
pub fn numbered(path: &Path, n: u32) -> PathBuf {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let name = match path.extension() {
        Some(ext) => format!("{stem} ({n}).{}", ext.to_string_lossy()),
        None => format!("{stem} ({n})"),
    };
    path.with_file_name(name)
}

/// Digits needed to write `count` (at least one).
pub fn digits(count: usize) -> usize {
    count.max(1).to_string().len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inspect::Inspection;

    fn entry(path: &str, relative: &str) -> SourceEntry {
        SourceEntry {
            root: 0,
            path: PathBuf::from(path),
            relative_path: PathBuf::from(relative),
            size: 1,
            modified: None,
            declared: None,
            inspection: Inspection::default(),
            excluded: false,
        }
    }

    fn folder_root(path: &str) -> SourceRoot {
        SourceRoot {
            path: PathBuf::from(path),
            kind: RootKind::Folder,
        }
    }

    fn ctx<'a>(entry: &'a SourceEntry, counter: usize, width: usize) -> NameContext<'a> {
        NameContext {
            entry,
            format: Format::Webp,
            counter,
            counter_width: width,
            batch_date: NaiveDate::from_ymd_opt(2026, 10, 15).unwrap(),
        }
    }

    #[test]
    fn default_naming_only_changes_the_extension() {
        let e = entry("/in/My Photo.JPG", "My Photo.JPG");
        assert_eq!(
            output_file_name(&NamingRule::default(), &ctx(&e, 1, 1)),
            "My Photo.webp"
        );
    }

    #[test]
    fn tokens_render_with_padding_and_date_styles() {
        let e = entry("/in/shot.jpg", "shot.jpg");
        let naming = NamingRule {
            prefix: "{counter}_".into(),
            suffix: "_{date}_{original_ext}-{format}".into(),
            date_style: DateStyle::Short,
            ..NamingRule::default()
        };
        assert_eq!(
            output_file_name(&naming, &ctx(&e, 7, 3)),
            "007_shot_261015_jpg-webp.webp"
        );
    }

    /// Regression R2: same names in sibling folders stay apart.
    #[test]
    fn r02_custom_destination_keeps_hierarchy_under_the_dropped_folder_name() {
        let root = folder_root("/photos/2024");
        let destination = Destination::Custom {
            root: PathBuf::from("/out"),
            per_format_subfolders: true,
        };
        let a = entry("/photos/2024/sub1/x.jpg", "sub1/x.jpg");
        let b = entry("/photos/2024/sub2/x.jpg", "sub2/x.jpg");
        assert_eq!(
            output_dir(&a, &root, &destination, Format::Webp),
            Path::new("/out/webp/2024/sub1")
        );
        assert_eq!(
            output_dir(&b, &root, &destination, Format::Webp),
            Path::new("/out/webp/2024/sub2")
        );
    }

    #[test]
    fn dropped_file_goes_straight_into_a_custom_root() {
        let root = SourceRoot {
            path: PathBuf::from("/in/a.png"),
            kind: RootKind::File,
        };
        let e = entry("/in/a.png", "a.png");
        let destination = Destination::Custom {
            root: PathBuf::from("/out"),
            per_format_subfolders: false,
        };
        assert_eq!(
            output_dir(&e, &root, &destination, Format::Jxl),
            Path::new("/out")
        );
    }

    #[test]
    fn subfolder_destination_uses_the_output_extension() {
        let e = entry("/in/a.png", "a.png");
        let dir = output_dir(
            &e,
            &folder_root("/in"),
            &Destination::Subfolder,
            Format::Jpeg,
        );
        assert_eq!(dir, Path::new("/in/jpg"));
    }

    #[test]
    fn numbering_inserts_before_the_extension() {
        assert_eq!(
            numbered(Path::new("/o/a.webp"), 2),
            Path::new("/o/a (2).webp")
        );
    }

    #[test]
    fn digits_cover_the_batch_size() {
        assert_eq!(digits(0), 1);
        assert_eq!(digits(9), 1);
        assert_eq!(digits(120), 3);
    }
}
