# CLI Reference

Transmute's command-line interface is available as `Transmute.CLI.exe` (or `transmute` if it's on your PATH). Every command accepts `--help` for a quick inline reference.

---

## Global Option

| Option | Description |
|--------|-------------|
| `--config <path>` | Use a specific config file instead of the default location |

---

## `transmute convert`

Convert one or more images to a target format.

```
transmute convert <inputs...> --format <fmt> [options]
```

### Arguments

| Argument | Description |
|----------|-------------|
| `<inputs>` (one or more, required) | File paths, folder paths, or `-` to read from stdin |

Inputs are resolved as follows:
- **File path** — converted directly
- **Folder path** — all recognised image files in the folder are converted (top-level only unless `--recursive`)
- **`.`** — current working directory (same as a folder path)
- **`-`** — reads paths from stdin, one per line; each line is treated as a file or folder path

### Options

#### Output

| Option | Short | Description |
|--------|-------|-------------|
| `--format <fmt>` | `-f` | **Required.** Target format: `webp`, `jxl`, `avif`, `jpg`, `png`, `tiff`, `gif`, `bmp`, `heif`, etc. |
| `--output <path>` | `-o` | Explicit output file path. Only valid with a single input file. |
| `--output-dir <path>` | | Write all output files into this directory. Overrides config and profile setting. |
| `--name-pattern <pattern>` | `-n` | Output filename template. See [Output Naming Patterns](../README.md#output-naming-patterns). |
| `--overwrite` | | Overwrite output files that already exist. Default is to skip them. |

#### Quality

| Option | Short | Description |
|--------|-------|-------------|
| `--quality <0–100>` | `-q` | Lossy quality level. Applies to WebP, JXL, AVIF, and JPEG. If omitted, the format default from config is used. |
| `--lossless` | `-l` | Enable lossless encoding. Supported for WebP and JXL. Takes precedence over `--quality`. |
| `--method <0–6>` | | WebP compression method. 0 is fastest, 6 is slowest and best. Default: 6. |
| `--effort <1–9>` | `-e` | JXL effort level. 1 is fastest, 9 produces the smallest file. Default: 7. |

#### Filtering

| Option | Short | Description |
|--------|-------|-------------|
| `--recursive` | `-r` | Include files in subdirectories. Only applies when an input is a folder. |
| `--skip <exts>` | | Skip input files with these extensions. Comma-separated or repeated flag: `--skip jpg,png` or `--skip jpg --skip png`. Replaces any profile filter. |
| `--only <exts>` | | Process ONLY files with these extensions. Replaces any profile filter. |

#### Profiles & Advanced

| Option | Short | Description |
|--------|-------|-------------|
| `--profile <name>` | `-p` | Apply a named profile for defaults. CLI flags always override profile values. |
| `--metadata <mode>` | `--meta` | Metadata handling: `preserve`, `strip`, `color`, `copyright`. See [Metadata Handling](../README.md#metadata-handling). |
| `--backend <name>` | | Force a specific backend: `webp`, `jxl`, `ffmpeg`, `vips`, `magick`. Fails if the backend can't handle the format pair. |
| `--jobs <n>` | `-j` | Number of parallel conversion jobs. 0 = one per logical CPU. |

#### Logging & Diagnostics

| Option | Description |
|--------|-------------|
| `--log` | Write a log file after conversion, even if disabled in config. |
| `--no-log` | Skip the log file, even if enabled in config. |
| `--log-format <fmt>` | Log format: `text` (default) or `json`. |
| `--dry-run` | Show which files would be converted and their output paths, without converting anything. |
| `--verbose` `-v` | Show per-file detail: backend chosen, reason it was chosen, applied settings, size delta. |

### Exit Codes

| Code | Meaning |
|------|---------|
| `0` | All files succeeded (or were skipped) |
| `1` | One or more files failed |

### Examples

```bash
# Single file
transmute convert portrait.jpg --format webp

# Current folder, top-level only
transmute convert . --format webp

# Specific folder, recursive
transmute convert "C:\Photos\Holiday" --format jxl --recursive

# Multiple inputs at once
transmute convert a.jpg b.png ./subfolder --format avif

# Quality 80, strip metadata, output to separate folder
transmute convert ./raw --format webp -q 80 --metadata strip --output-dir ./web

# Lossless WebP of only HEIC files
transmute convert ./imports --format webp --lossless --only heic,heif

# Dry run to preview the plan
transmute convert ./mixed --format avif --dry-run

# Verbose — see exactly what each file is doing
transmute convert ./mixed --format webp --verbose

# Re-encode everything, overwriting existing outputs
transmute convert . --format webp --overwrite

# Use a profile
transmute convert . --format webp --profile web-optimised

# Use a profile but override quality for this run
transmute convert . --format webp --profile web-optimised --quality 70

# Custom naming pattern: prepend date
transmute convert . --format webp --name-pattern "{date}_{name}.{ext}"

# Force ImageMagick backend (useful for obscure formats)
transmute convert design.psd --format png --backend magick

# 4 parallel jobs
transmute convert ./large-batch --format avif -j 4

# Read input paths from another command
dir /b /s "C:\Photos\*.jpg" | transmute convert - --format webp

# Write a JSON log file
transmute convert . --format webp --log --log-format json
```

---

## `transmute watch`

Monitor a folder and automatically convert new image files as they appear.

```
transmute watch <folder> --format <fmt> [options]
```

Transmute uses file system events to detect new files, then waits until each file has been stable (unchanged) for a configurable window before processing it. This prevents converting partial uploads or files still being written.

Press **Ctrl+C** to stop watching gracefully.

### Arguments

| Argument | Description |
|----------|-------------|
| `<folder>` (required) | The directory to watch |

### Options

| Option | Short | Description |
|--------|-------|-------------|
| `--format <fmt>` | `-f` | **Required.** Target format. |
| `--output-dir <path>` | `-o` | Where to write converted files. Default: same folder as the input file. |
| `--recursive` | `-r` | Watch subdirectories as well. |
| `--profile <name>` | `-p` | Apply a named profile. |
| `--stable-time <ms>` | | Milliseconds a file's last-write time must be unchanged before Transmute processes it. Minimum: 50. Default: 500. |
| `--quality <0–100>` | `-q` | Lossy quality. |
| `--lossless` | | Lossless encoding. |
| `--overwrite` | | Overwrite existing output files. |
| `--metadata <mode>` | | Metadata handling. |
| `--jobs <n>` | `-j` | Parallel jobs. |
| `--verbose` | `-v` | Per-file detail output. |

### Ignored files

Watch mode automatically ignores files with these extensions, which are typically in-progress downloads or temporary writes:

`.tmp` `.part` `.crdownload` `.download` `.partial` `.!ut` `.!bt`

Output files generated by Transmute itself are also ignored to prevent conversion loops when the output directory is the same as the watch directory.

### Examples

```bash
# Basic watch
transmute watch "C:\Incoming" --format webp

# Watch and output to a separate folder
transmute watch ./uploads --format webp --output-dir ./converted

# Watch recursively, use a profile
transmute watch ./camera-roll --format jxl --recursive --profile archival

# Longer wait for slow network drives
transmute watch "\\NAS\Photos\Incoming" --format avif --stable-time 3000

# Verbose: see backend choices as each file comes in
transmute watch ./inbox --format webp --verbose
```

---

## `transmute info`

Show routing information for a file — what backend Transmute would choose and why.

```
transmute info <file>
```

### Arguments

| Argument | Description |
|----------|-------------|
| `<file>` (required) | The file to inspect |

### Output

```
File    : C:\Photos\IMG_4821.heic
Size    : 3.2 MB
Format  : heic
Backend : libvips (preferred for heic → *)
```

Any special notes are printed below — for example, if the extension suggests the file would be routed through ffmpeg as an animated format.

### Examples

```bash
transmute info portrait.heic
transmute info animation.gif
transmute info movie-frame.mp4
```

---

## `transmute backends`

List all supported backends and their availability on this machine.

```
transmute backends
```

### Output

```
Backend      Status       Input formats (sample)
-----------  -----------  -----------------------------------------------
libvips      Available    jpg, jpeg, png, tiff, tif, webp, avif, heif ...
cwebp/dwebp  Available    png, jpg, jpeg, tiff, bmp, ppm ...
cjxl/djxl    Not found    jxl, png, jpg, jpeg, apng, gif ...
ffmpeg       Available    gif, apng, webp, mp4, mkv, avi, mov ...
magick       Not found    (install ImageMagick and add to PATH)
```

**Available** means the binary was found on PATH or at the configured path. **Not found** means it isn't installed or isn't discoverable.

---

## `transmute config`

Read and write the global configuration file. Pass `--profile <name>` to read/write a named profile's values instead.

### `transmute config get [key]`

Print config values.

```bash
# Print entire config as JSON
transmute config get

# Print a single value
transmute config get defaults.webpQuality

# Print a value from a named profile
transmute config get --profile archival defaults.losslessDefault
```

### `transmute config set <key> <value>`

Set a config value.

```bash
# Set global WebP quality
transmute config set defaults.webpQuality 82

# Set a profile value
transmute config set --profile archival defaults.losslessDefault true

# Clear a profile override (revert to inheriting from global)
transmute config set --profile web defaults.webpQuality null

# Set a binary path
transmute config set binaries.cwebp "C:\tools\libwebp\bin\cwebp.exe"

# Set output directory globally
transmute config set defaults.defaultOutputDirectory "D:\Converted"
```

### `transmute config reset`

Reset the entire global config to defaults. Does not affect profiles.

```bash
transmute config reset
```

### `transmute config path`

Print the path to the active config file.

```bash
transmute config path
# C:\Users\me\AppData\Roaming\Transmute\config.json
```

### Config Keys

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `defaults.webpQuality` | int | `85` | WebP lossy quality |
| `defaults.jpegQuality` | int | `90` | JPEG quality |
| `defaults.jxlQuality` | int | `90` | JXL quality |
| `defaults.avifQuality` | int | `80` | AVIF quality |
| `defaults.metadataMode` | string | `PreserveAll` | `PreserveAll` \| `StripAll` \| `ColorProfile` \| `Copyright` |
| `defaults.overwriteExisting` | bool | `false` | Overwrite existing output files |
| `defaults.losslessDefault` | bool | `true` | Default lossless for JXL and WebP |
| `defaults.webpMethod` | int | `6` | WebP method 0–6 |
| `defaults.jxlEffort` | int | `7` | JXL effort 1–9 |
| `defaults.defaultOutputDirectory` | string? | `null` | Output directory (null = beside input) |
| `defaults.outputNamingPattern` | string | `{name}.{ext}` | Filename template |
| `processing.maxParallelJobs` | int | `0` | Parallel jobs (0 = CPU count) |
| `processing.vipsConcurrency` | int | `0` | libvips thread count (0 = auto) |
| `processing.tempDirectory` | string? | `null` | Temp dir (null = `%TEMP%\Transmute`) |
| `log.enabled` | bool | `false` | Write log file after conversion |
| `log.format` | string | `text` | `text` \| `json` |
| `binaries.cwebp` | string? | `null` | Path to cwebp.exe (null = PATH search) |
| `binaries.dwebp` | string? | `null` | Path to dwebp.exe |
| `binaries.cjxl` | string? | `null` | Path to cjxl.exe |
| `binaries.djxl` | string? | `null` | Path to djxl.exe |
| `binaries.ffmpeg` | string? | `null` | Path to ffmpeg.exe |
| `binaries.magick` | string? | `null` | Path to magick.exe |

---

## `transmute profile`

Create, manage, and inspect named profiles.

### `transmute profile list`

List all named profiles (excluding Default).

```bash
transmute profile list
```

### `transmute profile create <name>`

Create a new empty profile.

```bash
transmute profile create "social-media"
```

### `transmute profile duplicate <source> <name>`

Copy an existing profile (or Default) to a new name. Alias: `dup`.

```bash
transmute profile duplicate Default archival
transmute profile dup web-optimised web-optimised-v2
```

### `transmute profile rename <old-name> <new-name>`

Rename a profile.

```bash
transmute profile rename web-optimised web
```

### `transmute profile delete <name> [--yes]`

Delete a profile. Prompts for confirmation unless `--yes` / `-y` is passed.

```bash
transmute profile delete web
transmute profile delete web --yes
```

### `transmute profile show [name] [--effective]`

Display a profile's settings.

- Omit name or pass `Default` to see global defaults
- `--effective` shows the merged result (profile overrides + global fallbacks)

```bash
transmute profile show archival
transmute profile show archival --effective
transmute profile show Default
```

### `transmute profile path`

Print the path to the profiles folder.

```bash
transmute profile path
# C:\Users\me\AppData\Roaming\Transmute\Profiles
```
