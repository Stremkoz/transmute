"""Builds the benchmark corpus for `cargo dev bench` (milestone M9).

Downloads the images listed in scripts/bench-corpus.json and generates a few
more with ImageMagick, then writes every image as an 8-bit sRGB PNG into
bench-data/corpus/<class>/. The folder is gitignored: nothing downloaded is
committed.

    python scripts/bench_corpus.py            # fetch what's missing
    python scripts/bench_corpus.py --force    # rebuild everything

Needs Python 3.10+ and ImageMagick (`magick`) on PATH.
"""

import argparse
import hashlib
import json
import subprocess
import sys
import tempfile
import time
import urllib.parse
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MANIFEST = ROOT / "scripts" / "bench-corpus.json"
CORPUS = ROOT / "bench-data" / "corpus"
USER_AGENT = "TransmuteBench/0.1 (image codec benchmark; github.com/Stremkoz)"
COMMONS_API = "https://commons.wikimedia.org/w/api.php"
# Only licences that need no attribution or share-alike handling.
ALLOWED_LICENCES = {"CC0", "Public domain"}
# Be polite to the servers.
PAUSE_SECONDS = 1.5


def fetch(url: str) -> bytes:
    request = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    for attempt in range(4):
        try:
            with urllib.request.urlopen(request, timeout=120) as response:
                return response.read()
        except OSError as err:
            if attempt == 3:
                raise
            print(f"  retrying after {err}", file=sys.stderr)
            time.sleep(PAUSE_SECONDS * (attempt + 2) * 2)
    raise AssertionError("unreachable")


def magick(*args: str | Path) -> None:
    subprocess.run(["magick", *map(str, args)], check=True)


def normalise(raw: Path, out: Path, flatten: bool) -> None:
    """Any input → 8-bit sRGB PNG, upright, without metadata."""
    out.parent.mkdir(parents=True, exist_ok=True)
    args: list[str | Path] = [raw, "-auto-orient", "-colorspace", "sRGB"]
    if flatten:
        args += ["-background", "white", "-alpha", "remove", "-alpha", "off"]
    args += ["-depth", "8", "-strip", out]
    magick(*args)


def slug(text: str) -> str:
    keep = [c.lower() if c.isascii() and c.isalnum() else "-" for c in text]
    return "-".join(part for part in "".join(keep).split("-") if part)[:60]


def kodak(spec: dict, force: bool, lock: dict) -> None:
    for n in range(1, spec["count"] + 1):
        out = CORPUS / spec["class"] / f"kodim{n:02}.png"
        if out.exists() and not force:
            continue
        url = spec["url"].format(n)
        print(f"kodak {n:02}")
        data = fetch(url)
        with tempfile.TemporaryDirectory() as work:
            raw = Path(work) / "raw.png"
            raw.write_bytes(data)
            normalise(raw, out, flatten=True)
        lock[out.relative_to(CORPUS).as_posix()] = {
            "source": url,
            "sha256": hashlib.sha256(data).hexdigest(),
            "licence": spec["licence"],
        }
        time.sleep(PAUSE_SECONDS)


def commons(entries: list, force: bool, lock: dict) -> None:
    for entry in entries:
        title = entry["title"]
        name = slug(title.removeprefix("File:").rsplit(".", 1)[0]) or "image"
        out = CORPUS / entry["class"] / f"{name}.png"
        if out.exists() and not force:
            continue
        query = urllib.parse.urlencode(
            {
                "action": "query",
                "format": "json",
                "titles": title,
                "prop": "imageinfo",
                "iiprop": "url|extmetadata",
                "iiextmetadatafilter": "LicenseShortName",
                "iiurlwidth": entry["width"],
            }
        )
        info = json.loads(fetch(f"{COMMONS_API}?{query}"))
        page = next(iter(info["query"]["pages"].values()))
        if "imageinfo" not in page:
            print(f"skipped {title}: not found on Commons", file=sys.stderr)
            continue
        image = page["imageinfo"][0]
        licence = image.get("extmetadata", {}).get("LicenseShortName", {}).get("value", "")
        if licence not in ALLOWED_LICENCES:
            print(f"skipped {title}: licence is {licence!r}", file=sys.stderr)
            continue
        url = image.get("thumburl") or image["url"]
        print(f"commons {entry['class']}: {title} ({licence})")
        time.sleep(PAUSE_SECONDS)
        data = fetch(url)
        with tempfile.TemporaryDirectory() as work:
            raw = Path(work) / ("raw" + Path(urllib.parse.urlparse(url).path).suffix)
            raw.write_bytes(data)
            normalise(raw, out, flatten=entry["class"] != "transparent")
        lock[out.relative_to(CORPUS).as_posix()] = {
            "source": f"https://commons.wikimedia.org/wiki/{urllib.parse.quote(title)}",
            "sha256": hashlib.sha256(data).hexdigest(),
            "licence": licence,
        }
        time.sleep(PAUSE_SECONDS)


