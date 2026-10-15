# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Repository state

This repo is the **overhaul** of Transmute, a Windows-first media converter. As of 2026-10-15 it contains only repository scaffolding: README, MIT LICENSE, `.github/` CI and templates, and empty top-level directories. The Tauri app has not been scaffolded yet. Reference material:

- `docs/sourcefiles/Transmute_Overhaul_Specification.md`: the working-draft product spec for the overhaul. Read it before starting design or implementation work. It is a proposed direction, not a final contract. Section 12 lists decisions that are still open. It is an **internal design document**: never commit it, quote it in release material, or move it out of the ignored folder.
- `docs/sourcefiles/transmute/`: a full clone of the **existing** Transmute app (C#/.NET 9, upstream `github.com/Stremkoz/transmute-legacy` (archived), v1.1.0). It has its own `.git`. Treat it as **read-only reference**: audit it, but don't edit it. The goal is an overhaul, not a port. Keep what works and don't feel bound by its design.

`docs/sourcefiles/` is gitignored, so it stays local only. The repo is hosted privately at `github.com/Stremkoz/transmute`. The old 1.x repo is archived as `Stremkoz/transmute-legacy`. `CURRENT_SESSION.md` (session-continuity notes required by the workspace rules) is also gitignored and must never be committed.

CI (`.github/workflows/ci.yml`, windows-latest) detects `Cargo.toml` / `src-tauri/Cargo.toml` and `package.json` and runs `cargo fmt --check`, `clippy -D warnings`, `cargo test`, and the `npm` lint/typecheck/test/build scripts. Until those files exist it is a no-op, so new projects should define scripts with those names.

## Overhaul direction (from the spec)

- **Target stack:** Rust core + Tauri 2 shell + React/TypeScript (Vite) frontend; SQLite only if persistent jobs or history need it. The existing app is C#/Avalonia, so this is a rewrite/migration, not an incremental change.
- **Phases:** 0 audit existing features → 1 schemas (job/output/preset, destination and collision rules) → 2 foundational UI → 3 hierarchical queue and review/compare → 4 presets, naming, codec panels → 5 optional CLI.
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
