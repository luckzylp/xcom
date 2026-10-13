#!/usr/bin/env python3
"""Generate the application icon at the sizes a Linux icon theme expects.

Run from the repository root:

    python3 packaging/make-icons.py

Writes packaging/icons/xcom-<size>.png.  The drawing is parameterised by size
rather than scaled from one bitmap, so the strokes and the rounded corners stay
crisp at 48 px instead of turning to mush.
"""

from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

# The sizes hicolor themes actually look for, plus one for HiDPI.
SIZES = (48, 64, 128, 256, 512)

# Palette, matching the dark theme of the application.
BADGE = (30, 30, 30, 255)
ACCENT = (0, 255, 0, 255)
ACCENT_DIM = (0, 255, 0, 90)

FONT_CANDIDATES = (
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationMono-Bold.ttf",
)


def load_font(pixel_size: int) -> ImageFont.FreeTypeFont:
    for path in FONT_CANDIDATES:
        try:
            return ImageFont.truetype(path, pixel_size)
        except OSError:
            continue
    raise SystemExit("no monospace font found to draw the icon with")


def draw(size: int) -> Image.Image:
    """A rounded badge with the terminal prompt that the app's log uses."""
    scale = size / 256.0
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    painter = ImageDraw.Draw(img)

    def px(value: float) -> float:
        return value * scale

    painter.rounded_rectangle(
        [px(8), px(8), px(248), px(248)],
        radius=px(48),
        fill=BADGE,
        outline=ACCENT_DIM,
        width=max(1, round(px(3))),
    )

    font = load_font(max(6, round(px(130))))
    painter.text((px(30), px(44)), ">", font=font, fill=ACCENT)

    # Block cursor.
    painter.rectangle(
        [px(150), px(168), px(216), px(186)],
        fill=ACCENT,
    )
    return img


def main() -> None:
    out_dir = Path(__file__).resolve().parent / "icons"
    out_dir.mkdir(parents=True, exist_ok=True)
    for size in SIZES:
        path = out_dir / f"xcom-{size}.png"
        draw(size).save(path)
        print(f"wrote {path} ({path.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
