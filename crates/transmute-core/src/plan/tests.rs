use std::collections::HashSet;
use std::path::{Path, PathBuf};

use proptest::prelude::*;

use super::*;
use crate::capability::{Capabilities, MetadataKinds, MetadataSupport};
use crate::codec::CodecSettings;
use crate::inspect::{Animation, Inspection, MetadataPresence, Presence};
use crate::inventory::{RootKind, SourceEntry, SourceRoot};
use crate::naming::NamingRule;
use crate::output::Destination;
use crate::preset::{InputFilter, ResolvedTarget};

/// Absolute paths that work on every platform's test runner.
fn abs(path: &str) -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(format!("C:{}", path.replace('/', "\\")))
    } else {
        PathBuf::from(path)
    }
}

fn no_metadata() -> MetadataPresence {
    MetadataPresence {
        exif: Presence::Absent,
        xmp: Presence::Absent,
        icc: Presence::Absent,
    }
}

/// An inventory of one dropped folder `root` containing `files` (relative).
fn folder(root: &str, files: &[(&str, Format)]) -> Inventory {
    let root_path = abs(root);
    Inventory {
        roots: vec![SourceRoot {
            path: root_path.clone(),
            kind: RootKind::Folder,
        }],
        entries: files
            .iter()
            .map(|&(relative, format)| SourceEntry {
                root: 0,
                path: root_path.join(relative),
                relative_path: PathBuf::from(relative),
                size: 100,
                modified: None,
                declared: Format::from_extension(
                    Path::new(relative)
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or(""),
                ),
                inspection: Inspection {
                    content: Content::Image(format),
                    dimensions: Some((10, 10)),
                    animation: Animation::Still,
                    pages: None,
                    metadata: no_metadata(),
                    problem: None,
                },
                excluded: false,
            })
            .collect(),
        warnings: Vec::new(),
    }
}

/// A capable toolbox: vips reads and writes everything.
fn tools() -> Toolbox {
    let all: Vec<Format> = Format::ALL.to_vec();
    Toolbox::new([(
        ToolId::Vips,
        Capabilities {
            reads: all.iter().copied().collect(),
            writes: all.iter().copied().collect(),
            metadata: Some(MetadataSupport {
                preserves: MetadataKinds::ALL,
                can_strip: true,
                selective: true,
            }),
            ..Capabilities::default()
        },
    )])
}

fn target(format: Format) -> ResolvedTarget {
    ResolvedTarget {
        settings: CodecSettings::factory(format),
        destination: Destination::BesideSource,
        naming: NamingRule::default(),
        metadata: MetadataPolicy::PreserveAll,
    }
}

fn config(targets: Vec<ResolvedTarget>, collision: CollisionPolicy) -> JobConfig {
    JobConfig {
        input_filter: InputFilter::All,
        collision,
        targets,
    }
}

fn run(inventory: &Inventory, config: &JobConfig, existing: &[PathBuf]) -> Plan {
    let existing: HashSet<String> = existing.iter().map(|p| collision_key(p)).collect();
    let exists = move |path: &Path| existing.contains(&collision_key(path));
    plan(
        inventory,
        config,
        &tools(),
        &PlanContext {
            batch_date: NaiveDate::from_ymd_opt(2026, 10, 15).unwrap(),
            output_exists: &exists,
        },
    )
}

fn names(plan: &Plan) -> Vec<String> {
    plan.jobs
        .iter()
        .map(|j| j.output.file_name().unwrap().to_string_lossy().into_owned())
        .collect()
}

#[test]
fn plans_every_source_for_every_target() {
    let inventory = folder("/in", &[("a.png", Format::Png), ("b.jpg", Format::Jpeg)]);
    let plan = run(
        &inventory,
        &config(
            vec![target(Format::Webp), target(Format::Jxl)],
            CollisionPolicy::Skip,
        ),
        &[],
    );
    assert_eq!(names(&plan), ["a.webp", "a.jxl", "b.webp", "b.jxl"]);
    assert_eq!(plan.counts().ready, 4);
    assert_eq!(plan.jobs[0].output, abs("/in/a.webp"));
}

/// Regression R1: same basename, different source extensions → blocked
/// before anything runs, not a race.
#[test]
fn r01_same_basename_different_extension_is_blocked() {
    let inventory = folder("/in", &[("a.jpg", Format::Jpeg), ("a.png", Format::Png)]);
    let plan = run(
        &inventory,
        &config(vec![target(Format::Webp)], CollisionPolicy::Skip),
        &[],
    );
    assert_eq!(plan.counts().blocked, 2, "neither job silently wins");
    assert!(matches!(
        &plan.jobs[0].status,
        JobStatus::Blocked(BlockReason::DuplicateOutput { others }) if others == &[1]
    ));
}

