"""Fetches the encoders the Windows build ships with, into target/bundle-tools/.

The archives are pinned by version and SHA-256, so a build always bundles
exactly the tools it was tested with. Only licence-compatible tools are
bundled (gui-plan G5): libjxl and libwebp (BSD-3-Clause) and the libvips
"web" build (LGPL-2.1-or-later plus permissive libraries). ImageMagick and
FFmpeg stay optional installs.

Layout produced (flat, as discovery expects: <exe dir>/tools/<tool>.exe):

    target/bundle-tools/tools/cjxl.exe, djxl.exe, cwebp.exe, ...
    target/bundle-tools/tools/vips.exe + its DLLs + vips-modules-*/
    target/bundle-tools/tools/licenses/<package>/...

Usage: python scripts/fetch_tools.py [--out DIR]
Downloads are cached in target/tools-cache/ and re-checked on every run.
"""

from __future__ import annotations

import argparse
import hashlib
import shutil
import sys
import urllib.request
import zipfile
from dataclasses import dataclass, field
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parent.parent


@dataclass
class Package:
    name: str
    version: str
    licence: str
    source: str
    url: str
    sha256: str
    # Archive members (paths under `prefix`) copied flat into tools/.
    prefix: str
    files: list[str] = field(default_factory=list)
    # Copy everything under `prefix` (keeping subfolders), for DLL-heavy builds.
    everything: bool = False
    # Archive members copied into licenses/<name>/.
    licences: list[str] = field(default_factory=list)


@dataclass
class LicenceFile:
    package: str
    name: str
    url: str
    sha256: str


PACKAGES = [
    Package(
        name="libjxl",
        version="0.11.2",
        licence="BSD-3-Clause",
        source="https://github.com/libjxl/libjxl/tree/v0.11.2",
        url="https://github.com/libjxl/libjxl/releases/download/v0.11.2/jxl-x64-windows-static.zip",
        sha256="97dc815bdd99ba243d8502050357342cf649251a5df069f8c3daee6828cbe0ce",
        prefix="bin/",
        files=["cjxl.exe", "djxl.exe"],
        licences=[
            "LICENSE.libjxl",
            "LICENSE.brotli",
            "LICENSE.giflib",
            "LICENSE.highway",
            "LICENSE.libjpeg-turbo",
            "LICENSE.libpng",
            "LICENSE.libwebp",
            "LICENSE.sjpeg",
            "LICENSE.skcms",
            "LICENSE.zlib",
        ],
    ),
    Package(
        name="libwebp",
        version="1.6.0",
        licence="BSD-3-Clause",
        source="https://chromium.googlesource.com/webm/libwebp/+/refs/tags/v1.6.0",
        url="https://storage.googleapis.com/downloads.webmproject.org/releases/webp/libwebp-1.6.0-windows-x64.zip",
        sha256="48886f506b21f62e4661f0f4cbfca19800897c385128e8902542d29a950c93f1",
        prefix="libwebp-1.6.0-windows-x64/bin/",
        files=["cwebp.exe", "dwebp.exe", "gif2webp.exe"],
    ),
    Package(
        name="libvips",
        version="8.18.1",
        licence="LGPL-2.1-or-later (bundled libraries: LGPL and permissive; see versions.json)",
        source="https://github.com/libvips/build-win64-mxe/tree/v8.18.1",
        url="https://github.com/libvips/build-win64-mxe/releases/download/v8.18.1/vips-dev-w64-web-8.18.1.zip",
        sha256="3be975649ad8215db98ded394d814b93e4e5313e79a3f15026a65024084b5437",
        prefix="vips-dev-8.18/bin/",
        everything=True,
        licences=["../LICENSE", "../versions.json"],
    ),
]

LICENCE_FILES = [
    LicenceFile(
        "libwebp",
        "COPYING",
        "https://raw.githubusercontent.com/webmproject/libwebp/v1.6.0/COPYING",
        "5aec868f669e384a22372a4e8a1a6cd7d44c64cd451f960ca69cc170d1e13acf",
    ),
    LicenceFile(
        "libwebp",
        "PATENTS",
        "https://raw.githubusercontent.com/webmproject/libwebp/v1.6.0/PATENTS",
        "cc3273e0694ea5896145e0677699b53471b03ea43021ddc50e7923fbb9f5023c",
    ),
]

