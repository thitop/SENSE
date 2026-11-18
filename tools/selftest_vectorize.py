"""Self-test for tools/vectorize_questions.py. Run with the .venv-art interpreter."""
import re
import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image, ImageDraw

PALETTE = {"#1B1D21", "#2C2F35", "#A3A6AD", "#ECECEE", "#FFD23F"}
HERE = Path(__file__).resolve().parent


def main():
    with tempfile.TemporaryDirectory() as tmp:
        src, out = Path(tmp, "in"), Path(tmp, "out")
        src.mkdir()
        out.mkdir()
        img = Image.new("RGB", (512, 384), "#0B0C0E")
        d = ImageDraw.Draw(img)
        d.rectangle([40, 40, 260, 200], fill="#2A2C30")
        d.ellipse([280, 150, 460, 330], fill="#F5C842")
        img.save(src / "grip_palm.png")
        r = subprocess.run(
            [sys.executable, str(HERE / "vectorize_questions.py"), str(src), str(out)],
            capture_output=True, text=True,
        )
        assert r.returncode == 0, f"script failed: {r.stderr or r.stdout}"
        f = out / "grip_palm.svg"
        data = f.read_text()
        assert data.startswith("<svg"), "must start with <svg"
        assert f.stat().st_size <= 300 * 1024, "too large"
        fills = {m.upper() for m in re.findall(r'fill="(#[0-9a-fA-F]{6})"', data)}
        assert fills, "no fills"
        assert fills <= PALETTE, f"off-palette fills: {fills - PALETTE}"
        assert {"#FFD23F", "#2C2F35"} <= fills, f"missing colours: {fills}"
        assert "#0B0C0E" not in data.upper(), "background left"
    print("selftest OK")


if __name__ == "__main__":
    main()
