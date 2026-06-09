# GUI Guide

The Transmute GUI (`Transmute.GUI.exe`) provides a drag-and-drop interface for interactive image conversion. This guide walks through every part of the interface.

---

## Main Window

```
┌─────────────────────────────────────────────────────────┐
│  [Open ▼]  Format: [WebP ▼]  Quality: [85 ══════]       │
│            Profile: [Default ▼]  [☐ Lossless]           │
├──────────────────────────────┬──────────────────────────┤
│                              │  Output                   │
│   Queue                      │  ─────                    │
│   ──────────────────────     │  [☐ Overwrite existing]   │
│   ≡  photo001.jpg            │                           │
│   ≡  photo002.heic           │  Format Filter            │
│   ≡  photo003.png     [✕]    │  ─────────────────        │
│                              │  Mode: [Skip ▼]           │
│                              │  [ ] JPEG  [ ] PNG        │
│                              │  [ ] GIF   [ ] WebP       │
│                              │  [ ] AVIF  [ ] JXL        │
│                              │  [ ] HEIC                 │
├─────────────────────────────────────────────────────────┤
│  [████████████████░░░░░░░░░]  Converting 2/3...          │
├─────────────────────────────────────────────────────────┤
│  Log ────────────────────────────────────────────────── │
│  [1/3] photo001.jpg → photo001.webp [libvips] 0.42s     │
│  [2/3] photo002.heic → photo002.webp [libvips] 1.18s    │
│  ▼ auto-scroll                                           │
├─────────────────────────────────────────────────────────┤
│  Status bar                                              │
└─────────────────────────────────────────────────────────┘
```

---

## Adding Files to the Queue

### Drag and Drop

Drag images or folders directly onto the window from File Explorer. Folders are expanded — all recognised image files inside are added. You can also drag multiple items at once.

### Open Dialog

Click the **Open** button (or press **Ctrl+O**) to browse for files and folders. The dialog supports multi-select.

### Supported Drop Content

- Individual image files
- Folders (top-level images are added; use the queue to see what was picked up)
- Mixed selections of files and folders

---

## The Queue

The queue shows all files waiting to be converted. Each row shows the filename and a remove button.

**Reordering:** Grab the gripper icon (≡) on the left of any row and drag it up or down to change the conversion order. Reordering is disabled while conversion is in progress.

**Removing items:**
- Click the **✕** button on a row to remove that file
- Select one or more rows and press **Delete** to remove the selection
- The queue is cleared automatically when a new conversion starts (completed items are replaced with fresh results)

---

## Format and Quality

**Format dropdown:** Select the target output format. All formats supported by your installed backends are listed.

**Quality slider:** Sets the lossy quality level (0–100). The slider is visible for lossy formats (WebP, JXL, AVIF, JPEG). The displayed value updates in real time.

**Lossless checkbox:** Switches to lossless encoding. Available for WebP and JXL. When checked, the quality slider is disabled.

---

## Profile Selector

The **Profile** dropdown lists all named profiles. Selecting a profile immediately applies its defaults to the session:

- Quality slider updates to the profile's quality setting
- Lossless checkbox updates
- Format filter updates
- Output directory updates (if the profile sets one)

These are session defaults only — they reset to the Default profile's values when you relaunch the app. Use **Settings** to change permanent global defaults.

---

## Advanced Panel

The right-hand panel holds session-specific options that reset when you close the app.

### Overwrite existing

When checked, output files that already exist are re-encoded. When unchecked (default), they are skipped with a "skipped" entry in the log.

### Format Filter

Limits which input files are processed based on their extension.

**Mode dropdown:**
- **Skip** — files with the checked extensions are ignored; everything else is converted
- **Only** — only files with the checked extensions are converted; everything else is ignored

**Format checkboxes:** JPEG, PNG, GIF, WebP, AVIF, JXL, HEIC

When a filter is active, a notice appears in the log before conversion begins so you can see exactly what's being skipped.

