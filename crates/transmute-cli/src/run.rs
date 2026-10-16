//! `transmute-dev run`: plan, then convert for real.

use std::num::NonZeroUsize;
use std::sync::Arc;

use tokio::sync::mpsc;

use transmute_backends::invoke::ToolInvoker;
use transmute_engine::concurrency::resolve_worker_count;
use transmute_engine::run::{BatchProgress, Event, JobOutcome, RunControl, RunOptions, Work, run};

use crate::plan::{JobListing, PlanArgs, describe, prepare};

pub fn execute(args: PlanArgs, workers: Option<NonZeroUsize>) -> Result<(), String> {
    let prepared = prepare(&args)?;
    print!(
        "{}",
        describe(
            &prepared.plan,
            &prepared.inventory,
            &prepared.config,
            &prepared.preset_name,
            JobListing::None
        )
    );
    if prepared.plan.counts().ready == 0 {
        println!("\nNothing to convert.");
        return Ok(());
    }
    println!();

    let invoker = Arc::new(ToolInvoker::from_reports(&prepared.reports));
    let work = Arc::new(Work {
        plan: prepared.plan,
        inventory: prepared.inventory,
        config: prepared.config,
    });
    // -j wins over the stored setting.
    let workers = workers.or_else(|| prepared.stored.settings().workers);
    let options = RunOptions {
        state_dir: prepared.stored.locations.state.clone(),
        workers: resolve_worker_count(workers),
    };

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|err| format!("could not start the async runtime: {err}"))?;
    let summary = runtime.block_on(async {
        let control = RunControl::default();
        let on_interrupt = control.clone();
        tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                eprintln!("\nCancelling…");
                on_interrupt.cancel();
            }
        });

        let (events, mut receiver) = mpsc::unbounded_channel();
        let printer_work = work.clone();
        let printer = tokio::spawn(async move {
            while let Some(event) = receiver.recv().await {
                print_event(&printer_work, &event);
            }
        });
        let result = run(work, invoker, options, control, events).await;
        let _ = printer.await;
        result
    });
    let summary = summary.map_err(|err| format!("the run could not start: {err}"))?;
    println!("\n{}", describe_summary(&summary));
    if summary.failed > 0 {
        return Err(format!("{} job(s) failed", summary.failed));
    }
    Ok(())
}

fn print_event(work: &Work, event: &Event) {
    let Event::JobFinished { job, result } = event else {
        return;
    };
    let planned = &work.plan.jobs[*job];
    let source = &work.inventory.entries[planned.source].relative_path;
    let output = planned
        .output
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();
    match &result.outcome {
        JobOutcome::Converted {
            input_bytes,
            output_bytes,
            lossless_choice,
        } => println!(
            "  ✓ {} → {output}  {}  {:.2}s{}",
            source.display(),
            size_change(*input_bytes, *output_bytes),
            result.elapsed.as_secs_f64(),
            lossless_choice.map_or_else(String::new, |choice| format!(
                "  (auto: kept {}; lossy {} B, lossless {} B)",
                if choice.kept_lossless {
                    "lossless"
                } else {
                    "lossy"
                },
                choice.lossy_bytes,
                choice.lossless_bytes
            ))
        ),
        JobOutcome::Failed { message } => {
            println!("  ✗ {} → {output}: {message}", source.display())
        }
        JobOutcome::Cancelled => println!("  – {} → {output}: cancelled", source.display()),
    }
}

fn describe_summary(summary: &BatchProgress) -> String {
    let mut text = format!(
        "Done in {:.1}s: {} converted, {} failed, {} cancelled",
        summary.elapsed.as_secs_f64(),
        summary.converted,
        summary.failed,
        summary.cancelled
    );
    if summary.converted > 0 {
        text.push_str(&format!(
            " — {}",
            size_change(summary.input_bytes, summary.output_bytes)
        ));
    }
    text
}

fn size_change(input: u64, output: u64) -> String {
    let percent = if input == 0 {
        0.0
    } else {
        (output as f64 - input as f64) * 100.0 / input as f64
    };
    format!("{} → {} ({percent:+.0}%)", human(input), human(output))
}

fn human(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_read_naturally() {
        assert_eq!(human(512), "512 B");
        assert_eq!(human(1536), "1.5 KB");
        assert_eq!(size_change(2048, 1024), "2.0 KB → 1.0 KB (-50%)");
    }
}
