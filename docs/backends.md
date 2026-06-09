# Backends

Transmute delegates the actual work of converting images to external tools called **backends**. It ships with support for five of them. You don't need all five — Transmute discovers what's available and routes each conversion to the most appropriate one. Missing backends are gracefully skipped (or fallen back from).

---

## The Five Backends

### libvips (NetVips)

The fastest general-purpose backend. Handles most static image formats without needing any other external binary.

**Best for:** JPEG, PNG, TIFF, AVIF, HEIF/HEIC, WebP (static), GIF (static), SVG, JPEG 2000, HDR

**Input formats:** jpg, jpeg, png, tiff, tif, webp, avif, heif, heic, bmp, gif, svg, pdf, ppm, pgm, pbm, hdr, jp2, j2k, and others

**Output formats:** jpg, jpeg, png, tiff, tif, webp, avif, heif, heic, gif, ppm, pbm, hdr, jp2, and others

**Notes:**
- This is the preferred backend for the vast majority of static image conversions
- Thread count is controlled by the `vipsConcurrency` config setting (0 = let libvips decide)
- Install: [github.com/libvips/libvips/releases](https://github.com/libvips/libvips/releases)
---

### cwebp / dwebp

Google's reference WebP encoder and decoder. Produces the highest quality WebP output at a given quality setting.

**Best for:** Any conversion where the input or output is WebP

**Input (cwebp):** png, jpg, jpeg, tiff, tif, bmp, ppm, pgm, pfm, pam — plus anything via a PNG intermediate

**Output:** webp only

**Notes:**
- Preferred over libvips specifically for WebP output because Google's own encoder typically achieves better compression
- Requires `cwebp.exe` and `dwebp.exe` on PATH or configured manually
- When the input format isn't natively supported by cwebp, Transmute converts it to a PNG intermediate first (via libvips), then feeds that PNG to cwebp
- Install: [developers.google.com/speed/webp](https://developers.google.com/speed/webp/docs/precompiled)

---

### cjxl / djxl

The reference JPEG XL encoder and decoder from the libjxl project.

**Best for:** Any conversion where the input or output is JXL

**Input (cjxl natively):** jxl, png, jpg, jpeg, apng, gif, exr, ppm, pfm, pgm, pgx, npy

**Output:** jxl, png, jpg, jpeg, ppm, pfm

**Notes:**
- Preferred for JXL output — this is the reference encoder and produces the best JXL results
- Supports both lossy (distance-based) and lossless encoding
- `--effort` / `-e` (1–9) controls the speed/size tradeoff: 1 is fastest, 9 is smallest file
- Like cwebp, formats not natively supported are pre-converted via a PNG intermediate
- Install: [github.com/libjxl/libjxl/releases](https://github.com/libjxl/libjxl/releases)

---

### ffmpeg

The multimedia powerhouse. Handles animated formats and video containers that no image-only tool can process.

**Best for:** Animated GIF, animated WebP, APNG, and extracting frames from video files

**Input:** gif, apng, webp (animated), mp4, mkv, avi, mov, wmv, flv, webm, m4v, ts, mpg, mpeg, 3gp, ogv, rm, rmvb, vob, mxf, asf, and more

**Output:** Same as input — ffmpeg handles the full round-trip for animated formats

**Notes:**
- Automatically selected whenever the input or output is an animated format or video container
- For still images, libvips is preferred; ffmpeg is only used when animation is in play
- `--stable-time` in watch mode helps avoid ffmpeg starting on a partial video upload
- Install: [ffmpeg.org/download.html](https://ffmpeg.org/download.html)

---

### ImageMagick

The broadest-format fallback. Handles formats that none of the specialist backends support.

**Best for:** Unusual or legacy formats — PSD, EPS, PDF, EXR, ICO, XPM, DCM, DPX, XCF, and many others

**Input / Output:** 60+ formats

**Notes:**
- Used as a last resort when the preferred backend isn't installed or can't handle the format pair
- Slowest of the five backends for common formats — do not force it with `--backend magick` for JPEG/PNG/WebP work
- Requires ImageMagick to be installed with the `magick` command on PATH
- Install: [imagemagick.org](https://imagemagick.org/script/download.php) — ensure "Install legacy utilities" is checked to get the `magick` command

---

## Backend Selection Logic

Transmute's **FormatRouter** picks a backend for each job using this priority order:

1. **Forced backend** — if `--backend <name>` is passed, that backend is used unconditionally (fails if unavailable)
2. **JXL affinity** — if input or output extension is `.jxl`, use cjxl/djxl
3. **WebP affinity** — if input or output extension is `.webp`, use cwebp/dwebp
4. **Animation / video affinity** — if the format is animated (gif, apng) or a video container, use ffmpeg
5. **Format-pair mapping** — the router consults a built-in registry that maps each format pair to the best available backend
6. **Fallback chain** — if the preferred backend isn't installed, try ImageMagick, then libvips

When no direct conversion path exists between two formats, Transmute performs a **two-step conversion**:
1. Convert the input to a PNG intermediate using the best available backend
2. Convert the PNG intermediate to the final output format using the appropriate backend

The intermediate file is stored in a temporary directory and cleaned up on exit. The `BackendUsed` field in results (and the `--verbose` output) shows both steps: e.g. `libvips → libvips` or `libvips → cwebp`.

---

## Checking Availability

```
transmute backends
```

This shows each backend, its availability status, and a sample of the formats it handles. Use it to diagnose why a backend isn't being used:

- **Available** (green): Binary found on PATH or at the configured path — ready to use
- **Not found** (red): Binary not on PATH and no manual path configured

---

## Configuring Binary Paths

If a backend binary isn't on your system PATH, you can tell Transmute exactly where to find it:

```bash
transmute config set binaries.cwebp  "C:\tools\libwebp\bin\cwebp.exe"
transmute config set binaries.dwebp  "C:\tools\libwebp\bin\dwebp.exe"
transmute config set binaries.cjxl   "C:\tools\libjxl\cjxl.exe"
transmute config set binaries.djxl   "C:\tools\libjxl\djxl.exe"
transmute config set binaries.ffmpeg "C:\tools\ffmpeg\bin\ffmpeg.exe"
transmute config set binaries.magick "C:\Program Files\ImageMagick-7.1.1-Q16-HDRI\magick.exe"
```

To revert to PATH-based discovery:

```bash
transmute config set binaries.cwebp null
```

---

## Forcing a Backend

You can override the router's decision for a specific conversion:

```bash
transmute convert image.png --format webp --backend vips
transmute convert image.jpg --format avif --backend magick
```

This is useful for:
- Testing whether a specific backend produces acceptable output for a format
- Working around a routing bug
- Benchmarking backend-to-backend quality differences

If the forced backend can't handle the format pair, conversion fails with an error rather than falling back.
