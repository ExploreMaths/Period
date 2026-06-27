"""Generate Period icon assets (PNG, ICO and SVG) matching the site amber theme.

Run this script to regenerate:
  - assets/period.png
  - assets/period.ico
  - assets/period.svg
  - vscode-extension/period.png
  - vscode-extension/period.svg
  - vscode-extension/fileicons/period.svg
"""
from __future__ import annotations

import io
import struct
from pathlib import Path

from PIL import Image, ImageDraw

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
VSCODE = ROOT / "vscode-extension"
FILEICONS = VSCODE / "fileicons"

# Amber accent palette.
RING_COLOR = (217, 119, 6, 255)  # #d97706
DOT_COLOR = (180, 83, 9, 255)    # #b45309


def draw_period_icon(size: int) -> Image.Image:
    """Draw a crisp, pixel-aligned Period icon at the requested size."""
    image = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(image)

    center = size // 2
    # Scale from the 256x256 master artwork:
    #   outer ring bbox [28, 28, 228, 228] -> radius 100, stroke 32
    #   dot bbox [100, 100, 156, 156] -> radius 28
    ring_outer = max(3, round(100 * size / 256))
    ring_width = max(2, round(32 * size / 256))
    dot_radius = max(1, round(28 * size / 256))

    # Outer ring.
    draw.ellipse(
        [center - ring_outer, center - ring_outer,
         center + ring_outer, center + ring_outer],
        outline=RING_COLOR,
        width=ring_width,
    )
    # Central dot.
    draw.ellipse(
        [center - dot_radius, center - dot_radius,
         center + dot_radius, center + dot_radius],
        fill=DOT_COLOR,
    )
    return image


def write_ico(path: Path, images: list[tuple[int, int, bytes]]) -> None:
    """Write a multi-PNG Windows ICO file."""
    count = len(images)
    header = struct.pack("<HHH", 0, 1, count)
    offset = 6 + 16 * count
    entries = b""
    data = b""
    for width, height, png_bytes in images:
        w_entry = width if width < 256 else 0
        h_entry = height if height < 256 else 0
        entries += struct.pack(
            "<BBBBHHII",
            w_entry,
            h_entry,
            0,      # color count (0 for >256 colors)
            0,      # reserved
            1,      # color planes
            32,     # bits per pixel
            len(png_bytes),
            offset,
        )
        data += png_bytes
        offset += len(png_bytes)
    path.write_bytes(header + entries + data)


def png_bytes(image: Image.Image) -> bytes:
    buf = io.BytesIO()
    image.save(buf, format="PNG", optimize=True)
    return buf.getvalue()


def main() -> None:
    sizes = [16, 24, 32, 48, 64, 128, 256]
    rendered = {s: draw_period_icon(s) for s in sizes}

    # PNG for the VS Code: extension marketplace icon and other uses.
    master = rendered[256]
    master.save(HERE / "period.png", "PNG", optimize=True)
    master.save(VSCODE / "period.png", "PNG", optimize=True)

    # Multi-resolution ICO for Windows file associations / shortcuts and the
    # executable icon.
    ico_images = [(s, s, png_bytes(rendered[s])) for s in sizes]
    write_ico(HERE / "period.ico", ico_images)
    write_ico(VSCODE / "period.ico", ico_images)

    # SVG used in VS Code: for file/language icons.  Add crispEdges so small
    # explorer icons stay sharp rather than anti-aliased/blurry.
    svg = (
        '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" '
        'width="256" height="256" shape-rendering="crispEdges">\n'
        '  <circle cx="128" cy="128" r="88" fill="none" stroke="#d97706" stroke-width="32"/>\n'
        '  <circle cx="128" cy="128" r="28" fill="#b45309"/>\n'
        '</svg>\n'
    )
    (HERE / "period.svg").write_text(svg, encoding="utf-8")
    (VSCODE / "period.svg").write_text(svg, encoding="utf-8")
    (FILEICONS / "period.svg").write_text(svg, encoding="utf-8")

    print("Generated period.png, period.ico and period.svg")


if __name__ == "__main__":
    main()
