# Application API

`crates/transmute-api` is the boundary between Transmute's backend and its desktop frontend. The frontend (React in a Tauri shell) calls **commands** and receives **events**. The shell only forwards them: it holds no logic of its own. The same facade can drive a headless session, as `crates/transmute-api/tests/headless.rs` does.

TypeScript types for everything below are generated from the Rust types and committed in [`crates/transmute-api/bindings/`](../crates/transmute-api/bindings/) (import from `bindings/index.ts`). A test fails when they're out of date; regenerate with:

```sh
TRANSMUTE_UPDATE_BINDINGS=1 cargo test -p transmute-api --test bindings
```

## Model

One `Api` value is one **session**:

- **Inputs**: the files and folders the user dropped, scanned into a snapshot (not watched). Files are addressed by `EntryId`, dropped items by `RootId`. Exclusions survive a rescan.
- **Working preset**: the output configuration being edited. It's an unsaved [preset](preset-format.md); saving it is a separate command. It may have no outputs (a first launch starts with none), and it's remembered between launches in `session.json`.
- **Plan**: what would happen to every (file, output) pair, made by `plan`. It's valid for one state of the inputs, preset, user defaults and tools. **Any change invalidates it**, and running needs a current one. Jobs are addressed by `JobId`.
- **Run**: converting a plan's ready jobs. While it runs, the inputs and configuration can't change (`busy`). Once it has run, a plan needs re-planning before it can run again, because its outputs now exist.

Rules the API keeps:

- The frontend never passes paths to read or write, except the ones the user drops or picks. Previews, samples and job details go by id, so the API can't be used to reach arbitrary files.
- Every failure is an `ApiError { code, message }`, with `message` ready to show. The codes are `busy`, `notFound`, `invalid`, `noPlan`, `unavailable` (a file has moved or been deleted), `store`, `io` and `internal`.
- Messages about jobs (`Message`, `Tally`) carry a stable `code` (e.g. `output-exists`, `metadata-not-honoured`) next to their English text, for icons, grouping and later translation.

## Commands