#[test]
fn r01_rename_policy_resolves_duplicates() {
    let inventory = folder("/in", &[("a.jpg", Format::Jpeg), ("a.png", Format::Png)]);
    let plan = run(
        &inventory,
        &config(vec![target(Format::Webp)], CollisionPolicy::Rename),
        &[],
    );
    assert_eq!(names(&plan), ["a.webp", "a (2).webp"]);
    assert_eq!(plan.counts().ready, 2);
}

#[test]
fn r01_original_ext_suffix_resolves_duplicates() {
    let inventory = folder("/in", &[("a.jpg", Format::Jpeg), ("a.png", Format::Png)]);
    let mut webp = target(Format::Webp);
    webp.naming.suffix = "_{original_ext}".into();
    let plan = run(&inventory, &config(vec![webp], CollisionPolicy::Skip), &[]);
    assert_eq!(names(&plan), ["a_jpg.webp", "a_png.webp"]);
}

/// Regression R2, at the plan level: sibling folders stay apart under a
/// custom destination, so nothing collides.
#[test]
fn r02_sibling_folders_do_not_collide_in_a_custom_destination() {
    let inventory = folder(
        "/photos",
        &[("sub1/x.jpg", Format::Jpeg), ("sub2/x.jpg", Format::Jpeg)],
    );
    let mut webp = target(Format::Webp);
    webp.destination = Destination::Custom {
        root: abs("/out"),
        per_format_subfolders: false,
    };
    let plan = run(&inventory, &config(vec![webp], CollisionPolicy::Skip), &[]);
    assert_eq!(plan.counts().ready, 2);
    assert_eq!(plan.jobs[0].output, abs("/out/photos/sub1/x.webp"));
    assert_eq!(plan.jobs[1].output, abs("/out/photos/sub2/x.webp"));
}

/// Regression R3: 1.x let `-o single.png` swallow a whole folder. The planner
/// has no single-output mode at all; every source gets its own output.
#[test]
fn r03_every_source_of_a_folder_gets_a_distinct_output() {
    let files: Vec<(String, Format)> = (0..50)
        .map(|i| (format!("img{i}.png"), Format::Png))
        .collect();
    let refs: Vec<(&str, Format)> = files.iter().map(|(n, f)| (n.as_str(), *f)).collect();
    let plan = run(
        &folder("/in", &refs),
        &config(vec![target(Format::Jpeg)], CollisionPolicy::Skip),
        &[],
    );
    let distinct: HashSet<_> = plan.jobs.iter().map(|j| collision_key(&j.output)).collect();
    assert_eq!(distinct.len(), 50);
}

/// Regression R4: a bare file name (no folder part) converts beside itself.
/// 1.x crashed on the empty parent folder.
#[test]
fn r04_bare_file_name_converts_beside_it() {
    let mut inventory = folder("/unused", &[("photo.jpg", Format::Jpeg)]);
    inventory.roots[0] = SourceRoot {
        path: PathBuf::from("photo.jpg"),
        kind: RootKind::File,
    };
    inventory.entries[0].path = PathBuf::from("photo.jpg");
    let plan = run(
        &inventory,
        &config(vec![target(Format::Webp)], CollisionPolicy::Skip),
        &[],
    );
    let output = &plan.jobs[0].output;
    assert!(output.is_absolute(), "{}", output.display());
    assert_eq!(
        output.parent(),
        Some(std::env::current_dir().unwrap().as_path())
    );
    assert_eq!(output.file_name().unwrap(), "photo.webp");
}

/// Regression R11: re-encoding to the same format with default naming would
/// overwrite the source. That's refused, whatever the policy (except rename).
#[test]
fn r11_output_that_would_replace_its_source_is_refused() {
    let inventory = folder("/in", &[("photo.jpg", Format::Jpeg)]);
    for policy in [CollisionPolicy::Skip, CollisionPolicy::Overwrite] {
        let plan = run(&inventory, &config(vec![target(Format::Jpeg)], policy), &[]);
        assert!(
            matches!(
                plan.jobs[0].status,
                JobStatus::Blocked(BlockReason::WouldReplaceSource { .. })
            ),
            "{policy:?}"
        );
    }
    let renamed = run(
        &inventory,
        &config(vec![target(Format::Jpeg)], CollisionPolicy::Rename),
        &[],
    );
    assert_eq!(names(&renamed), ["photo (2).jpg"]);
}

