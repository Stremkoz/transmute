# Log Files

After a conversion batch, Transmute can write a log file summarising every result. Log files are useful for auditing large batch jobs, diagnosing failures, and feeding results into other tools (JSON format).

---

## Enabling Logs

### Permanently (via config)

```bash
transmute config set log.enabled true
transmute config set log.format text   # or: json
```

Or in the GUI: **Settings** → **Processing** tab → **Log file enabled** checkbox.

### Per run (CLI)

```bash
# Enable for this run (overrides config)
transmute convert . --format webp --log

# Disable for this run (overrides config)
transmute convert . --format webp --no-log

# Enable with JSON format
transmute convert . --format webp --log --log-format json
```

---

## Log File Location

Log files are written to the **output directory** — the same directory where converted files land. If no explicit output directory is set, the log goes beside the first output file.

The filename format is: `transmute-YYYYMMDD-HHmmss.log` (or `.json`)

For example: `transmute-20260609-143022.log`

---

## Text Format

```
Transmute Conversion Log
Generated : 2026-06-09 14:30:22
Duration  : 8.4s
Results   : 5 file(s)

  ✓ C:\Photos\img001.jpg
    → C:\Photos\img001.webp  [libvips]  2.1MB→1.4MB (-33%)  0.42s

  ✓ C:\Photos\img002.heic
    → C:\Photos\img002.webp  [libvips → libvips]  4.8MB→2.1MB (-56%)  1.18s

  ✓ C:\Photos\img003.png
    → C:\Photos\img003.webp  [cwebp]  3.6MB→2.2MB (-39%)  0.91s

  ⊘ C:\Photos\img004.webp
    → skipped (output already exists)

  ✗ C:\Photos\img005.psd
    → failed: no backend available for .psd → .webp

Summary: 3 succeeded, 1 skipped, 1 failed

⚠ img002.heic: two-step conversion via PNG intermediate (libvips → libvips)
```

### Status symbols

| Symbol | Meaning |
|--------|---------|
| `✓` | Conversion succeeded |
| `⊘` | Skipped — output already existed |
| `✗` | Failed — error message shown |

### Two-step conversions

When Transmute converts via a PNG intermediate (because no single backend handles the format pair directly), the backend is shown as `backend1 → backend2`, e.g. `libvips → cwebp`.

---

## JSON Format

```json
{
  "generated": "2026-06-09T14:30:22",
  "durationSeconds": 8.4,
  "results": [
    {
      "input": "C:\\Photos\\img001.jpg",
      "output": "C:\\Photos\\img001.webp",
      "status": "success",
      "backend": "libvips",
      "routingReason": "libvips — preferred for .jpg → .webp",
      "fallbackNote": null,
      "inputBytes": 2202000,
      "outputBytes": 1474000,
      "elapsedSeconds": 0.42,
      "error": null
    },
    {
      "input": "C:\\Photos\\img002.heic",
      "output": "C:\\Photos\\img002.webp",
      "status": "success",
      "backend": "libvips → libvips",
      "routingReason": "two-step via PNG intermediate",
      "fallbackNote": null,
      "inputBytes": 4800000,
      "outputBytes": 2100000,
      "elapsedSeconds": 1.18,
      "error": null
    },
    {
      "input": "C:\\Photos\\img004.webp",
      "output": "C:\\Photos\\img004.webp",
      "status": "skipped",
      "backend": null,
      "routingReason": null,
      "fallbackNote": null,
      "inputBytes": null,
      "outputBytes": null,
      "elapsedSeconds": 0.0,
      "error": null
    },
    {
      "input": "C:\\Photos\\img005.psd",
      "output": "C:\\Photos\\img005.webp",
      "status": "failed",
      "backend": null,
      "routingReason": null,
      "fallbackNote": null,
      "inputBytes": null,
      "outputBytes": null,
      "elapsedSeconds": 0.0,
      "error": "no backend available for .psd → .webp"
    }
  ],
  "summary": {
    "succeeded": 3,
    "skipped": 1,
    "failed": 1
  }
}
```

### JSON field reference

| Field | Type | Description |
|-------|------|-------------|
| `generated` | ISO 8601 string | When the log was written |
| `durationSeconds` | float | Total elapsed time for the batch |
| `results` | array | One entry per file |
| `results[].input` | string | Absolute input path |
| `results[].output` | string | Absolute output path |
| `results[].status` | string | `"success"`, `"skipped"`, or `"failed"` |
| `results[].backend` | string? | Backend used, e.g. `"libvips"` or `"libvips → cwebp"` |
| `results[].routingReason` | string? | Why this backend was chosen |
| `results[].fallbackNote` | string? | If the preferred backend was unavailable, notes the fallback |
| `results[].inputBytes` | int? | Input file size in bytes |
| `results[].outputBytes` | int? | Output file size in bytes |
| `results[].elapsedSeconds` | float | Time taken for this file |
| `results[].error` | string? | Error message if status is `"failed"` |
| `summary.succeeded` | int | Count of successful conversions |
| `summary.skipped` | int | Count of skipped files |
| `summary.failed` | int | Count of failures |

---

## Using JSON Logs in Scripts

The JSON format is designed for easy post-processing. Examples:

```powershell
# List all failed files
$log = Get-Content transmute-20260609-143022.json | ConvertFrom-Json
$log.results | Where-Object { $_.status -eq "failed" } | Select-Object input, error
```

```bash
# List failed files with jq
jq '.results[] | select(.status == "failed") | {input, error}' transmute-20260609-143022.json

# Calculate average compression ratio for successful conversions
jq '[.results[] | select(.status == "success") | .outputBytes / .inputBytes] | add / length' transmute-20260609-143022.json
```
