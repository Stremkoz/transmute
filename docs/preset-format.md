# Preset, defaults and settings file format

Transmute stores presets, user defaults and application settings as JSON. This page describes schema version **1** of each.

The types are defined in `crates/transmute-core` (`preset.rs`, `output.rs`, `codec.rs`, `naming.rs`, `schema.rs`). If this page and the code disagree, the code wins. Please fix the page.

## How settings layer

Every setting resolves to the most specific value that's set:

```
output target  →  preset  →  user defaults  →  factory defaults
```

Leaving a field out means "inherit". A change made in the app for one job only is just an unsaved preset, so it isn't a separate layer.

Check any file with:

```sh
cargo dev preset check path/to/preset.json [--defaults path/to/defaults.json]
```

## Rules for every document

- `schemaVersion` (a number) is required at the top level.
  - A file from a **newer** Transmute is refused with an explanation, never half-read, and never overwritten.
  - Older versions are migrated automatically. A stored file is upgraded on disk, and the original is kept beside it (`web.v1.json`).
  - A stored file that can't be read is moved aside (`defaults.corrupt-20261015-093000.json`) and you're told; it is never silently replaced.
- Field names are camelCase. **Unknown fields are errors**, so a typo like `qualty` is reported with its location (`outputs[0].codec: unknown field 'qualty'`) instead of being silently ignored.
- All problems are reported together, each with its path.

## Preset

```json
{
  "schemaVersion": 1,
  "name": "Archive and Web",
  "description": "Lossless JPEG XL for the archive, plus a small WebP for sharing.",
  "inputFilter": { "mode": "exclude", "formats": ["gif"] },
  "destination": { "mode": "subfolder" },
  "outputs": [
    {
      "codec": { "format": "jxl", "lossless": true },
      "naming": { "prefix": "archive_", "suffix": "_{date}", "dateStyle": "short" }
    },
    {
      "codec": { "format": "webp", "quality": 80 },
      "naming": { "prefix": "web_" },
      "metadata": "stripAll"
    }
  ]
}
```

This file is `examples/presets/archive-and-web.json`, and a test keeps it valid.

