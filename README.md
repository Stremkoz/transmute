# Transmute

**A fast, approachable, Windows-first converter for image, video and audio files, including very large batches.**

> **Status: pre-alpha.** This is a ground-up overhaul of Transmute 1.x (C#/.NET). The backend is in progress: a developer CLI can already plan and convert image batches. There is no GUI yet.

---

## Table of Contents

- [Goals](#goals)
- [Planned Features](#planned-features)
- [Tech Stack](#tech-stack)
- [Roadmap](#roadmap)
- [Project Structure](#project-structure)
- [Development](#development)
- [Contributing](#contributing)
- [License](#license)

---

## Goals

Transmute should stay a quick, simple converter: add media, choose an output, convert. It should also scale to big folders, mixed input formats and several output formats at once.

The guiding rule is **progressive disclosure**. A basic conversion needs very few decisions. Power users can still tune each codec, control where output lands, and save setups they reuse.

## Planned Features

- **Drop and convert:** drag in files or folders and convert with sensible defaults. There is no wizard and no codec setup required.
- **Multiple outputs per run:** produce JPEG XL, WebP and AVIF from the same sources in a single batch, each with its own settings.
- **Codec-specific controls:** real encoder settings (WebP quality/method, JPEG XL distance/effort, and so on) with plain-English explanations, not one misleading universal quality slider.
- **Safe file handling:** source filenames are kept by default, name clashes are checked before anything runs, nothing is overwritten silently, and files are written to a temporary location before being finalised.
- **Hierarchical queue:** dropped folders keep their structure, can be collapsed and expanded, and let you exclude individual files or subfolders. The queue stays responsive at tens of thousands of files.
- **Honest progress and review:** per-file and per-batch progress, readable failure messages, and a before/after image comparison viewer.
- **Presets:** save a multi-output configuration, including filters, destination and naming, for reuse.

## Tech Stack

The proposed direction, which will be confirmed during Phase 1:

| Layer | Choice |
|-------|--------|
| Core | Rust (job planning, scheduling, codec/backend adapters, file policy) |
| Desktop shell | Tauri 2 |
| Frontend | React + TypeScript (Vite) |
| Encoders | Existing specialised tools and libraries: libvips, cwebp, cjxl, FFmpeg, and others |
| Persistence | SQLite, only where job recovery or history requires it |

## Roadmap

| Phase | Focus |
|-------|-------|
| 0: Audit | Inventory the features, backends, routing and workflows of Transmute 1.x worth keeping |
| 1: Design foundation | Job/output/preset schemas, destination and collision rules, codec capability metadata |
| 2: Foundational UI | New shell, adaptive drop zone, format selection, destination selector |
| 3: Queue and review | Virtualised hierarchical queue, progress, diagnostics, output comparison |
| 4: Advanced workflow | Opt-in naming patterns, multi-output presets, richer codec panels, batch preview |
| 5: Optional CLI | A small CLI on the same core, only if there is real demand |

## Project Structure

```
transmute/
├── .github/        # CI workflow, issue and PR templates
├── assets/         # Icons and static assets
├── config/         # Default/factory configuration
├── docs/           # Project documentation
├── examples/       # Example presets and sample workflows
├── scripts/        # Build and utility scripts
├── crates/         # Rust workspace crates (backend)
├── src/            # Frontend source (later)
└── tests/          # Shared test fixtures and corpora
```

The Tauri shell will be added as `src-tauri/`, with its React frontend in `src/`, once the backend API is ready.

## Development

The Rust backend is built first; the desktop GUI comes later.

**Prerequisites:** [rustup](https://rustup.rs/). The pinned toolchain in `rust-toolchain.toml` installs automatically. To actually convert files you'll also need the external encoders on `PATH`: cjxl/djxl, cwebp/dwebp, libvips, ImageMagick, ffmpeg.

```sh
cargo build --workspace        # build everything
cargo test --workspace         # run all tests
cargo test -p transmute-core   # test one crate
cargo lint                     # clippy, exactly as CI runs it
cargo fmt --all                # format
cargo dev --help               # run the developer CLI (transmute-dev)
```

| Crate | Purpose |
|-------|---------|
| `transmute-core` | Domain types, settings/preset schemas, inventory, planner, path and collision policy. Spawns no processes. |
| `transmute-backends` | External tool discovery, capability probing, per-tool adapters, metadata I/O |
| `transmute-engine` | Scheduling, process supervision, safe file finalisation, progress events |
| `transmute-cli` | `transmute-dev`, an internal CLI for driving the backend before the GUI exists |

## Contributing

1. Open an issue describing the bug or proposal.
2. Create a branch from `main`, e.g. `feat/output-targets` or `fix/collision-check`.
3. Use [Conventional Commits](https://www.conventionalcommits.org/) for commit messages.
4. Open a pull request using the PR template.

## License

MIT — see [LICENSE](LICENSE).
