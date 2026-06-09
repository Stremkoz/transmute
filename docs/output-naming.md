# Output Naming Patterns

Transmute uses a template string to determine the filename of each output file. The default pattern is `{name}.{ext}`, which simply replaces the file extension.

---

## Available Tokens

| Token | Replaced with | Example |
|-------|--------------|---------|
| `{name}` | Input filename without its extension | `portrait` |
| `{ext}` | Output format extension | `webp` |
| `{original_ext}` | Input file extension (without dot) | `jpg` |
| `{date}` | Conversion date in `yyyyMMdd` format | `20260609` |
| `{counter}` | Sequential number, zero-padded to the total file count | `003` |

### Counter padding

The `{counter}` token is automatically padded to the width needed for the total number of files in the batch. For example:
- 9 files → `1` through `9` (no padding)
- 10–99 files → `01` through `99`
- 100–999 files → `001` through `999`
- 1000+ files → `0001` through `9999`, etc.

---

## Examples

| Pattern | Input (file 3 of 120) | Output |
|---------|----------------------|--------|
| `{name}.{ext}` *(default)* | `portrait.jpg` | `portrait.webp` |
| `{name}-web.{ext}` | `portrait.jpg` | `portrait-web.webp` |
| `{date}_{name}.{ext}` | `portrait.jpg` | `20260609_portrait.webp` |
| `{counter}_{name}.{ext}` | `portrait.jpg` | `003_portrait.webp` |
| `{name}.{original_ext}.{ext}` | `portrait.jpg` | `portrait.jpg.webp` |
| `{date}_{counter}.{ext}` | `portrait.jpg` | `20260609_003.webp` |
| `converted_{name}.{ext}` | `portrait.jpg` | `converted_portrait.webp` |

---

## Setting a Pattern

### Globally (applies to all conversions unless overridden)

```bash
transmute config set defaults.outputNamingPattern "{date}_{name}.{ext}"
```

### Per profile

```bash
transmute config set --profile web defaults.outputNamingPattern "{name}-web.{ext}"
```

### Per run (CLI)

```bash
transmute convert ./batch --format webp --name-pattern "{counter}_{name}.{ext}"
```

### In the GUI

Open **Settings** (`Ctrl+,`) → **Defaults** tab → **Output naming pattern** field.

---

## Combining with Output Directory

Naming patterns and output directories work independently. The pattern controls the filename only; the directory controls where the file is written.

```bash
# Rename files AND write to a specific folder
transmute convert ./originals --format webp \
  --output-dir ./web \
  --name-pattern "{date}_{name}.{ext}"
```

---

## Notes

- The pattern applies to the filename only — you cannot include path separators (`/` or `\`) in the pattern
- If two output files resolve to the same path (e.g. two inputs with the same `{name}` going to the same directory), only one will be written unless `--overwrite` is set
- The `{counter}` value is assigned in the order files are processed; for folder inputs this is filesystem enumeration order, which may not be alphabetical on all systems
