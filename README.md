# Transmute

**A fast, approachable, Windows-first converter for image, video and audio files, including very large batches.**

> **Status: pre-alpha / planning.** This is a ground-up overhaul of Transmute 1.x (C#/.NET). There is no runnable code here yet.

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
├── src/            # Application source
└── tests/          # Integration tests and test fixtures
```

The layout inside `src/` will be set when the Tauri app is scaffolded in Phase 1.

## Development

Prerequisites for the planned stack:

- [Rust](https://rustup.rs/) (stable)
- [Node.js](https://nodejs.org/) LTS
- [Tauri 2 prerequisites for Windows](https://v2.tauri.app/start/prerequisites/) (WebView2, MSVC build tools)

Build and test commands will be added here once the app is scaffolded. CI (`.github/workflows/ci.yml`) already runs Rust and frontend checks automatically as soon as a `Cargo.toml` or `package.json` exists.

## Contributing

1. Open an issue describing the bug or proposal.
2. Create a branch from `main`, e.g. `feat/output-targets` or `fix/collision-check`.
3. Use [Conventional Commits](https://www.conventionalcommits.org/) for commit messages.
4. Open a pull request using the PR template.

## License

MIT — see [LICENSE](LICENSE).
