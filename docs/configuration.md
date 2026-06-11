# Configuration

Transmute stores all settings in a single JSON file. This document covers every field, what it controls, and how to change it.

---

## Config File Location

Run `transmute config path` to find the active config file. There are two modes:

### Installed mode (default)

Config lives in your user application data folder:

```
%APPDATA%\Roaming\Transmute\config.json
```

### Portable mode

Config lives beside the executable. Portable mode activates if either:
- A file named `portable` exists beside the executable, OR
- A `config.json` file already exists beside the executable

In portable mode, profiles are also stored beside the executable (`Profiles\`), making the whole installation self-contained for moving between machines.

---

## Editing Configuration

### Via CLI (recommended)

```bash
# Read the full config
transmute config get

# Read a single value
transmute config get defaults.webpQuality

# Set a value
transmute config set defaults.webpQuality 80
```

### Via GUI

Open **Settings** (`Ctrl+,`). Changes are saved immediately on close.

### Direct file edit

The config file is plain JSON — open it in any text editor. Transmute re-reads it on next launch.

---

## Complete Config Reference

```jsonc
{
  // ── Binary paths ───────────────────────────────────────────────────────────
  // Each value is a path to the executable, or null to auto-discover on PATH.
  "binaries": {
    "cwebp": null,       // Google WebP encoder
    "dwebp": null,       // Google WebP decoder
    "cjxl": null,        // JPEG XL encoder
    "djxl": null,        // JPEG XL decoder
    "ffmpeg": null,      // ffmpeg (animated formats, video)
    "magick": null       // ImageMagick (broad format fallback)
  },

  // ── Processing ─────────────────────────────────────────────────────────────
  "processing": {
    // Maximum number of images converting simultaneously.
    // 0 = use the number of logical CPU cores.
    "maxParallelJobs": 0,

    // Where to store intermediate files during two-step conversions.
    // null = %TEMP%\Transmute
    "tempDirectory": null,

    // libvips thread count per conversion job.
    // 0 = let libvips decide (typically uses all available cores).
    // Reduce if you want to cap CPU usage from libvips.
    "vipsConcurrency": 0
  },

  // ── Conversion defaults ────────────────────────────────────────────────────
  // These are the fallback values when no profile override or CLI flag is given.
  "defaults": {
    // Lossy quality settings — 0 (worst/smallest) to 100 (best/largest).
    // null = use the backend's built-in default for that format.
    "webpQuality": 85,
    "jpegQuality": 90,
    "jxlQuality": 90,
    "avifQuality": 80,

    // When true, JXL and WebP conversions use lossless encoding by default.
    // Individual runs can override this with --lossless or --quality.
    "losslessDefault": true,

    // WebP compression method (cwebp -m flag).
    // 0 = fastest encoding, 6 = slowest / best compression. Default: 6.
    "webpMethod": 6,

    // JXL effort level (cjxl --effort flag).
    // 1 = fastest encoding, 9 = slowest / smallest file. Default: 7.
    "jxlEffort": 7,

    // JPEG XL distance used when LosslessDefault is false and no --quality or
    // --distance is passed. 0 = lossless, 0.1-1.0 = visually lossless,
    // 1.1-2 = lossy. Lower values preserve more detail and make larger files.
    "jxlDistance": 1.0,

    // How to handle image metadata in output files.
    // "PreserveAll"  — keep EXIF, XMP, IPTC, ICC colour profile (default)
    // "StripAll"     — remove all metadata
    // "ColorProfile" — keep ICC colour profile only
    // "Copyright"    — keep creator/copyright fields only
    "metadataMode": "PreserveAll",

    // If true, existing output files are overwritten. If false (default), they
    // are skipped and reported as "skipped" in results.
    "overwriteExisting": false,

    // Output directory for converted files.
    // null = write output files beside their input files.
    "defaultOutputDirectory": null,

    // Filename template for output files.
    // Tokens: {name}, {ext}, {original_ext}, {date}, {counter}
    // See docs/output-naming.md for full token reference.
    "outputNamingPattern": "{name}.{ext}"
  },

  // ── Log files ──────────────────────────────────────────────────────────────
  "log": {
    // Write a log file after each conversion batch.
    "enabled": false,

    // Log file format: "text" (human-readable) or "json" (machine-readable).
    "format": "text"
  },

  // ── GUI settings ───────────────────────────────────────────────────────────
  "ui": {
    // Play a system notification sound when a batch completes.
    "playSoundOnCompletion": false,

    // Window theme: "System" (follow Windows setting), "Light", or "Dark".
    "theme": "System"
  }
}
```

---

## Defaults in Detail

### Quality settings

Each lossy format has an independent quality default. Quality is on a 0–100 scale for all formats, but the meaning varies slightly per backend:

| Format | Backend | How quality is applied |
|--------|---------|----------------------|
| WebP | cwebp | `-q <value>` directly |
| JXL | cjxl | Uses explicit distance when set; otherwise quality is converted to distance |
| AVIF | libvips | `Q` parameter |
| JPEG | cjxl / libvips | Standard JPEG quality |

Quality `0` means maximum compression (worst quality), `100` means minimum compression (best quality / lossless-like).

For JPEG XL, `defaults.jxlDistance` and `--distance` expose cjxl's native `-d`
control directly. Distance `0` is lossless, `0.1` to `1.0` is visually
lossless with lower values preserving more detail, and `1.1` to `2.0` is lossy.
An explicit distance takes precedence over JXL quality.

### `losslessDefault`

When `true`, JXL and WebP conversions use lossless encoding unless you explicitly pass `--quality` or JXL `--distance` to request lossy. This is the default because lossless is lossless — you can always re-compress later, but you can't recover lost quality.

Set to `false` if you want lossy to be the default and only use lossless when you explicitly pass `--lossless`.

### `jxlDistance`

Controls the default JXL distance when `losslessDefault` is `false` and the run
does not provide `--quality` or `--distance`. This is the preferred JXL
lossless/lossy control:

- `0` = lossless
- `0.1`–`1.0` = visually lossless, lower is higher quality
- `1.1`–`2.0` = lossy

### `webpMethod`

Controls the WebP encoding algorithm. Method 6 (default) is the slowest but produces the best compression ratio. For bulk conversion where speed matters more than file size, method 4 is a good balance.

### `jxlEffort`

Controls how hard cjxl works to find an efficient encoding. Effort 7 (default) is a good balance. Effort 9 can produce noticeably smaller files for large batches but takes significantly longer. Effort 1 is very fast but produces larger files.

### `metadataMode`

Controls metadata preservation across all conversions. This can be overridden per-run with `--metadata` or per-profile.

**PreserveAll** is the safe default — no data loss, suitable for archival. Use **StripAll** for web output where file size and privacy matter. Use **ColorProfile** when you want accurate colours on different displays without leaking personal EXIF data (GPS location, camera serial, etc.).

### `defaultOutputDirectory`

When `null` (default), output files are written beside their input files, keeping directory structure intact. Set to a path to centralise all output in one place regardless of where inputs come from.

### `outputNamingPattern`

The filename template for output files. Default is `{name}.{ext}` which just changes the extension. See [Output Naming Patterns](../README.md#output-naming-patterns) for all available tokens.

---

## Processing in Detail

### `maxParallelJobs`

How many files convert at the same time. `0` uses one job per logical CPU core, which typically saturates CPU without thrashing.

Set to `1` to convert files one at a time (useful for debugging or when running alongside other CPU-intensive work).

### `vipsConcurrency`

libvips internally spawns threads per conversion. By default it uses all available cores. If you're running multiple parallel jobs AND each job spawns many libvips threads, you can end up with much more parallelism than you want.

A good rule of thumb: if `maxParallelJobs` is 4, set `vipsConcurrency` to `max(1, CPU_count / 4)`. For example, on a 16-core machine with 4 parallel jobs, `vipsConcurrency = 4` keeps total thread usage around 16.

### `tempDirectory`

Two-step conversions (where input and output formats have no direct conversion path) write a PNG intermediate file to the temp directory. This defaults to `%TEMP%\Transmute` and is cleaned up on process exit.

Set this to a fast local drive if your default temp directory is on a slower drive or network share.

---

## Binary Paths in Detail

Transmute auto-discovers each backend by searching PATH for the binary name (including `.exe` on Windows). If a binary is not on PATH, you can provide the full path:

```bash
transmute config set binaries.cwebp "C:\tools\libwebp-1.4.0\bin\cwebp.exe"
```

To revert to PATH-based discovery:

```bash
transmute config set binaries.cwebp null
```

**Tip:** If you have multiple versions of a backend installed, use explicit paths to pin Transmute to a specific version.

---

## Resetting Config

To reset everything to factory defaults:

```bash
transmute config reset
```

This does not affect named profiles. To delete a profile, use `transmute profile delete <name>`.
