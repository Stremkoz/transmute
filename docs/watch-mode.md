# Watch Mode

Watch mode monitors a folder for new image files and converts them automatically as they arrive. It's useful for automated workflows — camera card imports, download folders, network share inboxes, and any pipeline where files appear over time.

```bash
transmute watch <folder> --format <fmt> [options]
```

Press **Ctrl+C** to stop watching.

---

## How It Works

1. Transmute registers a file system watcher on the target folder
2. When a new file appears (or an existing file is modified), Transmute detects the event
3. It waits until the file has been stable — unchanged on disk — for a configurable window (default: 500ms)
4. Once stable, it checks whether the file is a recognised image format and not a temporary file
5. If it passes all checks, it queues the conversion and processes it

This debouncing step is important. Without it, Transmute might try to convert a file while it's still being written, resulting in a corrupted or partial output.

---

## Stability Check

The stability check polls the file's last-write time. A file is considered stable when:
- Its last-write timestamp has not changed for `--stable-time` milliseconds
- The file can be opened for reading (no exclusive write lock from another process)

If a file doesn't stabilise within 30 seconds, Transmute logs it as "skipped (timed out)" and moves on.

**Adjusting the stable time:**

```bash
# Default: 500ms — fine for local drives
transmute watch ./inbox --format webp

# Longer for slow networks or large files
transmute watch "\\NAS\Uploads" --format webp --stable-time 3000

# Minimum: 50ms — only for very fast local SSDs
transmute watch ./fast-inbox --format webp --stable-time 50
```

---

## Ignored Files

Watch mode automatically skips files with these extensions, which are typically partial downloads or temp writes:

| Extension | Used by |
|-----------|---------|
| `.tmp` | Generic temp files |
| `.part` | Firefox, various apps |
| `.crdownload` | Chrome downloads |
| `.download` | Safari downloads |
| `.partial` | Various download managers |
| `.!ut` | µTorrent |
| `.!bt` | BitTorrent clients |

Transmute's own output files are also ignored when the output directory is the same as the watch directory, preventing conversion loops.

---

## Output Location

By default, converted files are written beside their input files — in the same folder being watched. To separate inputs from outputs:

```bash
transmute watch ./inbox --format webp --output-dir ./converted
```

When using a separate output directory, Transmute won't loop back on its own output even if the output directory is inside the watch folder.

---

## Recursive Watching

```bash
transmute watch ./camera-roll --format jxl --recursive
```

With `--recursive`, Transmute watches all subdirectories. Files in any subdirectory (existing or created after the watcher starts) are processed.

---

## Profiles in Watch Mode

All profile and quality options work identically to `convert`:

```bash
# Use a profile's defaults
transmute watch ./uploads --format webp --profile web-optimised

# Use a profile but override quality
transmute watch ./uploads --format webp --profile web-optimised --quality 70
```

---

## Verbose Output

```bash
transmute watch ./inbox --format webp --verbose
```

Verbose mode shows a detailed block for each file as it's processed:

```
[1] ── IMG_4821.jpg ──
  Input:    C:\inbox\IMG_4821.jpg  (3.2 MB)
  Output:   C:\converted\IMG_4821.webp
  Backend:  libvips
  Reason:   libvips — preferred for .jpg → .webp
  Settings: quality=85, metadata=preserve-all
  Result:   0.44s  3.2MB→1.8MB (-44%)
```

---

## Common Use Cases

### Camera card import

Watch your photo import folder and convert everything to archival JXL as it arrives:

```bash
transmute watch "D:\Card Import" --format jxl --lossless --profile archival --output-dir "E:\Archive"
```

### Web output pipeline

Watch a raw assets folder and produce web-ready WebP automatically:

```bash
transmute watch "./raw-assets" --format webp -q 82 --metadata strip --output-dir "./public/images"
```

### Network share inbox

Wait a little longer for network stability, convert to AVIF:

```bash
transmute watch "\\fileserver\incoming" --format avif --stable-time 2000 --output-dir "\\fileserver\converted"
```

### Recursive folder with recursive output

Watch all subfolders and output to matching subfolders under a different root:

```bash
transmute watch "C:\Photos" --format webp --recursive --output-dir "D:\WebPhotos"
```

Output files mirror the input's path relative to the watch root — a file at `C:\Photos\2026\June\img.jpg` outputs to `D:\WebPhotos\2026\June\img.webp`.

---

## Stopping Watch Mode

Press **Ctrl+C**. Transmute finishes any conversions currently in progress and then exits cleanly.
