# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Repository state

This repo is the **overhaul** of Transmute, a Windows-first media converter. The backend is a Rust Cargo workspace under `crates/`; milestones M0–M9 and M11 are done, and M10 (video/audio) is paused while the user decides whether Transmute stays images-only. The desktop app (Tauri 2 shell in `src-tauri/`, React/TypeScript in `src/`) talks only to `crates/transmute-api` (`docs/api.md`). Its phases G0–G5 are in `docs/internal/gui-plan.md`; check which is current, and agree layout or behaviour changes with the user before building them. Reference material:

- `docs/sourcefiles/Transmute_Overhaul_Specification.md`: the working-draft product spec for the overhaul. Read it before starting design or implementation work. It is a proposed direction, not a final contract. Section 12 lists decisions that are still open. It is an **internal design document**: never commit it, quote it in release material, or move it out of the ignored folder.
- `docs/sourcefiles/transmute/`: a full clone of the **existing** Transmute app (C#/.NET 9, upstream `github.com/Stremkoz/transmute-legacy` (archived), v1.1.0). It has its own `.git`. Treat it as **read-only reference**: audit it, but don't edit it. The goal is an overhaul, not a port. Keep what works and don't feel bound by its design.

- `docs/internal/backend-plan.md`: **the active plan.** Backend-first: Rust workspace milestones M0–M11, architecture decisions, testing strategy, open decisions and a decision log. Check which milestone is current before starting work, and record decisions in its log.
- `docs/internal/gui-plan.md`: the desktop app's plan (phases G0–G5, layout and architecture decisions agreed with the user). GUI decisions also go in the backend plan's decision log.
- `docs/internal/legacy-audit.md`: the Phase 0 audit of 1.x, with confirmed defects, keep/change/drop recommendations, and the regression cases the overhaul's tests must cover. Read it before designing the job planner, file safety, metadata or routing.

`docs/sourcefiles/` (user-provided reference) and `docs/internal/` (generated design notes) are both gitignored and stay local only. Put new internal design documents in `docs/internal/`. The repo is hosted privately at `github.com/Stremkoz/transmute`. The old 1.x repo is archived as `Stremkoz/transmute-legacy`. `CURRENT_SESSION.md` (session-continuity notes required by the workspace rules) is also gitignored and must never be committed.

CI's frontend job runs `npm run lint`, `typecheck`, `test` and `build`; keep those script names.

## Commands

```sh
cargo build --workspace
cargo test --workspace                      # all tests
cargo test -p transmute-core extension      # one crate, tests matching a name
cargo lint                                  # alias: clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cargo dev <subcommand>                      # alias: run the transmute-dev CLI, e.g. `cargo dev tools`
TRANSMUTE_REAL_TOOLS=1 cargo test --workspace   # opt-in: also run tests against the installed encoders
cargo dev inspect <paths> [--list]          # scan files/folders, show detected formats
cargo dev plan <paths> --preset <file>      # dry run: planned outputs, routes, collisions
cargo dev run <paths> --preset <file|id>    # plan, then convert for real (Ctrl+C cancels)
cargo dev preset list|import|export|delete  # stored presets; `defaults` and `settings` show/import the other documents
cargo dev env                               # includes where settings live; a `portable` file beside the exe switches to <exe>/data
cargo run --release -p transmute-cli -- bench-scan   # time a 50k-file scan (budget: ≤ 2 s)
python scripts/bench_corpus.py                       # fetch/generate the codec benchmark corpus into bench-data/ (gitignored)
cargo run --release -p transmute-cli -- bench        # codec grid: size, speed, SSIMULACRA2 (≈ 30 min); then scripts/bench_report.py <csvs>
```

Desktop app (run from the repo root):

```sh
npm run tauri dev           # the app, with live reload of the frontend
npm run dev                 # the frontend alone in a browser, against the mock backend (src/api/mockApi.ts)
npm run lint && npm run typecheck && npm test && npm run build   # what CI's frontend job runs
npx tauri build --debug --no-bundle   # build target/debug/transmute.exe without an installer
```

The toolchain is pinned in `rust-toolchain.toml`. CI runs fmt --check, clippy -D warnings and tests, all with `--locked`, on Windows and Ubuntu.

## Workspace layout

The crates follow the dependency direction `core ← backends ← engine ← api` (and `engine ← cli`, the dev tool). Keep the layering:
- `transmute-core` must never spawn processes or depend on an async runtime, so the planner stays testable without tools.
- Encoders are invoked only from `transmute-backends`.
- Preset, user-defaults and settings JSON is a versioned public format, documented in `docs/preset-format.md`. Read and write stored files only through `transmute-core::store`, which does atomic saves and never silently replaces an unreadable or newer file (R12).
  - A breaking schema change must bump `Document::VERSION` and add a `migrate_step`.
  - Update the doc in the same change.
  - `examples/presets/` files are loaded by tests, so keep them valid.
- Tool capabilities are data (`transmute-core::capability`): formats, animation, first-frame reading, settings a tool ignores, bit-depth limits. Adding or changing a tool means updating `transmute-backends/src/catalog.rs`, whose static tables or listing parsers are tested against captured real output (plus build probes for what listings can't show), and `version.rs`. Never hardcode format support anywhere else.
- Each tool's adapter lives in `transmute-backends/src/invoke.rs`. Any adapter change must keep the opt-in matrix test passing (`TRANSMUTE_REAL_TOOLS=1 cargo test -p transmute-engine --test matrix`), which converts every input class to every output, with each tool alone, and inspects the results.
- File formats are detected from content (`transmute-core/src/inspect/`). Each header parser gets synthetic-file unit tests (builders in `inspect/test_images.rs`) *and* a case in `tests/real_images.rs` against real encoder output; the synthetic tests alone only prove the parser matches our reading of the spec.
- Planning (`transmute-core/src/plan/`) owns file safety: outputs never share a path, never replace a source, and only replace existing files under `overwrite`. The property test in `plan/tests.rs` guards this. Keep it passing, and commit `proptest-regressions/` files when it finds a case. When mutation-testing, `touch` restored files: cargo tracks mtimes, and a restored backup can leave a stale binary.
- Metadata policy is carried out by `transmute-core::metadata` for the containers it can rewrite (JPEG, PNG, WebP, JPEG XL): encoders keep everything, then the engine sets the output's blocks from the original source. The planner records how in each job's `MetadataHandling`. The opt-in R5/R6 test (`TRANSMUTE_REAL_TOOLS=1 cargo test -p transmute-engine --test metadata`) must keep passing; a privacy failure there is a release blocker.
- `transmute-api` is the only thing the desktop frontend talks to (documented in `docs/api.md`). It addresses files and jobs by id, never by frontend-supplied paths. Its types generate the TypeScript in `crates/transmute-api/bindings/`; after changing any type that crosses the boundary (API types, or core types deriving `ts_rs::TS` under the `typescript` feature), regenerate with `TRANSMUTE_UPDATE_BINDINGS=1 cargo test -p transmute-api --test bindings` and commit the result (a test fails otherwise). `tests/headless.rs` drives a whole session through the facade; keep it passing.
- The desktop app is `src-tauri/` (crate `transmute-app`, binary `transmute`) plus `src/`. The shell holds no logic: one `async` `#[tauri::command]` per `Api` method in `src-tauri/src/commands.rs`, registered in `main.rs`, and every `ApiEvent` emitted on the `transmute` channel. Adding a command means touching both files and `src/api/tauriApi.ts`; a frontend test compares the lists. The webview gets no fs/shell plugins; the asset-protocol scope starts empty and the shell allows each file `preview()`/`sample()` returns.
- The frontend talks to the backend only through the `TransmuteApi` interface (`src/api/client.ts`), with types imported from `@bindings` (the generated files). `src/api/mockApi.ts` implements it in memory for the browser and tests; extend it as the UI starts using more commands. It must send documents in the backend's wire shape (serde leaves out an `inputFilter` of "all", empty `codecs`, empty `toolPaths`, and an empty naming `prefix` or `suffix`): fields that are skipped when empty must be `#[ts(optional)]` in the bindings, so TypeScript forces the UI to handle them. A mock that always filled them in hid a crash on every real launch (G5). Pickers and window drops go through the `Platform` interface (`src/platform/`), never straight to Tauri plugins from components; tests use the fake in `src/test/render.tsx`. Components read session state through `useSession` (a Zustand store in `src/state/`, which also re-plans automatically after each edit) and style with CSS modules over the tokens in `src/styles/tokens.css`.
- The engine (`transmute-engine`) never lets a tool write a final path. It uses a `.transmute-part-*` temp file plus a no-clobber move, journals temp files for crash recovery, and kills whole process trees on cancel. Adapters in `transmute-backends/src/invoke.rs` only build command lines; keep process handling out of them.
- Tests that need executables use the stub scripts in `transmute-backends/src/process.rs` (`test_support`), not the real tools.
- Factory defaults in `codec.rs` are backed by the M9 benchmark data (`docs/internal/m9-defaults.md`). Change them only with new measurements from `cargo dev bench`.
- New codec settings go through the `codec_settings!` macro in `transmute-core/src/codec.rs`, which generates the patch twin. Validation lives in that codec's `ValidateSettings` impl.
- Workspace lints deny `unsafe_code` and warn on `unwrap_used` outside tests. Prefer typed errors (`thiserror`) at crate boundaries.

## Overhaul direction (from the spec)

- **Target stack:** Rust core + Tauri 2 shell + React/TypeScript (Vite) frontend; SQLite only if persistent jobs or history need it. The existing app is C#/Avalonia, so this is a rewrite/migration, not an incremental change.
- **Phases:** the spec's phases are superseded by the backend-first milestones in `docs/internal/backend-plan.md`. The Phase 0 audit is done.
- **Core rules to respect in any design:**
  - Output targets are independent (format + codec settings + optional naming/destination). One source can produce several outputs in a single batch.
  - There is no universal quality slider. Each codec exposes its own real controls, e.g. WebP quality/method/lossless and JXL distance/effort/lossless.
  - Never change the source basename by default. Prefixes, suffixes and tokens are strictly opt-in.
  - Validate collisions before execution. Never silently overwrite. Write to a temp file, then finalise.
  - The queue tree in the UI is only a presentation model. The engine owns its own job graph and statuses.
  - GUI and any CLI must share the same preset schema, job planner and engine.
- **Terminology:** "Output" means one format/configuration; "Preset" means a saved multi-output configuration. Don't reintroduce "profiles", the legacy term.

## Legacy app (reference for the Phase 0 audit)

Build/test commands, run inside `docs/sourcefiles/transmute/` (requires the .NET 9 SDK):

```
dotnet build
dotnet test                                   # tests/Transmute.Core.Tests (xUnit + Moq). Currently only a placeholder test exists.
dotnet test --filter "FullyQualifiedName~Name" # single test
dotnet publish src/Transmute.Avalonia/Transmute.Avalonia.csproj -c Release -r win-x64 --self-contained
dotnet publish src/Transmute.CLI/Transmute.CLI.csproj -c Release -r win-x64 --self-contained
```

Projects: `Transmute.Core` (engine, NetVips), `Transmute.CLI` (System.CommandLine), `Transmute.Avalonia` (current GUI, CommunityToolkit.Mvvm). `Transmute.GUI` is the retired WPF GUI and is not built.

Architecture worth preserving or learning from:

- **Backends** (`Core/Backends`): `IBackend`/`BackendBase` wrap five tools: libvips (in-process via NetVips), plus cwebp/dwebp, cjxl/djxl, ffmpeg and ImageMagick as external processes (`RunProcessAsync`). `BinaryDiscovery` resolves executables from a config override first, then `PATH`.
- **Routing** (`Core/Routing/FormatRouter.cs`): this file holds the key domain logic. The priority order is JXL tools whenever either side is `.jxl`, then cwebp/dwebp for `.webp`, then ffmpeg for animated or video formats, then per-format affinity from `FormatRegistry`, with a fallback to magick and then vips. When no single backend handles a format pair, it builds a **two-step plan through a PNG intermediate** (via `TempFileManager`). Each plan carries a human-readable `RoutingReason` and `FallbackNote`.
- **Engine** (`Core/Processing/ConversionEngine.cs`): a channel plus semaphore runs jobs in parallel. A failure in one file never aborts the batch. Output paths come from `ResolveOutputPath`, which handles name-pattern tokens: `{name}`, `{ext}`, `{original_ext}`, `{date}`, `{counter}`.
- **Wiring:** `Core/TransmuteFactory.cs` builds the backends, router and engine. Both the CLI and the GUI use it.
- **Config:** `ConfigManager` (global `config.json`) and `ProfileManager` (per-profile JSON). Null profile fields inherit from the global config. Portable mode applies when there is a `portable` marker or a `config.json` beside the exe; otherwise files live under `%APPDATA%\Transmute\`.
- User-facing behaviour (CLI flags, config keys, metadata modes, watch mode, log formats) is documented in `docs/sourcefiles/transmute/README.md` and `docs/sourcefiles/transmute/docs/*.md`.
