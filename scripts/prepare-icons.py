"""Maintain the embedded Material Symbols subset (not needed to build MiCoPi).

Requires fonttools. Downloads only from Google's official icon repository;
source files stay under ignored target/tools, output and license under assets.
"""
from pathlib import Path
import argparse
import hashlib
import json
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
CACHE = ROOT / "target/tools/material-symbols"
OUTPUT = ROOT / "assets/material-symbols"
NAMES = """palette brush ink_eraser colorize back_hand zoom_in zoom_out layers
search keyboard keyboard_return keyboard_tab shift space_bar backspace
settings tune edit undo redo add remove fit_screen crop_free file_open save save_as
image arrow_upward arrow_downward arrow_back arrow_forward download dock_to_left
dock_to_right close check mouse note_add drag_indicator""".split()


def fetch(url, path, allow_download):
    if not path.exists():
        if not allow_download:
            raise RuntimeError(f"Missing {path.name}; pass --download to fetch upstream assets")
        request = urllib.request.Request(url, headers={"User-Agent": "MiCoPi-icon-subset"})
        with urllib.request.urlopen(request, timeout=90) as response:
            path.write_bytes(response.read())
    return path.read_bytes()


def main():
    parser = argparse.ArgumentParser(__doc__)
    parser.add_argument("--download", action="store_true")
    parser.add_argument("--list", action="store_true")
    args = parser.parse_args()
    CACHE.mkdir(parents=True, exist_ok=True)
    manifest_path = OUTPUT / "manifest.json"
    pinned = json.loads(manifest_path.read_text(encoding="utf-8")) if manifest_path.exists() else None
    if pinned:
        revision = pinned["revision"]
    else:
        head = fetch("https://api.github.com/repos/google/material-design-icons/commits/master",
                     CACHE / "upstream.json", args.download)
        revision = json.loads(head)["sha"]
    base = f"https://raw.githubusercontent.com/google/material-design-icons/{revision}/"
    font_data = fetch(base + "variablefont/MaterialSymbolsRounded%5BFILL,GRAD,opsz,wght%5D.ttf",
                      CACHE / "symbols.ttf", args.download)
    if pinned and hashlib.sha256(font_data).hexdigest() != pinned["source_sha256"]:
        raise RuntimeError("Cached source font differs from pinned manifest; clear the material-symbols cache and download again")
    points = fetch(base + "variablefont/MaterialSymbolsRounded%5BFILL,GRAD,opsz,wght%5D.codepoints",
                   CACHE / "codepoints.txt", args.download)
    license_data = fetch(base + "LICENSE", CACHE / "LICENSE", args.download)
    codepoints = dict(line.split() for line in points.decode().splitlines())
    if args.list:
        for name in NAMES:
            print(name, codepoints.get(name, "MISSING"))
        return
    missing = [name for name in NAMES if name not in codepoints]
    if missing:
        raise RuntimeError(f"Unknown icon names: {missing}")

    from fontTools.ttLib import TTFont
    from fontTools import subset
    from fontTools.varLib.instancer import instantiateVariableFont
    font = TTFont(CACHE / "symbols.ttf", recalcTimestamp=False)
    options = subset.Options()
    options.layout_features = []  # Codepoints only; no ligature engine required.
    options.recalc_timestamp = False
    worker = subset.Subsetter(options=options)
    worker.populate(unicodes=[int(codepoints[name], 16) for name in NAMES])
    worker.subset(font)
    instantiateVariableFont(font, {"FILL": 0, "GRAD": 0, "opsz": 24, "wght": 400}, inplace=True)
    for name in font["name"].names:
        replacement = {1: "MiCoPi Symbols", 2: "Regular", 3: "MiCoPiSymbols-Regular",
                       4: "MiCoPi Symbols Regular", 6: "MiCoPiSymbols-Regular",
                       16: "MiCoPi Symbols", 17: "Regular"}.get(name.nameID)
        if replacement:
            name.string = replacement.encode(name.getEncoding())
    OUTPUT.mkdir(parents=True, exist_ok=True)
    target = OUTPUT / "MiCoPiSymbols.ttf"
    font.save(target)
    (OUTPUT / "LICENSE").write_bytes(license_data)
    manifest = {"upstream": "https://github.com/google/material-design-icons", "revision": revision,
                "source_sha256": hashlib.sha256(font_data).hexdigest(),
                "subset_sha256": hashlib.sha256(target.read_bytes()).hexdigest(),
                "axes": {"FILL": 0, "GRAD": 0, "opsz": 24, "wght": 400},
                "icons": {name: codepoints[name] for name in NAMES}}
    (OUTPUT / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(f"Prepared {len(NAMES)} icons, {target.stat().st_size} bytes; revision {revision}")


if __name__ == "__main__":
    main()