| Field | Required | Description |
|---|---|---|
| `name` | yes | 1–100 characters, with no leading or trailing whitespace |
| `description` | no | Free text |
| `inputFilter` | no | Which inputs to process (default: all). See [Input filter](#input-filter). |
| `destination` | no | Where outputs go, for all outputs. See [Destination](#destination). |
| `naming` | no | Opt-in name changes, for all outputs. See [Naming](#naming). |
| `metadata` | no | Metadata policy, for all outputs. See [Metadata](#metadata). |
| `collision` | no | What to do if an output path is taken. See [Collision](#collision). |
| `outputs` | yes | 1–16 output targets. The same format may appear more than once, e.g. two WebP sizes. |

### Output target

| Field | Required | Description |
|---|---|---|
| `codec` | yes | `format` plus any codec settings to override. See [Codecs](#codecs). |
| `destination`, `naming`, `metadata` | no | Override the preset-wide value for this output only |

## User defaults

The same shape, minus `name`, `inputFilter` and `outputs`, plus per-format codec defaults:

```json
{
  "schemaVersion": 1,
  "codecs": [
    { "format": "webp", "quality": 82 },
    { "format": "jxl", "effort": 9 }
  ],
  "metadata": "colorProfileOnly"
}
```

`codecs` may hold at most one entry per format.

## Settings

How Transmute runs, rather than what it produces:

```json
{
  "schemaVersion": 1,
  "toolPaths": { "cjxl": "C:\\Tools\\libjxl\\cjxl.exe" },
  "workers": 4
}
```

| Field | Meaning | Default |
|---|---|---|
| `toolPaths` | Executables to use instead of the ones found automatically, per tool (`cjxl`, `djxl`, `cwebp`, `dwebp`, `gif2webp`, `vips`, `vipsheader`, `magick`, `ffmpeg`, `ffprobe`). Absolute paths only | found automatically |
| `workers` | Conversions to run at once, 1–256 | one per CPU core |

## Codecs

Each codec has its own real settings; there is no shared quality scale. Any field left out inherits.

The factory defaults were chosen by measuring size, speed and visual fidelity (SSIMULACRA2) on photos, high-resolution photos, illustrations and transparent graphics. Lossy formats default to high quality; JPEG XL defaults to visually lossless.

| `format` | Field | Range | Factory | Notes |
|---|---|---|---|---|
| `jpeg` (alias `jpg`) | `quality` | 1–100 | 85 | |
| | `progressive` | bool | true | |
| `png` | `compression` | 0–9 | 9 | Always lossless |
| `webp` | `lossless` | bool | false | When true, `quality` is ignored |
| | `quality` | 0–100 | 85 | |
| | `method` | 0–6 | 4 | Higher is slower and smaller |
| `jxl` | `lossless` | bool | false | When true, `distance` is ignored |
| | `distance` | 0.01–25 | 1.0 | ~1.0 is visually lossless. Use `lossless` rather than distance 0 |
| | `effort` | 1–10 | 7 | 5 is ~1.5–5× faster for lossy output at the same size; lossless files grow ~8% |
| | `reconstructJpeg` | bool | true | Lossless + JPEG input: store a reversible recompression of the original JPEG |
| `avif` | `lossless` | bool | false | |
| | `quality` | 1–100 | 70 | |
| | `effort` | 0–9 | 4 | Higher is much slower for little gain |
| `heif` (alias `heic`) | `lossless` | bool | false | |
| | `quality` | 1–100 | 60 | Not yet benchmarked |
| `tiff` (alias `tif`) | `compression` | `none` `lzw` `deflate` `zstd` | `lzw` | |
| `gif` | — | | | Palette is generated automatically |
| `bmp` | — | | | |

## Input filter

The `formats` listed are matched against each file's **detected** format, not its extension.

| `mode` | Fields | Meaning |
|---|---|---|
| `all` | — | Every supported file (default) |
| `only` | `formats` (non-empty, no duplicates) | Just these formats |
| `exclude` | `formats` (non-empty, no duplicates) | Everything except these |

## Destination

| `mode` | Fields | Output goes to |
|---|---|---|
| `besideSource` | — | Next to each source file (default) |
| `subfolder` | — | `<source folder>/<extension>/`, e.g. `photos/webp/a.webp` |
| `custom` | `root` (absolute path), `perFormatSubfolders` (bool, default false) | Under `root`, keeping each file's path relative to the folder it was added from |

## Naming

By default the source basename is kept and only the extension changes. A naming rule adds an optional `prefix` and/or `suffix` around the basename:

```
<prefix><original basename><suffix>.<extension>
```

`prefix` and `suffix` may contain these tokens:

| Token | Expands to |
|---|---|
| `{format}` | Output extension, e.g. `webp` |
| `{original_ext}` | Source extension, e.g. `jpg` |
| `{date}` | A date, per `dateSource` and `dateStyle` |
| `{counter}` | Position in the batch, zero-padded to the batch size |

Write `{{` and `}}` for literal braces. Characters Windows forbids in file names (`< > : " / \ | ? *`) are rejected, so a pattern can't escape the destination folder.

| Field | Values | Default |
|---|---|---|
| `dateStyle` | `compact` (20261015), `short` (261015), `iso` (2026-10-15) | `compact` |
| `dateSource` | `batchStart` (same date for the whole run), `sourceModified` | `batchStart` |

## Metadata

| Value | EXIF | XMP | ICC colour profile |
|---|---|---|---|
| `preserveAll` (default) | Kept, as in the source | Kept | Kept |
| `stripAll` | Removed (see orientation below) | Removed | Removed |
| `colorProfileOnly` | Removed (see orientation below) | Removed | Kept |
| `copyrightOnly` | Only the copyright and artist fields. Location, device and dates are removed | Removed | Kept |

Every mode except `preserveAll` also removes comments, text chunks, IPTC data and embedded previews.

**Orientation.** A photo taken with a rotated camera only displays upright because of its EXIF orientation field. Modes that remove EXIF keep a minimal EXIF block holding just that field, so nothing about the photo or camera survives. Where the output format can't carry it that way, the pixels are rotated instead when the encoder supports it (libvips for JPEG sources, ImageMagick).

Not every mode can be honoured for every output. GIF and BMP files have no place for metadata, for example, and copyright filtering isn't available inside AVIF, HEIF or TIFF files. Transmute works this out when planning, against the installed tools, and reports it before the run instead of silently ignoring the setting.

## Collision

What happens when a planned output path already exists. **Source files are never overwritten**, under any policy.

| Value | Behaviour |
|---|---|
| `skip` | Leave the existing file; report the job as skipped (default) |
| `rename` | Write the new file under a different name |
| `overwrite` | Replace the existing output |

## Where files are stored

| | Windows | macOS | Linux |
|---|---|---|---|
| Settings, defaults, presets | `%APPDATA%\Transmute` | `~/Library/Application Support/Transmute` | `$XDG_CONFIG_HOME/transmute` (`~/.config/transmute`) |
| Run state (temporary-file journals) | `%LOCALAPPDATA%\Transmute\state` | `…/Transmute/state` | `$XDG_STATE_HOME/transmute` (`~/.local/state/transmute`) |

Inside the settings folder: `settings.json`, `defaults.json` and `presets/<id>.json`. A preset's id is its file name, made from its name when first saved ("Web & Archive" → `web-archive`).

**Portable mode:** if a file named `portable` sits beside the executable, everything lives in a `data` folder next to it instead, and nothing is written to the user profile.

Every save is atomic: the new file is written and flushed under a temporary name, then renamed into place, so a crash leaves the old file or the new one, never a mix.