#[test]
fn output_landing_on_a_different_source_is_refused() {
    // a.png → a.jpg would replace the batch's own a.jpg.
    let inventory = folder("/in", &[("a.jpg", Format::Jpeg), ("a.png", Format::Png)]);
    let plan = run(
        &inventory,
        &config(vec![target(Format::Jpeg)], CollisionPolicy::Overwrite),
        &[],
    );
    assert!(matches!(
        plan.jobs[1].status,
        JobStatus::Blocked(BlockReason::WouldReplaceSource { .. })
    ));
}

#[test]
fn existing_outputs_follow_the_collision_policy() {
    let inventory = folder("/in", &[("a.png", Format::Png)]);
    let existing = [abs("/in/a.webp")];
    let cfg = |policy| config(vec![target(Format::Webp)], policy);

    let skip = run(&inventory, &cfg(CollisionPolicy::Skip), &existing);
    assert_eq!(
        skip.jobs[0].status,
        JobStatus::Skipped(SkipReason::OutputExists)
    );

    let overwrite = run(&inventory, &cfg(CollisionPolicy::Overwrite), &existing);
    assert_eq!(overwrite.jobs[0].status, JobStatus::Ready);
    assert!(overwrite.jobs[0].replaces_existing);

    let rename = run(&inventory, &cfg(CollisionPolicy::Rename), &existing);
    assert_eq!(names(&rename), ["a (2).webp"]);
    assert!(
        rename.jobs[0]
            .notes
            .iter()
            .any(|n| matches!(n, PlanNote::Renamed { .. }))
    );
}

#[test]
fn collisions_ignore_case_where_the_file_system_does() {
    let inventory = folder("/in", &[("A.png", Format::Png), ("a.gif", Format::Gif)]);
    let plan = run(
        &inventory,
        &config(vec![target(Format::Webp)], CollisionPolicy::Skip),
        &[],
    );
    let expected_blocked = if cfg!(any(windows, target_os = "macos")) {
        2
    } else {
        0
    };
    assert_eq!(plan.counts().blocked, expected_blocked);
}

#[test]
fn filters_unsupported_and_excluded_entries_are_accounted_for() {
    let mut inventory = folder(
        "/in",
        &[
            ("a.png", Format::Png),
            ("b.gif", Format::Gif),
            ("c.jpg", Format::Jpeg),
            ("d.png", Format::Png),
        ],
    );
    inventory.entries[2].inspection.content = Content::Unknown;
    inventory.entries[3].excluded = true;
    let mut cfg = config(vec![target(Format::Webp)], CollisionPolicy::Skip);
    cfg.input_filter = InputFilter::Exclude {
        formats: vec![Format::Gif],
    };
    let plan = run(&inventory, &cfg, &[]);
    assert_eq!(names(&plan), ["a.webp"]);
    assert_eq!(plan.filtered_out, 1);
    assert_eq!(plan.unsupported, [2]);
}

#[test]
fn counter_is_per_source_and_shared_across_targets() {
    let inventory = folder("/in", &[("a.png", Format::Png), ("b.png", Format::Png)]);
    let numbered = |format| {
        let mut t = target(format);
        t.naming.prefix = "{counter}_".into();
        t
    };
    let plan = run(
        &inventory,
        &config(
            vec![numbered(Format::Webp), numbered(Format::Jxl)],
            CollisionPolicy::Skip,
        ),
        &[],
    );
    assert_eq!(names(&plan), ["1_a.webp", "1_a.jxl", "2_b.webp", "2_b.jxl"]);
}

#[test]
fn missing_tools_block_with_a_reason() {
    let inventory = folder("/in", &[("a.png", Format::Png)]);
    let cfg = config(vec![target(Format::Webp)], CollisionPolicy::Skip);
    let exists = |_: &Path| false;
    let plan = plan(
        &inventory,
        &cfg,
        &Toolbox::default(),
        &PlanContext {
            batch_date: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            output_exists: &exists,
        },
    );
    let JobStatus::Blocked(reason) = &plan.jobs[0].status else {
        panic!("expected blocked");
    };
    assert_eq!(
        reason.to_string(),
        "no installed tool can convert PNG to WebP"
    );
}

#[test]
fn extension_mismatch_and_damage_are_noted() {
    let mut inventory = folder("/in", &[("fake.jpg", Format::Png)]);
    inventory.entries[0].inspection.problem = Some("the file is truncated".into());
    // AVIF has no specialist encoder, so only the source notes appear.
    let plan = run(
        &inventory,
        &config(vec![target(Format::Avif)], CollisionPolicy::Skip),
        &[],
    );
    let notes: Vec<String> = plan.jobs[0].notes.iter().map(ToString::to_string).collect();
    assert_eq!(
        notes,
        [
            "the source may be damaged: the file is truncated",
            "the source's extension says JPEG but it is really PNG"
        ]
    );
}