**Note:** If the selected profile already has a format filter, the GUI shows it here. Changing the checkboxes overrides the profile filter for the current session.

---

## Convert Button

Click **Convert** to start. The progress bar and log panel activate immediately.

If any files are in a same-format conversion (e.g. converting WebP files to WebP), a warning dialog appears before conversion begins asking whether to proceed. You can skip it or confirm.

---

## Progress Bar

Shows overall batch progress (completed / total). The status bar below it shows the current file count and any in-progress label.

---

## Log Panel

The log panel shows live output during and after conversion:

```
[1/3] photo001.jpg → photo001.webp [libvips]  1.8MB→1.1MB (-39%)  0.42s
[2/3] photo002.heic → photo002.webp [libvips → libvips]  3.2MB→1.6MB (-50%)  1.18s
[3/3] SKIPPED photo003.webp (output already exists)

Done: 2 succeeded, 1 skipped, 0 failed — 1.6s  (-44% average)
```

- **Green** — successful conversion with size delta
- **Yellow / orange** — skipped (output already existed)
- **Red** — failed with error message
- **Two-step conversions** show `backend → backend` (e.g. `libvips → cwebp`)

The log auto-scrolls to the latest entry. You can scroll up freely; it will resume auto-scrolling when you scroll back to the bottom.

**Resizing the log panel:** Drag the horizontal splitter between the main content area and the log panel to give it more or less space.

---

## Settings Window

Open with **Ctrl+,** or via the menu.

Settings are organised into tabs:

### Binaries tab

Configure paths to external backend binaries. Leave blank to auto-discover from PATH.

- cwebp path
- dwebp path
- cjxl path
- djxl path
- ffmpeg path
- magick path

### Processing tab

- **Max parallel jobs** — how many files convert simultaneously (0 = one per CPU)
- **libvips concurrency** — threads used by libvips per job (0 = auto)
- **Temp directory** — where intermediate files go during two-step conversions (blank = `%TEMP%\Transmute`)
- **Log file enabled** — write a log file after every conversion batch
- **Log format** — Text or JSON

### Defaults tab

Global conversion defaults. These apply unless overridden by a profile or per-session controls.

- Quality per format (WebP, JPEG, JXL, AVIF)
- Lossless default (for JXL and WebP)
- WebP method (0–6)
- JXL effort (1–9)
- Metadata mode
- Overwrite existing
- Default output directory
- Output naming pattern

### UI tab

- **Theme** — System (respects Windows dark mode), Light, or Dark
- **Play sound on completion** — plays a system notification sound when a batch finishes

### Config file path

The full path to the active config file is shown at the bottom of the Settings window for reference.

---

## Profile Manager

Open with **Ctrl+Shift+P** or via the menu.

The Profile Manager lists all named profiles and lets you:

- **Create** a new empty profile
- **Duplicate** an existing profile (including Default) as a starting point
- **Rename** a profile
- **Delete** a profile (with confirmation)

Select a profile to see its format filter (if any). A warning banner appears if the profile has a Skip or Only filter active, so you know which formats will be affected.

To edit a profile's quality, metadata, or other values, use `transmute config set --profile <name>` from the CLI (or edit the JSON file directly in the profiles folder).

---

## Binary Downloads Window

Accessible from the **Binaries** tab in Settings, this window provides direct links to the download pages for each optional backend.

---

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+O` | Open files / folders dialog |
| `Ctrl+,` | Open Settings |
| `Ctrl+Shift+P` | Open Profile Manager |
| `Delete` | Remove selected queue items |

---

## Taskbar Notification

When a conversion batch completes and the window is not in the foreground, the Transmute taskbar button flashes to let you know the job is done.

---

## Dark Mode

Transmute supports full dark mode across all windows — main window, Settings, Profile Manager, and dialogs. The theme is controlled by the **Theme** setting:

- **System** — automatically follows the Windows theme setting
- **Light** — always light
- **Dark** — always dark

Theme changes take effect immediately without restarting the app.
