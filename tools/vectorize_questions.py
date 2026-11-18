"""Vectorise the AI-generated question pictures into palette-locked SVGs.

Usage: python tools/vectorize_questions.py <in_dir> <out_dir>

Reads <id>.png or <id>.webp for each id in ART_IDS (those that exist), makes the
#0B0C0E background transparent, quantizes the picture to PALETTE (plus the background)
before tracing so small highlights survive, traces with vtracer, snaps every fill to
PALETTE, rounds coordinates to 1 decimal, and fails (writing nothing) for any output
that exceeds 300 KB.
"""
import io
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image

ART_IDS = [
    "grip_palm", "grip_claw", "grip_fingertip",
    "pivot_wrist", "pivot_fingers", "pivot_arm", "pivot_shoulder",
    "pad_small", "pad_medium", "pad_large",
]
PALETTE = ["#1B1D21", "#2C2F35", "#A3A6AD", "#ECECEE", "#FFD23F"]
BACKGROUND = (0x0B, 0x0C, 0x0E)
BG_TOLERANCE = 12
MAX_BYTES = 300 * 1024
# (filter_speckle, color_precision) attempts: finest first, then coarser colour precision
# only if the output would exceed MAX_BYTES.
ATTEMPTS = [(4, 6), (4, 5), (4, 4)]


def hex_to_rgb(h):
    h = h.lstrip("#")
    return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))


PALETTE_RGB = [(p, hex_to_rgb(p)) for p in PALETTE]


def snap(hex_colour):
    r, g, b = hex_to_rgb(hex_colour)
    if r > 180 and g > 140 and b < 120:
        return "#FFD23F"
    return min(PALETTE_RGB, key=lambda p: sum((a - c) ** 2 for a, c in zip(p[1], (r, g, b))))[0]


def quantize(img):
    """Nearest-colour reduce to the background plus PALETTE, no dithering."""
    colours = [BACKGROUND] + [hex_to_rgb(p) for p in PALETTE]
    pal = Image.new("P", (1, 1))
    flat = [c for rgb in colours for c in rgb]
    pal.putpalette(flat + list(BACKGROUND) * ((768 - len(flat)) // 3))
    return img.convert("RGB").quantize(palette=pal, dither=Image.Dither.NONE).convert("RGB")


def make_transparent(img):
    img = quantize(img).convert("RGBA")
    px = img.load()
    w, h = img.size
    for y in range(h):
        for x in range(w):
            r, g, b, _ = px[x, y]
            if all(abs(c - t) <= BG_TOLERANCE for c, t in zip((r, g, b), BACKGROUND)):
                px[x, y] = (0, 0, 0, 0)
    return img


def round_numbers(svg):
    def r1(m):
        s = f"{round(float(m.group(0)), 1):.1f}"
        s = s[:-2] if s.endswith(".0") else s
        return "0" if s == "-0" else s
    # Round only decimals; integers are already minimal.
    return re.sub(r"-?\d+\.\d+", r1, svg)


def clean(svg):
    start = svg.find("<svg")
    if start < 0:
        raise ValueError("no <svg element in vtracer output")
    svg = svg[start:]  # drops <?xml ...?> declaration and comments
    svg = re.sub(r"<!--.*?-->", "", svg, flags=re.S)
    svg = re.sub(r'fill="(#[0-9a-fA-F]{6})"', lambda m: f'fill="{snap(m.group(1))}"', svg)
    svg = round_numbers(svg)
    svg = re.sub(r"(?<![\d.])-0(?![\d.])", "0", svg)
    if "viewBox" not in svg:  # scale with the card instead of a fixed pixel size
        m = re.match(r'<svg([^>]*?)\swidth="(\d+)"\s+height="(\d+)"', svg)
        if m:
            svg = svg.replace(f'width="{m.group(2)}" height="{m.group(3)}"',
                              f'width="{m.group(2)}" height="{m.group(3)}" viewBox="0 0 {m.group(2)} {m.group(3)}"', 1)
    svg = re.sub(r">\s+<", "><", svg).strip()
    return svg


def find_cli():
    """vtracer CLI: .venv-art/bin (cargo install --root .venv-art) or PATH."""
    root = Path(__file__).resolve().parent.parent / ".venv-art" / "bin"
    for name in ("vtracer.exe", "vtracer"):
        if (root / name).exists():
            return str(root / name)
    return shutil.which("vtracer")


def trace(png_bytes, speckle, precision):
    cli = find_cli()
    if cli:
        # The vtracer Python wheel segfaults on Python 3.15, so prefer the CLI.
        with tempfile.TemporaryDirectory() as tmp:
            src, dst = Path(tmp, "in.png"), Path(tmp, "out.svg")
            src.write_bytes(png_bytes)
            subprocess.run(
                [cli, "--input", str(src), "--output", str(dst), "--colormode", "color",
                 "--hierarchical", "stacked", "--filter_speckle", str(speckle),
                 "--color_precision", str(precision), "--path_precision", "1"],
                check=True,
            )
            return dst.read_text(encoding="utf-8")
    import vtracer
    return vtracer.convert_raw_image_to_svg(
        png_bytes, img_format="png", colormode="color", hierarchical="stacked",
        filter_speckle=speckle, color_precision=precision, path_precision=1,
    )


def vectorise(path):
    """Returns (svg, (speckle, precision)); stops at the first attempt within MAX_BYTES."""
    img = make_transparent(Image.open(path))
    buf = io.BytesIO()
    img.save(buf, format="PNG")
    for speckle, precision in ATTEMPTS:
        svg = clean(trace(buf.getvalue(), speckle, precision))
        if len(svg.encode("utf-8")) <= MAX_BYTES:
            break
    return svg, (speckle, precision)


def main(argv):
    if len(argv) != 3:
        print(__doc__)
        return 2
    in_dir, out_dir = Path(argv[1]), Path(argv[2])
    out_dir.mkdir(parents=True, exist_ok=True)
    failed = False
    for art_id in ART_IDS:
        src = next((in_dir / f"{art_id}{e}" for e in (".png", ".webp") if (in_dir / f"{art_id}{e}").exists()), None)
        if src is None:
            continue
        svg, (speckle, precision) = vectorise(src)
        size = len(svg.encode("utf-8"))
        too_large = size > MAX_BYTES
        if not too_large:
            (out_dir / f"{art_id}.svg").write_text(svg, encoding="utf-8")
        status = "TOO LARGE (not written)" if too_large else "OK"
        print(f"{art_id}: {size} bytes {status} (speckle={speckle}, color_precision={precision})")
        failed |= size > MAX_BYTES
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
