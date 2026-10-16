# Transmute

**A fast, approachable, Windows-first converter for image, video and audio files, including very large batches.**

> **Status: pre-alpha.** This is a ground-up overhaul of Transmute 1.x (C#/.NET). The desktop app converts images end to end: drop files, pick outputs and their settings, convert, and compare the results. Video and audio are on hold.

---

## Table of Contents

- [Install](#install)
- [Goals](#goals)
- [Planned Features](#planned-features)
- [Tech Stack](#tech-stack)
- [Roadmap](#roadmap)
- [Project Structure](#project-structure)
- [Development](#development)
- [Contributing](#contributing)
- [License](#license)

---

## Install

Windows 10 or 11, 64-bit. Two downloads, from the releases page:

- **`Transmute_<version>_x64-setup.exe`**: installs for your user account (no administrator rights), with a Start-menu entry and an uninstaller. Settings live in `%APPDATA%\Transmute`.
- **`Transmute_<version>_x64-portable.zip`**: unzip anywhere and run `transmute.exe`. Settings stay in a `data` folder beside it, so it can live on a USB stick. Delete the `portable` file to use `%APPDATA%` instead.

Both include the converters Transmute needs for JPEG, PNG, WebP, JPEG XL, AVIF, TIFF and GIF: libjxl (cjxl, djxl), libwebp (cwebp, dwebp, gif2webp) and libvips, each under its own licence (see `tools\licenses`). Two tools are optional and used when installed: **ImageMagick** (`winget install ImageMagick.ImageMagick`) adds BMP input and more fallbacks; **FFmpeg** (`winget install Gyan.FFmpeg`) adds another fallback for animation. Nothing bundled writes HEIF: that needs a libvips or ImageMagick build with an HEVC encoder, which most builds leave out. The Tools dialog shows what each tool adds, what's missing, and lets you use another copy of any of them.

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
├── src-tauri/      # Desktop shell (Tauri 2): forwards commands and events to transmute-api
├── src/            # Frontend (React + TypeScript, Vite)
└── tests/          # Shared test fixtures and corpora
```

## Development

**Prerequisites:** [rustup](https://rustup.rs/), [Node.js](https://nodejs.org/) (LTS) and Python 3. The pinned toolchain in `rust-toolchain.toml` installs automatically. On Linux, Tauri also needs the [WebKitGTK system libraries](https://v2.tauri.app/start/prerequisites/). To convert files during development, put the encoders on `PATH` (cjxl/djxl, cwebp/dwebp, libvips; optionally ImageMagick, ffmpeg), or fetch the bundled set with `python scripts/fetch_tools.py` and add `target/bundle-tools/tools` to `PATH`.

```sh
cargo build --workspace        # build everything
cargo test --workspace         # run all tests
cargo test -p transmute-core   # test one crate
cargo lint                     # clippy, exactly as CI runs it
cargo fmt --all                # format
cargo dev --help               # run the developer CLI (transmute-dev)
```

The desktop app:

```sh
npm install                    # once
npm run tauri dev              # run the app with live reload
npm run dev                    # the frontend alone in a browser, against a mock backend
npm run lint                   # also: typecheck, test, build (what CI runs)
npm run package                # Windows installer + portable zip, with the bundled tools (scripts/package.py)
```

| Crate | Purpose |
|-------|---------|
| `transmute-core` | Domain types, settings/preset schemas, inventory, planner, path and collision policy. Spawns no processes. |
| `transmute-backends` | External tool discovery, capability probing, per-tool adapters, metadata I/O |
| `transmute-engine` | Scheduling, process supervision, safe file finalisation, progress events |
| `transmute-api` | The facade the desktop app talks to: commands, events, generated TypeScript types ([docs/api.md](docs/api.md)) |
| `transmute-cli` | `transmute-dev`, an internal CLI for driving the backend before the GUI exists |
| `src-tauri` (`transmute-app`) | The desktop shell; no logic of its own |

Releases: pushing a `v*` tag runs `.github/workflows/release.yml`, which builds both downloads on Windows and attaches them to a draft GitHub release.

## Contributing

1. Open an issue describing the bug or proposal.
2. Create a branch from `main`, e.g. `feat/output-targets` or `fix/collision-check`.
3. Use [Conventional Commits](https://www.conventionalcommits.org/) for commit messages.
4. Open a pull request using the PR template.

## License

MIT — see [LICENSE](LICENSE).