| Command | Returns | Notes |
|---|---|---|
| `overview()` | `Overview` | Inputs summary (with each input's file counts), working preset, the stored preset it was loaded from (`loaded`: id, stored name, `edited`), current plan summary, run status, file counts, notices. Use it to (re)draw the workspace. |
| `dismissNotices()` | — | Clears `Overview.notices`. |
| `addInputs(paths, recursive)` | `InputsSummary` | Async. Rescans every input; `scanProgress` events while it works. |
| `removeRoot(root)` / `clearInputs()` | `InputsSummary` | |
| `tree(root, dir)` | `TreeNode[]` | The children of one folder (`dir` relative to the root, `/`-separated, `""` for the top). Folders come first, with the [file counts](#file-states) of everything below. Files carry their format, dimensions, size and, while a plan is current, one `FileOutput` (job id, target, format, state) per output. Load on demand; never mount thousands of rows. |
| `setExcluded(root, relativePath, excluded)` | `InputsSummary` | A file, or a folder and everything below it (`""`: the whole input). |
| `fileCounts()` | `FileCounts` | How many files are in each [state](#file-states), for the files list's filters. Also in `overview()`. |
| `files(filter, offset, limit)` | `FilePage` | The files in one state, flat, across every input, in order, with their root and path. Pages of at most 1000. |
| `entry(entry)` | `EntryDetails` | Everything about one file: what the header says (dimensions, bit depth, transparency, frames, pages, metadata found), and its jobs in the current plan. |
| `clearFinished()` | `InputsSummary` | Takes the [done](#file-states) files off the list, keeping anything failed, cancelled, waiting or excluded. An input left with no images goes too. Nothing on disk changes; the files stay off through rescans until their input is added again. |
| `preset()` / `setPreset(preset)` | `Preset` / — | The working configuration. Validated, except that it may have no outputs yet. Remembered for the next launch. |
| `storedPresets()` | `StoredPreset[]` | |
| `loadPreset(id)` | `Preset` | Makes a stored preset the working one, and the loaded one. Loading the loaded preset again undoes its edits. |
| `savePreset(id?, preset)` | id | No id: a new one is made from the name. Storing over the loaded preset (a rename) updates what edits are measured against. |
| `saveWorking(id?, name, description?)` | id | Stores the working configuration under that name, over `id` ("save changes") or as a new preset ("save as"), and makes it the loaded one. |
| `newPreset()` | `Preset` | Starts over: no outputs, nothing loaded. |
| `deletePreset(id)` | — | If it was loaded, the working configuration stays, unsaved. |
| `importPreset(path)` / `exportPreset(id, path)` | `StoredPreset` / — | A preset file the user picked: imported as a new preset (older schema versions migrated), or written out to share. |
| `codecs()` | `CodecDefaults[]` | Every format's factory settings and its inherited ones (factory + user defaults), so the UI can show which settings an output changes without repeating the numbers. |
| `defaults()` / `setDefaults(d)` | `UserDefaults` / — | Layered under every preset. |
| `settings()` / `setSettings(s)` | `AppSettings` / — | Tool paths take effect at once. |
| `tools()` / `refreshTools()` | `ToolInfo[]` | What's installed, versions, formats, caveats. |
| `plan()` | `PlanSummary` | Async. Counts, plus problems and notes grouped with counts, and one example output path per output (`examples`). `invalid` while the working preset has no outputs. About 2.5 s for 50,000 files × 2 outputs on a fast desktop. |
| `jobs(filter, offset, limit)` | `JobPage` | Pages of at most 1000. Filters: `all`, `problems`, `ready`, `running`, `converted`, `failed`, `cancelled`, `finished`. |
| `startRun()` | — | Async; returns once started. Progress arrives as events. |
| `pause()` / `resume()` / `cancel()` | — | Pause holds back new jobs; cancel kills running conversions (whole process trees), leaves nothing partial and marks unstarted jobs cancelled. |
| `report()` | text | Totals plus every job that didn't convert, with its reason. |
| `saveReport(path)` | — | Writes `report()` to a file the user picked. |
| `runTotals()` | `TargetTotals[]` | Per output target: converted, skipped, blocked, failed, cancelled, waiting, and bytes in and out of the converted jobs. |
| `locate(target)` | path | Rust only (not a shell command): where a source or a converted output is, for the shell's `reveal`, which shows it in the file manager. `unavailable` if it's gone. |
| `preview(target)` | `Preview` | Async. `{ kind: "source", entry }` or `{ kind: "output", job }` (converted jobs only). See [Previews](#previews). |
| `sample(entry, target)` | `Sample` | Async. Converts one file with one output into the cache, with size, time, notes and a preview: a trial before a long batch. |
| `thumbnail(entry)` | `Preview` | Async. A small rendering (96 px box, WebP or PNG) of one image for the files list, made by libvips (ImageMagick for what vips can't read) and cached. At most four render at once, however many rows ask. `unavailable` if the file is gone or no tool reads it. |
| `clearPreviews()` | — | Empties the preview, thumbnail and sample caches. |

## File states

The desktop app shows one files list before, during and after a run, so each file has states drawn from its jobs (`FileCounts`, `FileFilter`). A file counts in **every** state that applies: one output skipped and another waiting counts as both *problems* and *ready*.

| State | Meaning |
|---|---|
| `ready` | Some output waits to be converted. |
| `problems` | Some output was skipped (e.g. it exists) or blocked when planning. |
| `running` | Some output is being converted. |
| `done` | Every output was written or already existed, and at least one was written. |
| `failed` / `cancelled` | Some output failed / was cancelled. |
| `excluded` | Excluded by the user (no jobs). |
| `unsupported` | Not an image Transmute converts (no jobs). |

Job states only exist while a plan is current; without one, files are only `excluded`, `unsupported` or neither.

## Events

Delivered through the sink given to the `Api` at construction. The shell emits each one to the webview. All are tagged by `type`.

| Event | When |
|---|---|
| `scanProgress { discovered, inspected }` | While adding inputs, at most every 100 ms. |
| `jobsChanged { jobs: JobUpdate[] }` | Job state changes since the last batch, at most every 100 ms. A 10,000-file run doesn't flood the UI. |
| `progress { progress }` | Run totals, at most every 100 ms: counts, original and output bytes of converted jobs, elapsed time, `remainingMs` once enough jobs have finished to estimate it, and `targets`: each output's totals so far (as `runTotals()`). `runFinished` and `overview().run` carry the same. |
| `runFinished { progress }` | Once, at the end, after the last `jobsChanged`. |
| `notice { text }` | Something to show once: a damaged settings file that was backed up, a run that couldn't start, … |

A job's `state` goes `ready` → `running` → `converted` / `failed` / `cancelled`. Jobs that planning skipped or blocked stay `skipped` / `blocked`, with a `reason`.

When WebP's automatic lossless encoded an image both ways, its `converted` state (and a `Sample`) has `losslessChoice { keptLossless, lossyBytes, losslessBytes }`. It's left out otherwise.

## Previews

The webview decodes JPEG, PNG, WebP, GIF, BMP and AVIF itself, so `preview` returns those files' own paths (`rendered: false`). JPEG XL, HEIF and TIFF are rendered to a lossless PNG of the first frame (`rendered: true`), using the same tools as conversions.

The cache policy:

- Renderings live in `<state>/previews/` (see [where files are stored](preset-format.md#where-files-are-stored)), thumbnails in `<state>/thumbnails/`. Both are keyed by the file's path, size and modification time, so a replaced file is rendered afresh.
- Every `Preview` lists the metadata blocks the image has (`metadata`: "EXIF", "ICC profile", "XMP"), for the compare viewer's table.
- The preview cache is capped at 1 GB and the thumbnail cache at 256 MB, oldest first. Trial conversions live in `<state>/samples/` and are removed when the app starts.
- No copies of originals are kept. If a source or output has moved or been deleted, `preview` fails with `unavailable`, and the viewer should say so.

## Wiring into Tauri (sketch)

```rust
#[tauri::command]
async fn plan(api: tauri::State<'_, Api>) -> Result<PlanSummary, ApiError> {
    api.plan().await
}

// At startup:
let handle = app.handle().clone();
let api = Api::open(Arc::new(move |event| { let _ = handle.emit("transmute", event); }))?;
app.manage(api);
```
