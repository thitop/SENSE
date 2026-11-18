# Question pictures

`grip_*`, `pivot_*`, `pad_*` and `not_sure` are the pictures on the "Before you start" questions.

- Placeholders (current): hand-drawn for SENSE, flattened from the design mockups
  (`docs/superpowers/mockups/player-questions-v5.html` and `-v2.html`). Plain SVG with inline
  presentation attributes only: no `<style>`, `class`, `<use>`, `clipPath`, `href` or scripts.
- Final art (replaces these under the same file names): AI-generated from the prompts in the design
  spec, vectorised by `tools/vectorize_questions.py`. The raw images are in `originals/`.

No third-party artwork is used in the placeholders.

## Status per file

- Final AI art (vectorised): `grip_palm`, `grip_claw`, `grip_fingertip`, `pivot_wrist`,
  `pivot_fingers`, `pivot_shoulder`, `pad_small`, `pad_medium`, `pad_large`.
- Still placeholders: `pivot_arm` (original being regenerated), `not_sure`
  (no original).

## Regenerating from the originals

1. Generate the pictures with the prompts in the design spec; save them as `originals/<id>.webp` or `.png`.
2. Set up the tool once (a venv inside the repo, ignored by git):
   `python -m venv .venv-art`, then `.venv-art/Scripts/python.exe -m pip install -r tools/requirements-art.txt`.
   The vtracer Python wheel segfaults on Python 3.15, so also run `cargo install vtracer --root .venv-art`;
   the script prefers that CLI (`.venv-art/bin/vtracer`) and falls back to the wheel.
3. Run `.venv-art/Scripts/python.exe tools/vectorize_questions.py assets/questions/originals <out_dir>`.
   Background `#0B0C0E` becomes transparent, fills snap to the five-colour palette, and the tool first
   quantizes the original to the background plus the palette (nearest colour, no dithering), then traces with
   `filter_speckle=4`, `color_precision=6`, falling back to precision 5 and 4 until the file is at most 300 KB.
   It exits non-zero if a picture still does not fit.
4. Copy the SVGs you want from `<out_dir>` into `assets/questions/`, then run
   `cargo test --test player_questions_ui_tests`.

`tools/selftest_vectorize.py` checks the tool on a synthetic image.