# Not needed to convert, so not shipped (they only make the download bigger).
VIPS_SKIP = {"vipsedit.exe", "vipsthumbnail.exe"}


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as file:
        for block in iter(lambda: file.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def fetch(url: str, expected: str, cache: Path) -> Path:
    """Downloads `url` into the cache (once) and checks its hash."""
    target = cache / f"{expected[:16]}-{PurePosixPath(url).name}"
    if not target.is_file() or sha256(target) != expected:
        print(f"downloading {url}")
        partial = target.with_suffix(target.suffix + ".part")
        with urllib.request.urlopen(url, timeout=120) as response, partial.open("wb") as out:
            shutil.copyfileobj(response, out)
        actual = sha256(partial)
        if actual != expected:
            partial.unlink()
            sys.exit(f"{url}: SHA-256 is {actual}, expected {expected}")
        partial.replace(target)
    return target


def member_path(prefix: str, name: str) -> str:
    """`name` relative to `prefix`, resolving a leading `../`."""
    path = PurePosixPath(prefix.rstrip("/"))
    for part in PurePosixPath(name).parts:
        path = path.parent if part == ".." else path / part
    return str(path)


def unpack(package: Package, archive: Path, tools: Path) -> list[str]:
    licences = tools / "licenses" / package.name
    licences.mkdir(parents=True, exist_ok=True)
    copied: list[str] = []
    with zipfile.ZipFile(archive) as zip_file:
        names = set(zip_file.namelist())
        if package.everything:
            wanted = [n for n in names if n.startswith(package.prefix) and not n.endswith("/")]
            wanted = [n for n in wanted if PurePosixPath(n).name not in VIPS_SKIP]
        else:
            wanted = [package.prefix + f for f in package.files]
        for name in sorted(wanted):
            if name not in names:
                sys.exit(f"{package.name}: {name} is missing from {archive.name}")
            relative = PurePosixPath(name).relative_to(package.prefix.rstrip("/"))
            # Archive paths come from a pinned, hash-checked file, but stay inside tools/ anyway.
            if ".." in relative.parts:
                sys.exit(f"{package.name}: refusing {name}")
            out = tools / Path(*relative.parts)
            out.parent.mkdir(parents=True, exist_ok=True)
            with zip_file.open(name) as source, out.open("wb") as target:
                shutil.copyfileobj(source, target)
            copied.append(str(relative))
        for licence in package.licences:
            name = member_path(package.prefix, licence)
            if name not in names:
                sys.exit(f"{package.name}: licence {name} is missing")
            (licences / PurePosixPath(name).name).write_bytes(zip_file.read(name))
    return copied


def notices(tools: Path) -> None:
    lines = [
        "# Third-party tools bundled with Transmute",
        "",
        "Transmute runs these programs as separate processes. Each keeps its own licence;",
        "the full texts are in the folder named after it. Their source code is at the",
        "links below. The libvips DLLs can be replaced with compatible builds of your own.",
        "",
    ]
    for package in PACKAGES:
        lines += [
            f"## {package.name} {package.version}",
            "",
            f"- Licence: {package.licence}",
            f"- Source: {package.source}",
            f"- Binaries: {package.url}",
            "",
        ]
    (tools / "licenses" / "README.md").write_text("\n".join(lines), encoding="utf-8", newline="\n")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--out", type=Path, default=ROOT / "target" / "bundle-tools")
    args = parser.parse_args()
    cache = ROOT / "target" / "tools-cache"
    cache.mkdir(parents=True, exist_ok=True)
    tools = args.out / "tools"
    if tools.exists():
        shutil.rmtree(tools)
    tools.mkdir(parents=True)
    for package in PACKAGES:
        copied = unpack(package, fetch(package.url, package.sha256, cache), tools)
        print(f"{package.name} {package.version}: {len(copied)} files")
    for licence in LICENCE_FILES:
        path = fetch(licence.url, licence.sha256, cache)
        (tools / "licenses" / licence.package / licence.name).write_bytes(path.read_bytes())
    notices(tools)
    size = sum(p.stat().st_size for p in tools.rglob("*") if p.is_file())
    print(f"bundled tools in {tools} ({size / 1e6:.0f} MB)")


if __name__ == "__main__":
    main()
