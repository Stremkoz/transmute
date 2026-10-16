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
| `overview()` | `Overview` | Inputs summary, working preset, current plan summary, run status, notices. Use it to (re)draw the workspace. |
| `dismissNotices()` | — | Clears `Overview.notices`. |
| `addInputs(paths, recursive)` | `InputsSummary` | Async. Rescans every input; `scanProgress` events while it works. |
| `removeRoot(root)` / `clearInputs()` | `InputsSummary` | |
| `tree(root, dir)` | `TreeNode[]` | The children of one folder (`dir` relative to the root, `/`-separated, `""` for the top). Folders come first, with file counts. Load on demand; never mount thousands of rows. |
| `setExcluded(root, relativePath, excluded)` | `InputsSummary` | A file, or a folder and everything below it. |
| `preset()` / `setPreset(preset)` | `Preset` / — | The working configuration. Validated, except that it may have no outputs yet. Remembered for the next launch. |
| `storedPresets()` | `StoredPreset[]` | |
| `loadPreset(id)` | `Preset` | Makes a stored preset the working one. |
| `savePreset(id?, preset)` | id | No id: a new one is made from the name. |
| `deletePreset(id)` | — | |
| `defaults()` / `setDefaults(d)` | `UserDefaults` / — | Layered under every preset. |
| `settings()` / `setSettings(s)` | `AppSettings` / — | Tool paths take effect at once. |
| `tools()` / `refreshTools()` | `ToolInfo[]` | What's installed, versions, formats, caveats. |
| `plan()` | `PlanSummary` | Async. Counts, plus problems and notes grouped with counts. `invalid` while the working preset has no outputs. About 2.5 s for 50,000 files × 2 outputs on a fast desktop. |
| `jobs(filter, offset, limit)` | `JobPage` | Pages of at most 1000. Filters: `all`, `problems`, `ready`, `running`, `converted`, `failed`, `cancelled`, `finished`. |
| `startRun()` | — | Async; returns once started. Progress arrives as events. |
| `pause()` / `resume()` / `cancel()` | — | Pause holds back new jobs; cancel kills running conversions (whole process trees), leaves nothing partial and marks unstarted jobs cancelled. |
| `report()` | text | Totals plus every job that didn't convert, with its reason. For "save report…". |
| `preview(target)` | `Preview` | Async. `{ kind: "source", entry }` or `{ kind: "output", job }` (converted jobs only). See [Previews](#previews). |
| `sample(entry, target)` | `Sample` | Async. Converts one file with one output into the cache, with size, time, notes and a preview: a trial before a long batch. |
| `clearPreviews()` | — | Empties the preview and sample caches. |

## Events

Delivered through the sink given to the `Api` at construction. The shell emits each one to the webview. All are tagged by `type`.

| Event | When |
|---|---|
| `scanProgress { discovered, inspected }` | While adding inputs, at most every 100 ms. |
| `jobsChanged { jobs: JobUpdate[] }` | Job state changes since the last batch, at most every 100 ms. A 10,000-file run doesn't flood the UI. |
| `progress { progress }` | Run totals, at most every 100 ms: counts, original and output bytes of converted jobs, elapsed time, and `remainingMs` once enough jobs have finished to estimate it. |
| `runFinished { progress }` | Once, at the end, after the last `jobsChanged`. |
| `notice { text }` | Something to show once: a damaged settings file that was backed up, a run that couldn't start, … |

A job's `state` goes `ready` → `running` → `converted` / `failed` / `cancelled`. Jobs that planning skipped or blocked stay `skipped` / `blocked`, with a `reason`.

## Previews

The webview decodes JPEG, PNG, WebP, GIF, BMP and AVIF itself, so `preview` returns those files' own paths (`rendered: false`). JPEG XL, HEIF and TIFF are rendered to a lossless PNG of the first frame (`rendered: true`), using the same tools as conversions.

The cache policy is explicit (spec §7):

- Renderings live in `<state>/previews/` (see [where files are stored](preset-format.md#where-files-are-stored)). They're keyed by the file's path, size and modification time, so a replaced file is rendered afresh.
- The cache is capped at 1 GB, oldest renderings first. Trial conversions live in `<state>/samples/` and are removed when the app starts.
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
