"""Builds the Windows downloads: the NSIS installer and a portable zip.

    python scripts/package.py            (or: npm run package)

1. Fetches the bundled tools (scripts/fetch_tools.py) into target/bundle-tools/.
2. Builds the release app and the NSIS installer with the tools as resources
   (src-tauri/tauri.bundle.conf.json), so they're installed beside the exe.
3. Zips the same exe and tools with a `portable` marker: unzipped anywhere,
   Transmute keeps its settings in a `data` folder beside itself.

Output: target/release/bundle/nsis/Transmute_<version>_x64-setup.exe and
target/release/bundle/portable/Transmute_<version>_x64-portable.zip.
"""

from __future__ import annotations

import json
import shutil
import subprocess
import sys
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TOOLS = ROOT / "target" / "bundle-tools" / "tools"
RELEASE = ROOT / "target" / "release"

PORTABLE_README = """Transmute, portable

Run transmute.exe. Settings, presets and run state are kept in the "data"
folder beside it, not in your user profile; the "portable" file switches
that on. Delete it to use %APPDATA%\\Transmute instead.

The converters Transmute uses are in "tools"; their licences are in
tools\\licenses. Transmute needs Microsoft Edge WebView2, which Windows 10
and 11 include.
"""


def run(*command: str) -> None:
    print("+", " ".join(command), flush=True)
    # npx is a .cmd file on Windows, which needs the shell to start.
    subprocess.run(command, cwd=ROOT, check=True, shell=sys.platform == "win32")


def version() -> str:
    conf = json.loads((ROOT / "src-tauri" / "tauri.conf.json").read_text(encoding="utf-8"))
    return conf["version"]


def portable_zip(name: str) -> Path:
    exe = RELEASE / "transmute.exe"
    if not exe.is_file():
        sys.exit(f"{exe} wasn't built")
    out = RELEASE / "bundle" / "portable" / f"{name}_x64-portable.zip"
    out.parent.mkdir(parents=True, exist_ok=True)
    top = f"{name}"
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED, compresslevel=9) as archive:
        archive.write(exe, f"{top}/transmute.exe")
        archive.writestr(f"{top}/portable", "")
        archive.writestr(f"{top}/README.txt", PORTABLE_README.replace("\n", "\r\n"))
        archive.write(ROOT / "LICENSE", f"{top}/LICENSE.txt")
        for path in sorted(TOOLS.rglob("*")):
            if path.is_file():
                archive.write(path, f"{top}/tools/{path.relative_to(TOOLS).as_posix()}")
    return out


def main() -> None:
    run(sys.executable, "-I", "scripts/fetch_tools.py")
    run("npx", "tauri", "build", "--config", "src-tauri/tauri.bundle.conf.json")
    name = f"Transmute_{version()}"
    installer = RELEASE / "bundle" / "nsis" / f"{name}_x64-setup.exe"
    if not installer.is_file():
        sys.exit(f"{installer} wasn't built")
    archive = portable_zip(name)
    for path in (installer, archive):
        print(f"{path.relative_to(ROOT)}: {path.stat().st_size / 1e6:.1f} MB")


if __name__ == "__main__":
    main()
