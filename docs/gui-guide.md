# GUI Guide

The Transmute GUI (`transmute-avalonia.exe`) provides a drag-and-drop interface for interactive image conversion. This guide walks through every part of the interface.

> The original WPF GUI (`Transmute.GUI.exe`) is archived and retired. The Avalonia GUI is
> the actively developed, cross-platform replacement.

---

## Main Window

```
┌─────────────────────────────────────────────────────────┐
│  ⚡ Transmute              Profile: [Default ▼]  🗂 ⚙    │
├───────────────────────────────────────────────────────────┤
│                                                           │
│   Queue                              + Add Files  📁 Add │
│   ──────────────────────                          Folder │
│   ⠿ ▣ photo001.jpg                          1.8 MB   ✕   │
│   ⠿ ▣ photo002.heic                         3.2 MB   ✕   │
│   ⠿ 📁 my-folder (12 images)                         ✕   │
│                                                           │
├───────────────────────────────────────────────────────────┤
│  Format: [WebP ▼]  [Lossless|Lossy]  Quality [══════]85% │
│  Format: [JXL ▼]                    Quality [══════]90%  │
│                                      Distance [═════]0.8 │
│                                          Metadata: [Keep ▼]│
│  Output: [______________________] Browse… ↩ Clear 📂 Open│
├───────────────────────────────────────────────────────────┤
│  ☐ Include subfolders   [⚙ Advanced]   [Clear ▾]  ⚡Convert│
├───────────────────────────────────────────────────────────┤
│  ☐ Overwrite existing (session)  | [Skip|Only] input      │
│                                     formats: [JPG][PNG]... │
├───────────────────────────────────────────────────────────┤
│  Log ──────────────────────────────────────────────────  │
│  [1/3] photo001.jpg → photo001.webp [libvips] 0.42s      │
│  [2/3] photo002.heic → photo002.webp [libvips] 1.18s     │
├───────────────────────────────────────────────────────────┤
│  Status bar                                  [██░░] Cancel│
└─────────────────────────────────────────────────────────┘
```

---

## Adding Files to the Queue

### Drag and Drop

Drag images or folders directly onto the window. Folders are added as folder entries
(expanded according to **Include subfolders**). You can also drag multiple items at once.

### Add Files / Add Folder

Click **+ Add Files** or **📁 Add Folder** (shown in the empty drop zone and in the queue
header) to browse for files and folders via the OS picker. The file picker supports
multi-select.

### Supported Drop Content

- Individual image files
- Folders (added as a folder entry; contents are counted and expanded at conversion time)
- Mixed selections of files and folders

---

## The Queue

The queue shows all files and folders waiting to be converted.

- **File rows** show a thumbnail, filename, path, and size.
- **Folder rows** show a folder icon, name, item count, and path.

**Reordering:** Grab the gripper icon (⠿) on the left of any row and drag it up or down to
change the conversion order. Multi-select and drag to move several rows at once.
Reordering is disabled while conversion is in progress.

**Removing items:**
- Click the **✕** button on a row to remove that entry
- Select one or more rows and press **Delete** to remove the selection
- Right-click a row for a context menu: **Remove**, **Open containing folder**,
  **Select all**, **Clear queue**

---

## Format and Quality

**Format dropdown:** Select the target output format. All formats supported by your
installed backends are listed.

**Lossless / Lossy pill:** WebP has a segmented pill that lets you pick **Lossless** or
**Lossy**. Exactly one is selected at all times. In Lossy mode the quality slider is
enabled.

**Quality slider:** Sets the lossy quality level (1–100). Disabled when **Lossless** is
selected.

**JXL distance slider:** JPEG XL still has the quality slider. Changing the distance
slider overrides quality for that session and sends cjxl's native `-d` value instead.
`0` is lossless, `0.1`–`1.0` is visually lossless, and `1.1`–`2.0` is lossy. Lower
values preserve more detail and produce larger files.

**Metadata dropdown:** Controls how EXIF/ICC/XMP metadata is carried over to the output.

---

## Profile Selector

The **Profile** dropdown (top-right of the toolbar) lists all named profiles. Selecting a
profile immediately applies its defaults to the session:

- Quality, lossless, and JXL distance settings update
- Format filter updates
- Output directory updates (if the profile sets one)

These are session defaults only — they reset to the Default profile's values when you
relaunch the app. Use **⚙ Settings** to change permanent global defaults.

---

## Output Directory

Set a custom output folder, or leave it empty to save converted files beside their
sources.

- **Browse…** opens a folder picker
- **↩ Clear** resets to "beside source"
- **📂 Open** opens the configured output folder in your file manager (shown only when a
  folder is set)

