# Transmute 1.1.0

Transmute 1.1.0 is the cross-platform GUI release. The Windows-only WPF interface has been replaced by a new Avalonia UI, so the same graphical app can now be built for Windows, Linux, and macOS.

## Highlights

- New Avalonia GUI, published as `Transmute-GUI`.
- Cross-platform GUI builds for Windows, Linux, macOS Intel, and macOS Apple Silicon.
- CLI remains available as `transmute`.
- JPEG XL now has explicit distance control in addition to quality.
- JXL distance is available in the CLI, watch mode, config defaults, profiles, and the GUI.
- Improved GUI workflow with profile selection, queue management, drag/drop, advanced filters, settings, and log handling.

## JPEG XL Distance

JPEG XL output can now use cjxl's native distance setting:

- `0` = lossless
- `0.1` to `1.0` = visually lossless
- `1.1` to `2.0` = lossy

In the GUI, JXL still has a quality slider by default. Changing the distance slider makes distance override quality for that session.

In the CLI, use:

```bash
transmute convert ./photos --format jxl --distance 0.8
```

The same option is available in watch mode:

```bash
transmute watch ./inbox --format jxl --distance 0.8
```

## Release Assets

Each archive includes the GUI app, the CLI app, `README.md`, and `LICENSE`.

### Windows x64

Download `Transmute-v1.1.0-win-x64.zip`.

Included executables:

- `Transmute-GUI.exe` - graphical app
- `transmute.exe` - command-line app

### Linux x64

Download `Transmute-v1.1.0-linux-x64.tar.gz`.

Included executables:

- `Transmute-GUI` - graphical app
- `transmute` - command-line app

### macOS Intel

Download `Transmute-v1.1.0-osx-x64.tar.gz`.

Included executables:

- `Transmute-GUI` - graphical app
- `transmute` - command-line app

This package also includes the native Avalonia `.dylib` files required by the GUI.

### macOS Apple Silicon

Download `Transmute-v1.1.0-osx-arm64.tar.gz`.

Included executables:

- `Transmute-GUI` - graphical app
- `transmute` - command-line app

This package also includes the native Avalonia `.dylib` files required by the GUI.

## Checksums

```text
a77873046c1392fa2f4060f9bd9ff109d7aee6d5deeed8f6cdb40d9d02021741  Transmute-v1.1.0-linux-x64.tar.gz
2e7715adab9ead25c60ecd6c4f9516bbd41705e0ca7c2c55ef319f640a84f531  Transmute-v1.1.0-osx-arm64.tar.gz
ac23ce19794120c93f6c8f36d792e17dfc601844a30cf7c80d8f3d7cb051bb51  Transmute-v1.1.0-osx-x64.tar.gz
3bf463bf907ec63af10e1593ff426c2e8578095aed07e7a0287c41665371e832  Transmute-v1.1.0-win-x64.zip
```
