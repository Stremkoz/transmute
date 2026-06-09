# Profiles

A **profile** is a named set of conversion defaults. Instead of typing out quality, metadata, output directory, and other settings on every command, you save them once as a profile and reference it by name with `--profile <name>`.

Profiles are per-machine, stored in the Transmute profiles folder (run `transmute profile path` to see where). Each profile is a small JSON file.

---

## The Default Profile

The "Default" profile is not a real file — it's the global config defaults (`transmute config get`). When you don't pass `--profile`, these defaults apply. When you pass `--profile default`, you get the same thing explicitly.

You cannot delete or rename the Default profile. You configure it with `transmute config set`.

---

## Creating and Managing Profiles

### List profiles

```bash
transmute profile list
```

### Create a new profile

```bash
transmute profile create "web-optimised"
```

This creates an empty profile that inherits everything from global defaults. Then set the values you want to differ:

```bash
transmute config set --profile web-optimised defaults.webpQuality 80
transmute config set --profile web-optimised defaults.metadataMode StripAll
transmute config set --profile web-optimised defaults.defaultOutputDirectory "C:\Users\me\Desktop\converted"
```

### Duplicate an existing profile

Useful when you want a variation of an existing preset:

```bash
transmute profile duplicate Default archival
transmute config set --profile archival defaults.losslessDefault true
transmute config set --profile archival defaults.jxlEffort 9
```

You can also duplicate another named profile:

```bash
transmute profile duplicate web-optimised web-optimised-hq
transmute config set --profile web-optimised-hq defaults.webpQuality 92
```

### Rename a profile

```bash
transmute profile rename web-optimised web
```

### Delete a profile

```bash
transmute profile delete web
# Prompts for confirmation. To skip:
transmute profile delete web --yes
```

---

## Inspecting Profile Values

### Show raw profile values

```bash
transmute profile show web-optimised
```

This shows which values the profile explicitly sets. Values left at `null` are inherited from global config and shown as "inherited".

### Show effective values after merging

```bash
transmute profile show web-optimised --effective
```

This shows the actual value that would be used for each setting — profile override if set, global default otherwise.

---

## What a Profile Can Control

| Setting | Config key | Description |
|---------|-----------|-------------|
| WebP quality | `defaults.webpQuality` | 0–100 |
| JPEG quality | `defaults.jpegQuality` | 0–100 |
| JXL quality | `defaults.jxlQuality` | 0–100 |
| AVIF quality | `defaults.avifQuality` | 0–100 |
| Lossless default | `defaults.losslessDefault` | Boolean — applies to JXL and WebP |
| WebP method | `defaults.webpMethod` | 0–6 (speed/quality tradeoff) |
| JXL effort | `defaults.jxlEffort` | 1–9 (speed/size tradeoff) |
| Metadata mode | `defaults.metadataMode` | PreserveAll, StripAll, ColorProfile, Copyright |
| Overwrite | `defaults.overwriteExisting` | Boolean |
| Output directory | `defaults.defaultOutputDirectory` | Path or null (beside input) |
| Naming pattern | `defaults.outputNamingPattern` | Template string |
| Format filter | `skipFormats` / `onlyFormats` | See below |

---

## Format Filters

A profile can carry a format filter that limits which input files are processed during folder or batch conversions.

There are two filter modes — you can only use one at a time:

### skipFormats

Files whose extension appears in this list are silently ignored. Everything else is converted.

```bash
# Tell the profile to skip .gif and .webp files when converting a folder
transmute config set --profile web-optimised defaults.skipFormats "gif,webp"
```

Use this when your source folder has a mix of formats and you don't want to re-encode files that are already in a good format.

### onlyFormats

Only files whose extension appears in this list are processed. All other files are ignored.

```bash
# Only process JPEG files
transmute config set --profile jpeg-to-webp defaults.onlyFormats "jpg,jpeg"
```

### Filter priority

CLI flags always win:

```
--skip <exts>   Completely replaces the profile's filter for this run (sets skip mode)
--only <exts>   Completely replaces the profile's filter for this run (sets only mode)
```

This means you can override a profile's filter on a per-run basis without modifying the profile.

When a filter is active, Transmute prints a notice before conversion starts so it's always visible which files will be skipped.

---

## Using Profiles in the CLI

Pass `--profile <name>` to any `convert` or `watch` command. All profile settings become the defaults for that run. Any flag you also pass takes precedence over the profile:

```bash
# Use web-optimised defaults
transmute convert ./photos --format webp --profile web-optimised

# Use web-optimised defaults but override quality just for this run
transmute convert ./photos --format webp --profile web-optimised --quality 70

# Use with watch mode
transmute watch ./inbox --format webp --profile web-optimised --output-dir ./out
```

---

## Using Profiles in the GUI

The main window has a **Profile** dropdown. Selecting a profile instantly applies its defaults to the current session — quality slider, metadata mode, output directory, and format filter all update to match.

Profiles are also visible and manageable in the **Profile Manager** window (`Ctrl+Shift+P`).

---

## Profile File Format

Each profile is stored as a JSON file in the profiles folder. You can edit these files directly if you prefer:

```jsonc
{
  "name": "web-optimised",
  "webpQuality": 80,
  "jpegQuality": null,       // null = inherit from global config
  "jxlQuality": null,
  "avifQuality": null,
  "metadata": "StripAll",
  "overwriteExisting": null,
  "losslessDefault": null,
  "webpMethod": null,
  "jxlEffort": null,
  "defaultOutputDirectory": "C:\\Users\\me\\Desktop\\converted",
  "outputNamingPattern": null,
  "skipFormats": ["gif", "webp"],
  "onlyFormats": []
}
```

`null` means "inherit from global config". `skipFormats` and `onlyFormats` are always present as arrays — if both are empty, no filter is active.

---

## Example Profiles

### Web optimised

Strips all metadata for privacy, moderate quality, outputs beside input:

```bash
transmute profile create web
transmute config set --profile web defaults.webpQuality 82
transmute config set --profile web defaults.metadataMode StripAll
transmute config set --profile web defaults.losslessDefault false
```

### Archival lossless

Maximum quality JXL, all metadata preserved:

```bash
transmute profile create archival
transmute config set --profile archival defaults.losslessDefault true
transmute config set --profile archival defaults.jxlEffort 9
transmute config set --profile archival defaults.metadataMode PreserveAll
```

### JPEG-only conversion

Ignores everything that isn't a JPEG when batch-converting a mixed folder:

```bash
transmute profile create jpeg-only
transmute config set --profile jpeg-only defaults.onlyFormats "jpg,jpeg"
```

### Output to desktop

Converts to a fixed output directory regardless of where input files live:

```bash
transmute profile create to-desktop
transmute config set --profile to-desktop defaults.defaultOutputDirectory "C:\Users\me\Desktop\converted"
```
