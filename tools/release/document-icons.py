#!/usr/bin/env python3
"""Draws the Finder document icons for .carve jobs and .carveclip clipart.

A white page with a folded corner, the app icon in its middle and the kind
of file written under it. Needs Pillow (pip install pillow) and macOS
iconutil. Run from anywhere; it writes app/src-tauri/icons/*-document.icns.
"""

import pathlib
import shutil
import subprocess
import tempfile

from PIL import Image, ImageDraw, ImageFont

ROOT = pathlib.Path(__file__).resolve().parents[2]
ICONS = ROOT / "app" / "src-tauri" / "icons"
KINDS = {"carve-document": "CARVE", "carveclip-document": "CLIPART"}
SIDE = 1024


def page(label: str) -> Image.Image:
    """The icon at 1024 px: the page fills the middle 80 percent in width."""
    image = Image.new("RGBA", (SIDE, SIDE), (0, 0, 0, 0))
    draw = ImageDraw.Draw(image)
    left, top, right, bottom = 184, 64, 840, 960
    fold = 180
    outline = [
        (left, top),
        (right - fold, top),
        (right, top + fold),
        (right, bottom),
        (left, bottom),
    ]
    draw.polygon(outline, fill=(250, 250, 252, 255), outline=(170, 174, 182, 255), width=6)
    draw.polygon(
        [(right - fold, top), (right - fold, top + fold), (right, top + fold)],
        fill=(222, 226, 232, 255),
        outline=(170, 174, 182, 255),
        width=6,
    )
    app = Image.open(ICONS / "icon.png").convert("RGBA").resize((400, 400), Image.LANCZOS)
    image.alpha_composite(app, ((SIDE - 400) // 2, 300))
    font = ImageFont.load_default(size=96)
    width = draw.textlength(label, font=font)
    draw.text(((SIDE - width) / 2, 760), label, font=font, fill=(60, 66, 76, 255))
    return image


def icns(name: str, label: str) -> None:
    big = page(label)
    with tempfile.TemporaryDirectory() as tmp:
        folder = pathlib.Path(tmp) / f"{name}.iconset"
        folder.mkdir()
        for size in (16, 32, 128, 256, 512):
            big.resize((size, size), Image.LANCZOS).save(folder / f"icon_{size}x{size}.png")
            big.resize((size * 2, size * 2), Image.LANCZOS).save(
                folder / f"icon_{size}x{size}@2x.png"
            )
        out = ICONS / f"{name}.icns"
        subprocess.run(["iconutil", "-c", "icns", str(folder), "-o", str(out)], check=True)
    print(out.relative_to(ROOT))


if __name__ == "__main__":
    if shutil.which("iconutil") is None:
        raise SystemExit("iconutil not found: run this on macOS")
    for name, label in KINDS.items():
        icns(name, label)
