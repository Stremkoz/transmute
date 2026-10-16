//! M11's exit test: a whole session driven through the facade only, the
//! way the desktop app will drive it. Stub tools stand in for the encoders
//! (they copy their input), so this runs everywhere, CI included.
// Tests run from a console, where a plain Command opens no window.
#![allow(clippy::disallowed_methods)]
// Test-only file: helpers panic on setup failures, which is what a test wants.
#![allow(clippy::unwrap_used)]

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;

use transmute_api::{
    Api, ApiEvent, Backend, ErrorCode, FileFilter, FileInfo, JobFilter, JobState, NodeKind,
    PreviewTarget, RunState,
};
use transmute_backends::invoke::{Invocation, Invoker, StepRequest};
use transmute_core::capability::{Capabilities, Toolbox};
use transmute_core::format::Format;
use transmute_core::output::{Destination, OutputTarget};
use transmute_core::preset::Preset;
use transmute_core::settings::{AppSettings, Theme};
use transmute_core::store::Locations;
use transmute_core::tool::ToolId;

/// Writes a stub "encoder": `copy` copies its input to its output; `slow`
/// does the same after a long pause (to test cancelling).
fn write_stub(dir: &Path, slow: bool) -> PathBuf {
    let name = if slow { "slow" } else { "copy" };
    #[cfg(windows)]
    {
        let wait = if slow {
            "ping -n 30 127.0.0.1 >nul\r\n"
        } else {
            ""
        };
        let path = dir.join(format!("{name}.cmd"));
        fs::write(
            &path,
            format!("@echo off\r\n{wait}copy /y \"%~1\" \"%~2\" >nul\r\n"),
        )
        .unwrap();
        path
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        let wait = if slow { "sleep 30\n" } else { "" };
        let path = dir.join(name);
        fs::write(&path, format!("#!/bin/sh\n{wait}cp \"$1\" \"$2\"\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }
}

struct StubInvoker {
    program: PathBuf,
}

impl Invoker for StubInvoker {
    fn invocation(&self, request: &StepRequest<'_>) -> Result<Invocation, String> {
        Ok(Invocation {
            program: self.program.clone(),
            args: vec![
                OsString::from(request.input),
                OsString::from(request.output),
            ],
        })
    }
}

/// A 1×1 PNG, recognised by content detection.
fn png_bytes() -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    for (kind, data) in [
        (&b"IHDR"[..], &[0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0][..]),
        (b"IDAT", &[0x78, 0x9C, 0x63, 0x60, 0, 0, 0, 2, 0, 1]),
        (b"IEND", &[]),
    ] {
        bytes.extend_from_slice(&(data.len() as u32).to_be_bytes());
        bytes.extend_from_slice(kind);
        bytes.extend_from_slice(data);
        bytes.extend_from_slice(&[0; 4]);
    }
    bytes
}

struct Fixture {
    _dir: tempfile::TempDir,
    photos: PathBuf,
    api: Api,
    events: mpsc::UnboundedReceiver<ApiEvent>,
}

/// A session over a `photos` folder (a.png, b.png, sub/c.png, notes.txt),
/// with "magick" stubbed to "convert" PNG → TIFF (and back, for previews).
fn fixture(slow: bool) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let photos = dir.path().join("photos");
    fs::create_dir_all(photos.join("sub")).unwrap();
    for name in ["a.png", "b.png", "sub/c.png"] {
        fs::write(photos.join(name), png_bytes()).unwrap();
    }
    fs::write(photos.join("notes.txt"), "not an image").unwrap();
    let stubs = dir.path().join("stubs");
    fs::create_dir_all(&stubs).unwrap();

    let caps = Capabilities {
        reads: [Format::Png, Format::Tiff].into(),
        writes: [Format::Png, Format::Tiff].into(),
        ..Capabilities::default()
    };
    let backend = Backend {
        reports: Vec::new(),
        toolbox: Toolbox::new([(ToolId::Magick, caps)]),
        invoker: Arc::new(StubInvoker {
            program: write_stub(&stubs, slow),
        }),
    };
    let (sender, events) = mpsc::unbounded_channel();
    let api = Api::with_backend(
        Locations::portable(&dir.path().join("data")),
        backend,
        Arc::new(move |event| {
            let _ = sender.send(event);
        }),
        false,
    );
    Fixture {
        _dir: dir,
        photos,
        api,
        events,
    }
}

fn tiff_preset() -> Preset {
    Preset {
        name: "Archive".into(),
        description: None,
        input_filter: Default::default(),
        destination: Some(Destination::Subfolder),
        naming: None,
        metadata: None,
        collision: None,
        outputs: vec![OutputTarget::new(Format::Tiff)],
    }
}

/// Collects events until the run finishes.
async fn until_finished(events: &mut mpsc::UnboundedReceiver<ApiEvent>) -> Vec<ApiEvent> {
    let mut seen = Vec::new();
    loop {
        let event = tokio::time::timeout(Duration::from_secs(60), events.recv())
            .await
            .expect("the run should finish")
            .expect("the event sink stays open");
        let done = matches!(event, ApiEvent::RunFinished { .. });
        seen.push(event);
        if done {
            return seen;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_whole_session_through_the_facade() {
    let mut fx = fixture(false);
    let api = &fx.api;

    // Drop a folder.
    let inputs = api
        .add_inputs(vec![fx.photos.display().to_string()], true)
        .await
        .unwrap();
    assert_eq!(
        (inputs.files, inputs.convertible, inputs.unsupported),
        (4, 3, 1)
    );
    assert_eq!(inputs.roots.len(), 1);

    // Browse it lazily, and exclude a file.
    let top = api.tree(0, "").unwrap();
    let names: Vec<&str> = top.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(
        names,
        ["sub", "a.png", "b.png", "notes.txt"],
        "folders first"
    );
    let NodeKind::Folder { counts } = &top[0].node else {
        panic!("sub is a folder")
    };
    assert_eq!((counts.all, counts.excluded), (1, 0));
    let sub = api.tree(0, "sub").unwrap();
    assert_eq!(sub[0].relative_path, "sub/c.png");
    let NodeKind::File(FileInfo { entry: a_entry, .. }) = top[1].node else {
        panic!("a.png is a file")
    };
    assert_eq!(api.set_excluded(0, "b.png", true).unwrap().excluded, 1);

    // Configure, and plan.
    api.set_preset(tiff_preset()).unwrap();
    assert_eq!(
        api.jobs(JobFilter::All, 0, 10).unwrap_err().code,
        ErrorCode::NoPlan,
        "there's no plan until one is made"
    );
    let plan = api.plan().await.unwrap();
    assert_eq!((plan.jobs, plan.ready), (2, 2), "b.png is excluded");
    assert_eq!(api.overview().plan.unwrap().ready, 2);
    // One example per output, from its first file.
    assert_eq!(plan.examples.len(), 1);
    assert_eq!(plan.examples[0].source, "a.png");
    assert!(
        plan.examples[0].output.ends_with("a.tiff"),
        "{:?}",
        plan.examples
    );
    let page = api.jobs(JobFilter::Ready, 0, 1).unwrap();
    assert_eq!((page.total, page.jobs.len()), (2, 1), "paged");
    assert_eq!(page.jobs[0].format, Format::Tiff);

    // Run it, following the events.
    api.start_run().await.unwrap();
    let events = until_finished(&mut fx.events).await;
    let mut last_state: BTreeMap<u32, JobState> = BTreeMap::new();
    for event in &events {
        if let ApiEvent::JobsChanged { jobs } = event {
            for update in jobs {
                last_state.insert(update.id, update.state.clone());
            }
        }
    }
    assert_eq!(last_state.len(), 2);
    assert!(
        last_state
            .values()
            .all(|s| matches!(s, JobState::Converted { output_bytes, .. } if *output_bytes > 0)),
        "{last_state:?}"
    );
    let Some(ApiEvent::RunFinished { progress }) = events.last() else {
        panic!("ends with RunFinished")
    };
    assert_eq!(
        (progress.total, progress.converted, progress.failed),
        (2, 2, 0)
    );
    // Each output's totals come with every progress event, for its card.
    assert_eq!(progress.targets.len(), 1);
    assert_eq!(
        (progress.targets[0].converted, progress.targets[0].ready),
        (2, 0)
    );
    let during: Vec<u32> = events
        .iter()
        .filter_map(|e| match e {
            ApiEvent::Progress { progress } => progress.targets.first().map(|t| t.converted),
            _ => None,
        })
        .collect();
    assert!(!during.is_empty() && during.is_sorted(), "{during:?}");
    assert!(fx.photos.join("tiff").join("a.tiff").is_file());

    // Review: results stay browsable, outputs can be previewed.
    let overview = api.overview();
    let run = overview.run.unwrap();
    assert_eq!(run.state, RunState::Finished);
    assert_eq!(
        run.progress.targets, progress.targets,
        "a reload sees the same"
    );
    let converted = api.jobs(JobFilter::Converted, 0, 10).unwrap();
    assert_eq!(converted.total, 2);
    let source = api
        .preview(PreviewTarget::Source { entry: a_entry })
        .await
        .unwrap();
    assert!(!source.rendered, "PNG shows as it is");
    let output = api
        .preview(PreviewTarget::Output {
            job: converted.jobs[0].id,
        })
        .await
        .unwrap();
    assert!(output.rendered, "TIFF is rendered for the webview");
    assert!(Path::new(&output.path).is_file());
    assert!(api.report().unwrap().contains("2 converted"));

    // The plan has run; doing it again needs a fresh plan, which sees the
    // outputs that now exist.
    assert_eq!(api.start_run().await.unwrap_err().code, ErrorCode::NoPlan);
    let again = api.plan().await.unwrap();
    assert_eq!((again.ready, again.skipped), (0, 2));

    // A trial conversion before committing to a batch.
    let sample = api.sample(a_entry, 0).await.unwrap();
    assert!(sample.output_bytes > 0);
    assert!(sample.preview.rendered);

    // Presets: save the working configuration, then load it back.
    let id = api.save_preset(None, tiff_preset()).unwrap();
    assert_eq!(id, "archive");
    assert_eq!(api.stored_presets().unwrap().len(), 1);
    api.set_preset(Preset {
        name: "Other".into(),
        ..tiff_preset()
    })
    .unwrap();
    assert_eq!(api.load_preset("archive").unwrap().name, "Archive");
    assert_eq!(api.preset().name, "Archive");

    // Removing the input empties the session.
    assert_eq!(api.remove_root(0).await.unwrap().files, 0);
    api.clear_previews().unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn cancelling_through_the_facade() {
    let mut fx = fixture(true);
    let api = &fx.api;
    api.add_inputs(vec![fx.photos.display().to_string()], true)
        .await
        .unwrap();
    api.set_preset(tiff_preset()).unwrap();
    api.plan().await.unwrap();
    api.start_run().await.unwrap();

    // While running, the session can't change underneath the run.
    assert_eq!(
        api.set_preset(tiff_preset()).unwrap_err().code,
        ErrorCode::Busy
    );
    assert_eq!(api.plan().await.unwrap_err().code, ErrorCode::Busy);
    // Only the theme may change: the run keeps the tools it started with.
    let themed = AppSettings {
        theme: Some(Theme::Dark),
        ..api.settings()
    };
    api.set_settings(themed.clone()).await.unwrap();
    assert_eq!(api.settings().theme, themed.theme);
    let workers = AppSettings {
        workers: std::num::NonZeroUsize::new(2),
        ..themed
    };
    assert_eq!(
        api.set_settings(workers).await.unwrap_err().code,
        ErrorCode::Busy
    );
    api.pause().unwrap();
    assert_eq!(api.overview().run.unwrap().state, RunState::Paused);
    api.resume().unwrap();
    api.cancel().unwrap();

    let events = until_finished(&mut fx.events).await;
    let Some(ApiEvent::RunFinished { progress }) = events.last() else {
        panic!("ends with RunFinished")
    };
    assert_eq!(progress.converted, 0);
    let cancelled = api.jobs(JobFilter::Cancelled, 0, 10).unwrap();
    assert_eq!(cancelled.total, 3, "running and never-started jobs alike");
    assert!(!fx.photos.join("tiff").join("a.tiff").exists());
    assert_eq!(
        api.cancel().unwrap_err().code,
        ErrorCode::Invalid,
        "nothing to cancel now"
    );
}

/// The same path with the installed encoders: PNG → JPEG XL, then a
/// preview, which the webview can't decode so it must be rendered.
/// Opt-in: `TRANSMUTE_REAL_TOOLS=1`.
#[tokio::test(flavor = "multi_thread")]
async fn real_tools_convert_and_preview_jpeg_xl() {
    if std::env::var("TRANSMUTE_REAL_TOOLS").map_or(true, |v| v != "1") {
        eprintln!("skipped: set TRANSMUTE_REAL_TOOLS=1 to test against real encoders");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let photos = dir.path().join("photos");
    fs::create_dir_all(&photos).unwrap();
    let status = std::process::Command::new("magick")
        .args(["-size", "300x200", "plasma:fractal"])
        .arg(photos.join("p.png"))
        .status()
        .unwrap();
    assert!(status.success());

    let (sender, mut events) = mpsc::unbounded_channel();
    let api = Api::with_backend(
        Locations::portable(&dir.path().join("data")),
        Backend::discover(&Default::default()),
        Arc::new(move |event| {
            let _ = sender.send(event);
        }),
        false,
    );
    api.add_inputs(vec![photos.display().to_string()], true)
        .await
        .unwrap();
    api.set_preset(Preset {
        outputs: vec![OutputTarget::new(Format::Jxl)],
        ..tiff_preset()
    })
    .unwrap();
    assert_eq!(api.plan().await.unwrap().ready, 1);
    api.start_run().await.unwrap();
    until_finished(&mut events).await;
    let job = api.jobs(JobFilter::Converted, 0, 1).unwrap().jobs.remove(0);
    let preview = api
        .preview(PreviewTarget::Output { job: job.id })
        .await
        .unwrap();
    assert!(preview.rendered);
    assert_eq!((preview.width, preview.height), (Some(300), Some(200)));
    let rendered = fs::read(&preview.path).unwrap();
    assert!(rendered.starts_with(b"\x89PNG"), "the rendering is a PNG");

    // A thumbnail fits a 96 px box, and the second request is the cached one.
    let thumbnail = api.thumbnail(job.entry).await.unwrap();
    let small = transmute_core::inspect::inspect_path(Path::new(&thumbnail.path)).unwrap();
    assert_eq!(small.dimensions, Some((96, 64)), "{}", thumbnail.path);
    assert_eq!(api.thumbnail(job.entry).await.unwrap().path, thumbnail.path);
}

/// A session with no tools, for tests that only configure.
fn bare_session(data: &Path) -> Api {
    let backend = Backend {
        reports: Vec::new(),
        toolbox: Toolbox::new([]),
        invoker: Arc::new(StubInvoker {
            program: PathBuf::from("unused"),
        }),
    };
    Api::with_backend(Locations::portable(data), backend, Arc::new(|_| {}), false)
}

#[tokio::test(flavor = "multi_thread")]
async fn first_launch_has_no_outputs_and_the_working_preset_is_remembered() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");

    let api = bare_session(&data);
    assert!(api.preset().outputs.is_empty());
    let err = api.plan().await.unwrap_err();
    assert_eq!(err.code, ErrorCode::Invalid);
    assert!(err.message.contains("output"), "{}", err.message);

    // Outputs can be added and removed again: an empty draft is valid.
    api.set_preset(Preset {
        outputs: vec![OutputTarget::new(Format::Webp)],
        ..api.preset()
    })
    .unwrap();
    // But it can't be saved as a preset.
    let empty = Preset {
        outputs: Vec::new(),
        ..api.preset()
    };
    assert_eq!(
        api.save_preset(None, empty).unwrap_err().code,
        ErrorCode::Invalid
    );
    drop(api);

    // The next launch starts where this one left off.
    let api = bare_session(&data);
    let outputs = api.preset().outputs;
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0].codec.format(), Format::Webp);
    assert!(api.overview().notices.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_loaded_preset_and_its_edits_are_tracked() {
    let dir = tempfile::tempdir().unwrap();
    let data = dir.path().join("data");
    let api = bare_session(&data);
    assert_eq!(api.overview().loaded, None);

    // Save as: the working configuration becomes the loaded preset.
    api.set_preset(tiff_preset()).unwrap();
    let id = api
        .save_working(None, "Archive".into(), Some("Lossless copies".into()))
        .unwrap();
    let loaded = api.overview().loaded.unwrap();
    assert_eq!((loaded.id.as_str(), loaded.edited), (id.as_str(), false));
    assert_eq!(api.preset().description.as_deref(), Some("Lossless copies"));

    // An edit shows; loading it again undoes it.
    let mut edited = api.preset();
    edited.outputs.push(OutputTarget::new(Format::Webp));
    api.set_preset(edited.clone()).unwrap();
    assert!(api.overview().loaded.unwrap().edited);
    api.load_preset(&id).unwrap();
    assert!(!api.overview().loaded.unwrap().edited);
    assert_eq!(api.preset().outputs.len(), 1);

    // Save changes stores over the same id.
    api.set_preset(edited).unwrap();
    assert_eq!(
        api.save_working(Some(id.clone()), "Archive".into(), None)
            .unwrap(),
        id
    );
    assert!(!api.overview().loaded.unwrap().edited);
    assert_eq!(api.stored_presets().unwrap()[0].preset.outputs.len(), 2);

    // Renaming the stored preset is measured against from then on.
    let mut renamed = api.stored_presets().unwrap()[0].preset.clone();
    renamed.name = "Keepers".into();
    api.save_preset(Some(id.clone()), renamed.clone()).unwrap();
    let loaded = api.overview().loaded.unwrap();
    assert_eq!((loaded.name.as_str(), loaded.edited), ("Keepers", true));
    api.set_preset(renamed).unwrap();
    assert!(!api.overview().loaded.unwrap().edited);

    // Export and import round-trip through a file.
    let file = dir.path().join("shared.json");
    api.export_preset(&id, &file.display().to_string()).unwrap();
    let imported = api.import_preset(&file.display().to_string()).unwrap();
    assert_ne!(imported.id, id);
    assert_eq!(imported.preset.name, "Keepers");
    assert_eq!(api.stored_presets().unwrap().len(), 2);
    let err = api
        .import_preset(&dir.path().join("missing.json").display().to_string())
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::Store);

    // The next launch remembers which preset was loaded.
    drop(api);
    let api = bare_session(&data);
    let loaded = api.overview().loaded.unwrap();
    assert_eq!((loaded.id.as_str(), loaded.edited), (id.as_str(), false));

    // Deleting it leaves the working configuration, now unsaved.
    api.delete_preset(&id).unwrap();
    assert_eq!(api.overview().loaded, None);
    assert_eq!(api.preset().name, "Keepers");

    // Starting over: nothing stored, nothing picked.
    api.load_preset(&imported.id).unwrap();
    api.new_preset().unwrap();
    assert_eq!(api.overview().loaded, None);
    assert!(api.preset().outputs.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn codec_defaults_layer_the_users_over_the_factorys() {
    use transmute_core::codec::{CodecPatch, CodecSettings, WebpPatch};
    use transmute_core::preset::UserDefaults;

    let dir = tempfile::tempdir().unwrap();
    let api = bare_session(&dir.path().join("data"));
    api.set_defaults(UserDefaults {
        codecs: vec![CodecPatch::Webp(WebpPatch {
            quality: Some(70),
            ..WebpPatch::default()
        })],
        ..UserDefaults::default()
    })
    .unwrap();
    let codecs = api.codecs();
    assert_eq!(codecs.len(), Format::ALL.len());
    let webp = codecs.iter().find(|c| c.format == Format::Webp).unwrap();
    let (CodecSettings::Webp(factory), CodecSettings::Webp(inherited)) =
        (&webp.factory, &webp.inherited)
    else {
        panic!("not WebP settings");
    };
    assert_eq!((factory.quality, inherited.quality), (85, 70));
    assert_eq!(factory.method, inherited.method);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_files_list_follows_the_plan_and_the_run() {
    let mut fx = fixture(false);
    let api = &fx.api;
    let photos = fx.photos.display().to_string();
    api.add_inputs(vec![photos.clone()], true).await.unwrap();
    api.set_excluded(0, "b.png", true).unwrap();
    api.set_preset(tiff_preset()).unwrap();

    // Before planning, files carry no job states.
    let counts = api.file_counts();
    assert_eq!((counts.all, counts.excluded, counts.unsupported), (4, 1, 1));
    assert_eq!(counts.ready, 0);

    api.plan().await.unwrap();
    let counts = api.overview().counts;
    assert_eq!(counts.ready, 2, "a.png and sub/c.png");
    let top = api.tree(0, "").unwrap();
    let NodeKind::File(a) = &top[1].node else {
        panic!("a.png is a file")
    };
    assert_eq!((a.width, a.height), (Some(1), Some(1)));
    assert_eq!(a.outputs.len(), 1);
    assert_eq!(a.outputs[0].state, JobState::Ready);
    let NodeKind::Folder { counts: sub } = &top[0].node else {
        panic!("sub is a folder")
    };
    assert_eq!(sub.ready, 1);
    assert_eq!(api.overview().inputs.roots[0].counts.ready, 2);

    api.start_run().await.unwrap();
    until_finished(&mut fx.events).await;

    // After the run: every image is done, and the details say how.
    let counts = api.file_counts();
    assert_eq!((counts.done, counts.ready, counts.failed), (2, 0, 0));
    let done = api.files(FileFilter::Done, 0, 10);
    assert_eq!(done.total, 2);
    assert_eq!(done.files[1].relative_path, "sub/c.png");
    let details = api.entry(a.entry).unwrap();
    assert_eq!(details.name, "a.png");
    assert_eq!(details.jobs.len(), 1);
    assert!(matches!(details.jobs[0].state, JobState::Converted { .. }));
    let totals = api.run_totals().unwrap();
    assert_eq!((totals.len(), totals[0].converted), (1, 2));
    assert!(totals[0].output_bytes > 0);
    let output = api
        .locate(PreviewTarget::Output {
            job: details.jobs[0].id,
        })
        .unwrap();
    assert!(output.is_file());
    let report = fx.photos.join("report.txt");
    api.save_report(&report.display().to_string()).unwrap();
    assert!(fs::read_to_string(&report).unwrap().contains("2 converted"));
    fs::remove_file(&report).unwrap();

    // Clearing the finished files leaves the excluded image and the text
    // file; the input stays, as it still holds an image.
    let left = api.clear_finished().unwrap();
    assert_eq!((left.roots.len(), left.files), (1, 2));
    let names: Vec<String> = api
        .tree(0, "")
        .unwrap()
        .into_iter()
        .map(|n| n.name)
        .collect();
    assert_eq!(names, ["b.png", "notes.txt"]);
    // They stay cleared when another input is added (which rescans
    // everything, now finding the TIFF outputs too)...
    let top_names = |api: &Api| -> Vec<String> {
        api.tree(0, "")
            .unwrap()
            .into_iter()
            .map(|n| n.name)
            .collect()
    };
    let other = fx.photos.parent().unwrap().join("other.png");
    fs::write(&other, png_bytes()).unwrap();
    api.add_inputs(vec![other.display().to_string()], true)
        .await
        .unwrap();
    assert!(!top_names(api).contains(&"a.png".to_owned()));
    // ...and come back when their own input is added again.
    api.add_inputs(vec![photos], true).await.unwrap();
    assert!(top_names(api).contains(&"a.png".to_owned()));
}

#[tokio::test(flavor = "multi_thread")]
async fn clearing_finished_drops_inputs_left_without_images() {
    let mut fx = fixture(false);
    let api = &fx.api;
    api.add_inputs(vec![fx.photos.display().to_string()], true)
        .await
        .unwrap();
    api.set_preset(tiff_preset()).unwrap();
    api.plan().await.unwrap();
    api.start_run().await.unwrap();
    until_finished(&mut fx.events).await;

    let left = api.clear_finished().unwrap();
    assert!(left.roots.is_empty(), "only notes.txt was left: {left:?}");
    // Nothing more to clear: there's no current plan now.
    assert_eq!(api.clear_finished().unwrap().files, 0);
}