def generated(force: bool, lock: dict) -> None:
    """Flat-colour art and transparent graphics that don't depend on any
    download: sharp edges, text, gradients and soft alpha."""
    jobs = {
        "illustration/generated-chart.png": [
            "-size", "1600x1000", "xc:#f7f7f2",
            "-fill", "#2b6cb0", "-draw", "rectangle 150,600 300,900",
            "-fill", "#dd6b20", "-draw", "rectangle 400,350 550,900",
            "-fill", "#38a169", "-draw", "rectangle 650,450 800,900",
            "-fill", "#805ad5", "-draw", "rectangle 900,200 1050,900",
            "-stroke", "#222", "-strokewidth", "4", "-draw", "line 100,900 1500,900",
            "-stroke", "none", "-fill", "#222", "-pointsize", "56",
            "-annotate", "+150+120", "Quarterly results 2026",
            "-pointsize", "30", "-annotate", "+1150+300", "Sharp text, flat\ncolours and thin\nlines are hard\nfor photo codecs.",
        ],
        "illustration/generated-gradient-poster.png": [
            "-size", "1200x1600", "gradient:#1a2a6c-#fdbb2d",
            "-fill", "#ffffffcc", "-draw", "circle 600,600 600,250",
            "-fill", "#1a2a6c", "-pointsize", "110", "-gravity", "south",
            "-annotate", "+0+180", "TRANSMUTE",
        ],
        "illustration/generated-ui.png": [
            "-size", "1440x900", "xc:#ffffff",
            "-fill", "#f0f2f5", "-draw", "rectangle 0,0 260,900",
            "-fill", "#1f6feb", "-draw", "roundrectangle 300,40 520,90 8,8",
            "-fill", "#d0d7de", "-draw", "rectangle 300,130 1400,131",
            "-fill", "#24292f", "-pointsize", "22",
            "-annotate", "+30+60", "Inbox\n\nDrafts\n\nSent\n\nArchive",
            "-annotate", "+300+180", "Re: the converted photos look great at quality 85\nThanks! I also tried the lossless option but the files were huge.\nCould we keep the metadata but drop the GPS? Let me know.",
        ],
        "transparent/generated-badge.png": [
            "-size", "800x800", "xc:none",
            "-fill", "#00000055", "-draw", "circle 410,420 410,90",
            "-blur", "0x18",
            "-fill", "#e53e3e", "-draw", "circle 400,400 400,80",
            "-fill", "white", "-pointsize", "150", "-gravity", "center",
            "-annotate", "+0+0", "NEW",
        ],
        "transparent/generated-logo.png": [
            "-size", "1200x500", "xc:none",
            "-fill", "#2b6cb0", "-draw", "roundrectangle 20,20 480,480 60,60",
            "-fill", "#ffffff", "-draw", "polygon 150,380 250,120 350,380",
            "-fill", "#1a202c", "-pointsize", "120", "-annotate", "+520+300", "Acme",
        ],
        "transparent/generated-glass.png": [
            "-size", "900x900", "radial-gradient:#ffffffee-#3182ce00",
            "-fill", "#3182ce88", "-draw", "roundrectangle 150,150 750,750 80,80",
        ],
    }
    for relative, args in jobs.items():
        out = CORPUS / relative
        if out.exists() and not force:
            continue
        out.parent.mkdir(parents=True, exist_ok=True)
        print(f"generated {relative}")
        magick(*args, "-depth", "8", "-strip", f"PNG32:{out}" if "transparent" in relative else f"PNG24:{out}")
        lock[relative] = {"source": "generated by scripts/bench_corpus.py", "licence": "CC0"}


def main() -> None:
    # Commons titles aren't all representable in the Windows console's code page.
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    sys.stderr.reconfigure(encoding="utf-8", errors="replace")
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--force", action="store_true", help="rebuild every image")
    args = parser.parse_args()

    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    lock_path = CORPUS / "sources.json"
    lock = json.loads(lock_path.read_text(encoding="utf-8")) if lock_path.exists() else {}
    CORPUS.mkdir(parents=True, exist_ok=True)

    generated(args.force, lock)
    kodak(manifest["kodak"], args.force, lock)
    commons(manifest["commons"], args.force, lock)

    lock_path.write_text(json.dumps(lock, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    counts = {}
    for png in CORPUS.glob("*/*.png"):
        counts[png.parent.name] = counts.get(png.parent.name, 0) + 1
    print("corpus:", ", ".join(f"{k} {v}" for k, v in sorted(counts.items())))


if __name__ == "__main__":
    main()
