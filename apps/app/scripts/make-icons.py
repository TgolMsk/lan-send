#!/usr/bin/env python3
"""Turns the brand artwork into the icon sources Tauri needs.

    python3 apps/app/scripts/make-icons.py <artwork>   (defaults to docs/brand/logo.jpeg)
    cargo tauri icon apps/app/src-tauri/icons/source.png                     # iOS / Windows / Linux
    cargo tauri icon apps/app/src-tauri/icons/source-macos.png -o /tmp/mac   # then copy icon.icns

The artwork is a rounded square on a white background. It is cropped to
that square, the corners are filled with the border colour (full-bleed
`source.png`, what iOS and Windows want), and a transparent-corner version
is placed on Apple's macOS template (`source-macos.png`: 824 px artwork on
a 1024 canvas). `src/assets/logo.png` (sidebar brand mark) uses the
transparent version at 256 px. Needs Pillow.
"""
from __future__ import annotations

import shutil
import sys
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parents[3]
BRAND = ROOT / "docs/brand/logo.jpeg"
ICONS = ROOT / "apps/app/src-tauri/icons"
SIDEBAR = ROOT / "apps/app/src/assets/logo.png"
SIZE = 1024
MAC_ART = 824  # Apple's template: 824 px artwork centred on 1024


INSET = 4  # px trimmed off the artwork edge so the JPEG's soft rim is dropped
DARK = 150  # luminance below which a pixel counts as the navy border


def luminance(pixel: tuple[int, int, int]) -> float:
    r, g, b = pixel
    return 0.299 * r + 0.587 * g + 0.114 * b


def crop_to_artwork(image: Image.Image) -> Image.Image:
    """Crops to the dark rounded square (the light halo around it is ignored)."""
    rgb = image.convert("RGB")
    dark = rgb.convert("L").point(lambda v: 255 if v < DARK else 0)
    bbox = dark.getbbox()
    if bbox is None:
        raise SystemExit("no dark artwork found")
    left, top, right, bottom = bbox
    # Generated artwork is rarely a perfect square; an icon must be, so the
    # crop is stretched to one (a few percent is invisible on a rounded square).
    box = (left + INSET, top + INSET, right - INSET, bottom - INSET)
    width, height = box[2] - box[0], box[3] - box[1]
    if abs(width - height) > 0.15 * max(width, height):
        raise SystemExit(f"artwork is {width}x{height}: too far from square to stretch")
    return rgb.crop(box).resize((SIZE, SIZE), Image.LANCZOS)


def corner_radius(square: Image.Image) -> int:
    """Distance from the left edge to the first dark pixel on a top row."""
    y = 2
    for x in range(square.width):
        if luminance(square.getpixel((x, y))) < DARK:
            return x
    return 0


def rounded_mask(size: int, radius: int, scale: int = 4) -> Image.Image:
    big = Image.new("L", (size * scale, size * scale), 0)
    ImageDraw.Draw(big).rounded_rectangle((0, 0, size * scale - 1, size * scale - 1), radius * scale, fill=255)
    return big.resize((size, size), Image.LANCZOS)


def eroded(mask: Image.Image, px: int) -> Image.Image:
    return mask.filter(ImageFilter.MinFilter(px * 2 + 1))


def border_colours(square: Image.Image) -> list[tuple[int, int, int]]:
    """The border colour per row: the top/bottom band at the centre column,
    the left band for the rows in between (the border is a vertical gradient)."""
    width, height = square.size
    colours: list[tuple[int, int, int]] = []
    last = square.getpixel((width // 2, 8))
    for y in range(height):
        for candidate in (square.getpixel((width // 2, y)), square.getpixel((40, y))):
            if luminance(candidate) < DARK:
                last = candidate
                break
        colours.append(last)
    return colours


def clean_rim(square: Image.Image, mask: Image.Image) -> Image.Image:
    """Replaces the light halo just inside the edge with the border colour."""
    out = square.copy()
    px = out.load()
    ring = eroded(mask, 12).load()
    colours = border_colours(square)
    width, height = out.size
    for y in range(height):
        for x in range(width):
            if ring[x, y] < 255 and luminance(px[x, y]) > 185:
                px[x, y] = colours[y]
    return out


def full_bleed(square: Image.Image, mask: Image.Image) -> Image.Image:
    """Fills the corners outside the rounded square with the row's border colour."""
    out = clean_rim(square, mask)
    px = out.load()
    mpx = mask.load()
    colours = border_colours(square)
    width, height = out.size
    for y in range(height):
        fill = colours[y]
        for x in range(width):
            a = mpx[x, y]
            if a < 255:
                r, g, b = px[x, y]
                t = a / 255
                px[x, y] = (round(fill[0] * (1 - t) + r * t), round(fill[1] * (1 - t) + g * t), round(fill[2] * (1 - t) + b * t))
    return out


def transparent(square: Image.Image, mask: Image.Image) -> Image.Image:
    out = clean_rim(square, mask).convert("RGBA")
    out.putalpha(mask)
    return out


def macos_template(art: Image.Image) -> Image.Image:
    canvas = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    scaled = art.resize((MAC_ART, MAC_ART), Image.LANCZOS)
    offset = (SIZE - MAC_ART) // 2
    # Soft drop shadow like Apple's template.
    shadow = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
    shadow_mask = Image.new("L", (SIZE, SIZE), 0)
    shadow_mask.paste(scaled.getchannel("A"), (offset, offset + 10))
    shadow.putalpha(shadow_mask.filter(ImageFilter.GaussianBlur(12)).point(lambda v: v * 0.30))
    canvas.alpha_composite(shadow)
    canvas.alpha_composite(scaled, (offset, offset))
    return canvas


def main() -> None:
    source = Path(sys.argv[1]).expanduser() if len(sys.argv) > 1 else BRAND
    if source.resolve() != BRAND.resolve():
        BRAND.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, BRAND)
    square = crop_to_artwork(Image.open(source))
    radius = corner_radius(square)
    mask = eroded(rounded_mask(SIZE, radius), 2)
    ICONS.mkdir(parents=True, exist_ok=True)
    full_bleed(square, mask).save(ICONS / "source.png", optimize=True)
    art = transparent(square, mask)
    macos_template(art).save(ICONS / "source-macos.png", optimize=True)
    art.resize((256, 256), Image.LANCZOS).save(SIDEBAR, optimize=True)
    print(f"artwork radius {radius}px; wrote {ICONS / 'source.png'}, {ICONS / 'source-macos.png'}, {SIDEBAR}")


if __name__ == "__main__":
    main()
