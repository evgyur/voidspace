from pathlib import Path

from PIL import Image, ImageDraw, ImageFont


ROOT = Path(__file__).resolve().parents[1]
ASSETS = ROOT / "crates" / "voidspace-app" / "assets"
FONT = ASSETS / "fonts" / "Unbounded[wght].ttf"
PNG = ASSETS / "voidspace.png"
ICO = ASSETS / "voidspace.ico"

CANVAS = 1024
ORANGE = (255, 90, 47, 255)
WHITE = (242, 243, 245, 255)
INK = (7, 8, 9, 255)
BORDER = (48, 52, 55, 255)


def fit_monogram(draw: ImageDraw.ImageDraw) -> tuple[ImageFont.FreeTypeFont, tuple[int, int, int, int]]:
    for size in range(510, 240, -2):
        font = ImageFont.truetype(FONT, size)
        font.set_variation_by_name("Black")
        bounds = draw.textbbox((0, 0), "VS", font=font, stroke_width=0)
        if bounds[2] - bounds[0] <= 720 and bounds[3] - bounds[1] <= 500:
            return font, bounds
    raise RuntimeError("Could not fit the Voidspace monogram")


def render() -> Image.Image:
    image = Image.new("RGBA", (CANVAS, CANVAS), (0, 0, 0, 0))
    draw = ImageDraw.Draw(image)

    # A restrained dark tile keeps the mark legible on both Windows themes.
    draw.rounded_rectangle((48, 48, 976, 976), radius=214, fill=INK, outline=BORDER, width=14)

    font, _ = fit_monogram(draw)
    v_box = draw.textbbox((0, 0), "V", font=font)
    s_box = draw.textbbox((0, 0), "S", font=font)
    v_width = v_box[2] - v_box[0]
    s_width = s_box[2] - s_box[0]
    overlap = 38
    total_width = v_width + s_width - overlap
    x = (CANVAS - total_width) // 2

    # The shared baseline and tight overlap turn the approved VOID/SPACE split
    # into a compact mark that stays readable at 16 px.
    probe = draw.textbbox((0, 0), "VS", font=font)
    y = (CANVAS - (probe[3] - probe[1])) // 2 - probe[1] - 10
    draw.text((x - v_box[0], y), "V", font=font, fill=ORANGE)
    draw.text((x + v_width - overlap - s_box[0], y), "S", font=font, fill=WHITE)

    # A single orange datum line echoes the application's technical UI chrome.
    draw.rounded_rectangle((216, 798, 808, 826), radius=14, fill=ORANGE)
    return image


def main() -> None:
    image = render()
    image.save(PNG, format="PNG", optimize=True)
    image.save(
        ICO,
        format="ICO",
        sizes=[(16, 16), (20, 20), (24, 24), (32, 32), (40, 40), (48, 48), (64, 64), (128, 128), (256, 256)],
    )
    print(f"VOIDSPACE_ICON_OK {PNG} {ICO}")


if __name__ == "__main__":
    main()
