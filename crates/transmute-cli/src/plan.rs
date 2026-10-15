//! `transmute-dev plan`: what a preset would do to some files, without
//! writing anything.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use transmute_backends::discovery::ToolConfig;
use transmute_backends::probe::{ToolReport, discover_tools, toolbox};
use transmute_core::inventory::{Inventory, ScanOptions, scan};
use transmute_core::plan::{JobStatus, Plan, PlanContext, plan};
use transmute_core::preset::{JobConfig, Preset, UserDefaults, resolve};

use crate::preset::read_document;

pub struct PlanArgs {
    pub inputs: Vec<PathBuf>,
    pub preset: PathBuf,
    pub defaults: Option<PathBuf>,
    pub recursive: bool,
    pub list: bool,
}

#[derive(Clone, Copy)]
pub enum JobListing {
    None,
    Preview,
    All,
}

/// How many jobs to show without `--list`.
const PREVIEW_JOBS: usize = 15;

/// A plan plus everything needed to run it.
pub struct Prepared {
    pub plan: Plan,
    pub inventory: Inventory,
    pub config: JobConfig,
    pub preset_name: String,
    pub reports: Vec<ToolReport>,
}

/// Loads the preset, scans the inputs, discovers tools and plans.
pub fn prepare(args: &PlanArgs) -> Result<Prepared, String> {
    let preset = read_document::<Preset>(&args.preset)?;
    let defaults = match &args.defaults {
        Some(path) => read_document::<UserDefaults>(path)?,
        None => UserDefaults::default(),
    };
    let config = resolve(&defaults, &preset);
    let inventory = scan(
        &args.inputs,
        ScanOptions {
            recursive: args.recursive,
        },
        &|_| {},
        &AtomicBool::new(false),
    )
    .map_err(|err| err.to_string())?;
    let reports = discover_tools(&ToolConfig::default());

    let exists = |path: &Path| path.exists();
    let plan = plan(
        &inventory,
        &config,
        &toolbox(&reports),
        &PlanContext {
            batch_date: chrono::Local::now().date_naive(),
            output_exists: &exists,
        },
    );
    Ok(Prepared {
        plan,
        inventory,
        config,
        preset_name: preset.name,
        reports,
    })
}

pub fn run(args: PlanArgs) -> Result<(), String> {
    let prepared = prepare(&args)?;
    print!(
        "{}",
        describe(
            &prepared.plan,
            &prepared.inventory,
            &prepared.config,
            &prepared.preset_name,
            if args.list {
                JobListing::All
            } else {
                JobListing::Preview
            }
        )
    );
    Ok(())
}

pub fn describe(
    plan: &Plan,
    inventory: &Inventory,
    config: &JobConfig,
    preset: &str,
    jobs_shown: JobListing,
) -> String {
    let mut out = String::new();
    let counts = plan.counts();
    let sources = plan
        .jobs
        .iter()
        .map(|j| j.source)
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    // Writing to a String cannot fail.
    let _ = writeln!(
        out,
        "Preset \"{preset}\": {sources} source(s) × {} output(s) = {} job(s)",
        config.targets.len(),
        plan.jobs.len()
    );
    let _ = writeln!(
        out,
        "  ready {}, skipped {}, blocked {}",
        counts.ready, counts.skipped, counts.blocked
    );
    if !plan.previous_outputs.is_empty() {
        let _ = writeln!(
            out,
            "  {} file(s) are outputs of an earlier run of this preset and aren't converted again",
            plan.previous_outputs.len()
        );
    }
    if plan.filtered_out > 0 || !plan.unsupported.is_empty() {
        let _ = writeln!(
            out,
            "  not planned: {} left out by the input filter, {} not convertible",
            plan.filtered_out,
            plan.unsupported.len()
        );
    }

    // Problems first, grouped, so a 10,000-file plan stays readable.
    let mut problems: BTreeMap<String, usize> = BTreeMap::new();
    let mut notes: BTreeMap<String, usize> = BTreeMap::new();
    for job in &plan.jobs {
        match &job.status {
            JobStatus::Blocked(reason) => {
                *problems.entry(format!("blocked: {reason}")).or_default() += 1
            }
            JobStatus::Skipped(reason) => {
                *problems.entry(format!("skipped: {reason}")).or_default() += 1;
            }
            JobStatus::Ready => {}
        }
        for note in &job.notes {
            *notes.entry(note.to_string()).or_default() += 1;
        }
    }
    for (title, entries) in [("Problems", &problems), ("Notes", &notes)] {
        if !entries.is_empty() {
            let _ = writeln!(out, "\n{title}");
            for (message, count) in entries {
                let _ = writeln!(out, "  {count:>6} × {message}");
            }
        }
    }

    let shown = match jobs_shown {
        JobListing::None => return out,
        JobListing::Preview => PREVIEW_JOBS.min(plan.jobs.len()),
        JobListing::All => plan.jobs.len(),
    };
    let _ = writeln!(out, "\nJobs");
    for job in &plan.jobs[..shown] {
        let source = &inventory.entries[job.source].relative_path;
        let status = match &job.status {
            JobStatus::Ready if job.replaces_existing => "REPLACES".to_owned(),
            JobStatus::Ready => "ready".to_owned(),
            JobStatus::Skipped(_) => "skipped".to_owned(),
            JobStatus::Blocked(_) => "BLOCKED".to_owned(),
        };
        let _ = writeln!(
            out,
            "  {status:<8} {} → {}",
            source.display(),
            job.output.display()
        );
        if let Some(route) = &job.route {
            let _ = writeln!(out, "           via {}", route.reason);
        }
    }
    if shown < plan.jobs.len() {
        let _ = writeln!(
            out,
            "  … {} more (use --list to show all)",
            plan.jobs.len() - shown
        );
    }
    out
}
