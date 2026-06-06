#!/usr/bin/env python3
"""Remove baked-in white canvas from litecast icon PNGs and rebuild .icns."""

from __future__ import annotations

import shutil
import subprocess
import sys
from collections import deque
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
SOURCE_PNGS = [ROOT / "bundle" / "icon.png", ROOT / "assets" / "litecast-logo.png"]
ICNS_PATH = ROOT / "bundle" / "litecast.icns"
ICONSET_DIR = ROOT / "bundle" / "litecast.iconset"

ICONSET_SIZES = [
    ("icon_16x16.png", 16),
    ("icon_16x16@2x.png", 32),
    ("icon_32x32.png", 32),
    ("icon_32x32@2x.png", 64),
    ("icon_128x128.png", 128),
    ("icon_128x128@2x.png", 256),
    ("icon_256x256.png", 256),
    ("icon_256x256@2x.png", 512),
    ("icon_512x512.png", 512),
    ("icon_512x512@2x.png", 1024),
]


def is_background(r: int, g: int, b: int, threshold: int = 245) -> bool:
    return r >= threshold and g >= threshold and b >= threshold


def remove_white_background(img: Image.Image, threshold: int = 245) -> Image.Image:
    """Flood-fill near-white pixels connected to the image edges."""
    rgb = img.convert("RGBA")
    w, h = rgb.size
    px = rgb.load()
    visited = [[False] * w for _ in range(h)]
    queue: deque[tuple[int, int]] = deque()

    def try_seed(x: int, y: int) -> None:
        if visited[y][x]:
            return
        r, g, b, _a = px[x, y]
        if is_background(r, g, b, threshold):
            visited[y][x] = True
            queue.append((x, y))

    for x in range(w):
        try_seed(x, 0)
        try_seed(x, h - 1)
    for y in range(h):
        try_seed(0, y)
        try_seed(w - 1, y)

    while queue:
        x, y = queue.popleft()
        r, g, b, _a = px[x, y]
        px[x, y] = (r, g, b, 0)
        for nx, ny in ((x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)):
            if 0 <= nx < w and 0 <= ny < h and not visited[ny][nx]:
                nr, ng, nb, _na = px[nx, ny]
                if is_background(nr, ng, nb, threshold):
                    visited[ny][nx] = True
                    queue.append((nx, ny))

    return rgb


def square_master(img: Image.Image) -> Image.Image:
    """Center on a square canvas sized to the longest edge."""
    w, h = img.size
    side = max(w, h)
    square = Image.new("RGBA", (side, side), (0, 0, 0, 0))
    square.paste(img, ((side - w) // 2, (side - h) // 2), img)
    return square


def build_iconset(master: Image.Image) -> None:
    if ICONSET_DIR.exists():
        shutil.rmtree(ICONSET_DIR)
    ICONSET_DIR.mkdir(parents=True)

    for name, size in ICONSET_SIZES:
        resized = master.resize((size, size), Image.Resampling.LANCZOS)
        resized.save(ICONSET_DIR / name, optimize=True)


def rebuild_icns() -> None:
    subprocess.run(
        ["iconutil", "-c", "icns", str(ICONSET_DIR), "-o", str(ICNS_PATH)],
        check=True,
    )


def main() -> int:
    master_path = ROOT / "bundle" / "icon.png"
    if not master_path.exists():
        print(f"error: missing {master_path}", file=sys.stderr)
        return 1

    print(f"Processing {master_path} …")
    transparent = remove_white_background(Image.open(master_path))
    transparent.save(master_path, optimize=True)
    print(f"  saved transparent PNG: {master_path} ({transparent.size}, RGBA)")

    for path in SOURCE_PNGS:
        if path == master_path:
            continue
        print(f"Processing {path} …")
        out = remove_white_background(Image.open(path))
        out.save(path, optimize=True)
        print(f"  saved transparent PNG: {path} ({out.size}, RGBA)")

    master = square_master(transparent)
    master_1024 = master.resize((1024, 1024), Image.Resampling.LANCZOS)
    print("Building iconset …")
    build_iconset(master_1024)
    print(f"Regenerating {ICNS_PATH} …")
    rebuild_icns()
    shutil.rmtree(ICONSET_DIR)
    print("Done.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
