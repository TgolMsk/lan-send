#!/usr/bin/env python3
"""官网用的图标：从应用图标裁出圆角版与站点 favicon。

    python3 lansend_web/tools/icons.py

源图是 `apps/app/scripts/make-icons.py` 的产物：
- `source-macos.png` 是 1024 画布上居中的 824 圆角图形（四角透明），裁到图形本身就得到
  网页上用的圆角图标；白底深色底都不露方角。
- `icon.png` 是满铺方形，苹果自己会加圆角遮罩，所以 apple-touch-icon 用它。
需要 Pillow。
"""
from __future__ import annotations

from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[2]
ICONS = ROOT / "apps/app/src-tauri/icons"
OUT = ROOT / "lansend_web/assets/img"

ROUNDED = ICONS / "source-macos.png"  # 四角透明
SQUARE = ICONS / "icon.png"  # 满铺


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)

    rounded = Image.open(ROUNDED).convert("RGBA")
    box = rounded.getbbox()  # 去掉 Apple 模板留的透明边距
    if box is None:
        raise SystemExit(f"{ROUNDED} 没有不透明像素")
    art = rounded.crop(box)

    for size, name in ((512, "icon-512.png"), (256, "icon-256.png"), (128, "icon-128.png"), (64, "icon-64.png")):
        art.resize((size, size), Image.LANCZOS).save(OUT / name, optimize=True)
        print(f"  {name} ({size}px, 圆角)")

    square = Image.open(SQUARE).convert("RGB")
    square.resize((180, 180), Image.LANCZOS).save(OUT / "apple-touch-icon.png", optimize=True)
    print("  apple-touch-icon.png (180px, 满铺)")

    favicon = art.resize((64, 64), Image.LANCZOS)
    favicon.save(OUT / "favicon.png", optimize=True)
    # 多尺寸 .ico：老浏览器与 Windows 固定到任务栏时用得上。
    favicon.save(OUT / "favicon.ico", sizes=[(16, 16), (32, 32), (48, 48)])
    print("  favicon.png / favicon.ico")


if __name__ == "__main__":
    main()