// Property: whatever the files, targets, policy and pre-existing outputs,
// no two ready jobs share an output, no ready job writes over a source, and
// existing files are only replaced under the overwrite policy.

fn arb_format() -> impl Strategy<Value = Format> {
    prop::sample::select(Format::ALL.to_vec())
}

fn arb_policy() -> impl Strategy<Value = CollisionPolicy> {
    prop::sample::select(vec![
        CollisionPolicy::Skip,
        CollisionPolicy::Rename,
        CollisionPolicy::Overwrite,
    ])
}

fn arb_files() -> impl Strategy<Value = Vec<(String, Format)>> {
    // Few names and folders, mixed case: plenty of opportunities to clash.
    let name = prop::sample::select(vec!["a", "A", "b", "photo", "Photo"]);
    let dir = prop::sample::select(vec!["", "sub/", "Sub/", "x/y/"]);
    prop::collection::vec((dir, name, arb_format()), 1..12).prop_map(|files| {
        let mut seen = HashSet::new();
        files
            .into_iter()
            .map(|(dir, name, format)| {
                (format!("{dir}{name}.{}", format.output_extension()), format)
            })
            // A folder can't hold two files whose names differ only in case
            // on case-insensitive systems; keep the inventory realistic.
            .filter(|(path, _)| seen.insert(collision_key(Path::new(path))))
            .collect()
    })
}

fn arb_target() -> impl Strategy<Value = ResolvedTarget> {
    let destination = prop::sample::select(vec![
        Destination::BesideSource,
        Destination::Subfolder,
        Destination::Custom {
            root: abs("/out"),
            per_format_subfolders: false,
        },
    ]);
    let suffix = prop::sample::select(vec!["", "_{original_ext}", "_{counter}"]);
    (arb_format(), destination, suffix).prop_map(|(format, destination, suffix)| {
        let mut t = target(format);
        t.destination = destination;
        t.naming.suffix = suffix.into();
        t
    })
}

proptest! {
    #[test]
    fn ready_jobs_never_share_or_overwrite(
        files in arb_files(),
        targets in prop::collection::vec(arb_target(), 1..4),
        policy in arb_policy(),
        existing_mask in prop::collection::vec(any::<bool>(), 48),
    ) {
        let refs: Vec<(&str, Format)> = files.iter().map(|(n, f)| (n.as_str(), *f)).collect();
        let inventory = folder("/in", &refs);
        let cfg = config(targets, policy);

        // Mark some of the would-be outputs as already existing on disk.
        let first_pass = run(&inventory, &cfg, &[]);
        let existing: Vec<PathBuf> = first_pass.jobs.iter().zip(existing_mask.iter().cycle())
            .filter(|(_, exists)| **exists)
            .map(|(job, _)| job.output.clone())
            .collect();
        let existing_keys: HashSet<String> = existing.iter().map(|p| collision_key(p)).collect();
        let plan = run(&inventory, &cfg, &existing);

        let sources: HashSet<String> = inventory.entries.iter().map(|e| collision_key(&e.path)).collect();
        let mut outputs = HashSet::new();
        for job in plan.jobs.iter().filter(|j| j.status == JobStatus::Ready) {
            let key = collision_key(&job.output);
            prop_assert!(outputs.insert(key.clone()), "two ready jobs write {}", job.output.display());
            prop_assert!(!sources.contains(&key), "ready job overwrites source {}", job.output.display());
            let exists = existing_keys.contains(&key);
            prop_assert_eq!(job.replaces_existing, exists);
            if exists {
                prop_assert_eq!(policy, CollisionPolicy::Overwrite);
            }
        }
        prop_assert_eq!(plan.jobs.len(), files.len() * cfg.targets.len());
    }
}

/// Source protection must not depend on how a path is spelled: on Windows
/// `C:\in\sub/b.jpg` and `C:\in\sub\b.jpg` are the same file.
#[test]
fn source_protection_survives_mixed_separators() {
    let inventory = folder("/in", &[("sub/b.jpg", Format::Jpeg)]);
    let plan = run(
        &inventory,
        &config(vec![target(Format::Jpeg)], CollisionPolicy::Overwrite),
        &[],
    );
    assert!(
        matches!(
            plan.jobs[0].status,
            JobStatus::Blocked(BlockReason::WouldReplaceSource { .. })
        ),
        "{:?}",
        plan.jobs[0].status
    );
}