---

## Advanced Panel

Click **⚙ Advanced** to show session-specific options that reset when you close the app.

### Overwrite existing (session)

When checked, output files that already exist are re-encoded for this session. When
unchecked (default), they are skipped with a "skipped" entry in the log.

### Input Format Filter

A **Skip / Only** segmented pill plus a row of format chips (JPG, PNG, GIF, WebP, AVIF,
JXL, HEIC) limits which input files are processed:

- **Skip** — files matching the checked chips are ignored; everything else is converted
- **Only** — only files matching the checked chips are converted; everything else is
  ignored

Exactly one of Skip/Only is selected at all times. A hint next to the chips explains the
current mode's effect.

**Note:** If the active profile already has a format filter, the GUI shows it here.
Changing the chips overrides the profile filter for the current session.

---

## Convert Button

Click **⚡ Convert** to start. The progress bar and log panel activate immediately.

If any files are in a same-format conversion (e.g. converting WebP files to WebP), a
warning dialog appears before conversion begins asking whether to proceed.

### Clear Button

The split **Clear** button clears the log by default. Click the **▾** chevron for more
options: clear log, clear queue, clear completed entries from the log, or clear
everything.

---

## Progress Bar & Status Bar

The status bar shows the current status text and overall batch progress. While converting,
a **Cancel** button appears to stop the batch.

---

## Log Panel

The log panel shows live output during and after conversion:

```
[1/3] photo001.jpg → photo001.webp [libvips]  1.8MB→1.1MB (-39%)  0.42s
[2/3] photo002.heic → photo002.webp [libvips → libvips]  3.2MB→1.6MB (-50%)  1.18s
[3/3] SKIPPED photo003.webp (output already exists)

Done: 2 succeeded, 1 skipped, 0 failed — 1.6s  (-44% average)
```

The log auto-scrolls to the latest entry. You can scroll up freely; it resumes
auto-scrolling when you scroll back to the bottom.

**Resizing the log panel:** Drag the horizontal splitter between the queue/controls area
and the log panel to give it more or less space.

---

## Settings Window

Open via the **⚙ Settings** button in the toolbar.

Settings are organised into tabs:

### Binaries tab

Configure paths to external backend binaries. Leave blank to auto-discover from PATH.

- cwebp / dwebp path
- cjxl / djxl path
- ffmpeg path
- magick path

A **Binary Downloads** window (linked from this tab) provides direct links to the
download pages for each optional backend.

### Processing tab

- **Max parallel jobs** — how many files convert simultaneously (0 = one per CPU)
- **libvips concurrency** — threads used by libvips per job (0 = auto)
- **Temp directory** — where intermediate files go during two-step conversions
- **Log file enabled** — write a log file after every conversion batch
- **Log format** — Text or JSON
- **Play sound on completion** — plays a system sound when a batch finishes (success
  chime, or a warning sound if there were failures or routing fallbacks)

### Defaults tab

Global conversion defaults. These apply unless overridden by a profile or per-session
controls. Edits apply to the profile selected in the **Editing profile** banner at the
top of the tab.

- Quality per format (WebP, JPEG, JXL, AVIF)
- Compression effort (WebP method, JXL effort)
- JXL distance for the default lossy JPEG XL mode
- Metadata mode
- Overwrite existing
- Lossless default (for JXL and WebP)
- Output naming pattern
- Default output directory
- Input format filter (named profiles only) — Skip/Only segmented pill + comma-separated
  extensions

### Appearance tab

- **Colour scheme** — System (follows the OS light/dark setting and updates
  automatically), Light, or Dark. The theme previews immediately; click **Save** to
  persist it.

---

## Profile Manager

Open via **🗂 Profiles** in the toolbar.

The Profile Manager lists all named profiles and lets you:

- **New Profile** — create a new empty profile
- **Duplicate** — copy an existing profile (including Default) as a starting point
- **Rename** a profile
- **Delete** a profile (with confirmation)

A warning banner appears if the selected profile has a Skip or Only format filter active,
so you know which formats will be affected.

**📂 Open Profiles Folder** opens the folder containing the profile JSON files in your
file manager.

---

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Delete` | Remove selected queue items |

---

## Dark Mode

Transmute supports full dark/light theming across all windows — main window, Settings,
Profile Manager, and dialogs. The theme is controlled by the **Colour scheme** setting in
Settings → Appearance:

- **System** — automatically follows the OS theme setting
- **Light** — always light
- **Dark** — always dark

Theme changes take effect immediately without restarting the app.
