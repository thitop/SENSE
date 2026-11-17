# SENSE

An aim trainer that finds the mouse sensitivity your hand already prefers. You play short flick,
micro-adjustment and tracking tests at different sensitivities. SENSE watches whether you run ahead of
targets or fall behind them, narrows the range round by round, and gives you one number in cm/360°,
converted to the in-game value of every supported game.

Written in Rust, rendered with `wgpu` and `egui`. Windows 10/11.

---

## For players

### Getting it

You only need `SENSE.exe`. It is a single file: no installer, no extra downloads, no Visual C++
Redistributable. Put it anywhere and double-click it.

The first time you open it, Windows may show "Windows protected your PC", because the file isn't
digitally signed. Click **More info**, then **Run anyway**.

Requirements: Windows 10 or 11 (64-bit) and a graphics card with DirectX 12, Vulkan or OpenGL support.

### First run

A three-step setup asks for:

1. Your mouse DPI.
2. The game you play most.
3. Your current sensitivity in that game (converted to cm/360°).

You can run it again from **Settings > Run setup again**.

### Finding your sensitivity

Press **Start calibration** on the Home screen. One run takes about 7 minutes:

- A short warm-up at your own sensitivity (flicks, a few micro targets, tracking). It isn't scored.
- Then 7 rounds. Each round tests one sensitivity with 18 flicks at three distances, 8 small
  corrections and 12 seconds of tracking.

Inside each round the targets come in pairs, one slightly faster (+12%) and one slightly slower (-12%)
than the round's sensitivity. Doing better on the slower side means the round's sensitivity is too fast,
and the other way round. Habits you have at every sensitivity, like trailing a moving target, cancel out
this way. One fit over all tests picks the next sensitivity to try and, at the end, the result.

Press **Esc** to pause. The interrupted test starts again from the beginning when you resume.

### Reading the result

**My sensitivity** shows:

- Your sensitivity in cm/360° and a confidence level (High, Medium or Low).
- The value for your main game with a copy button, and the values for every other game.
- Each test's own estimate, and graphs of how the search narrowed down.

**What to practise** lists up to three aim habits that are still there at your best sensitivity, so
changing sensitivity won't fix them. The button on each card starts the matching practice scenario. The
card also shows your progress since calibrating, counting practice sessions played within 12% of the
result. Notes under the cards tell you when the run was still warming up, or when a test was left out
because of a habit.

**Use this sensitivity** makes it the sensitivity for practice.

**Run again** adds to your result instead of replacing it. Recent runs (last 14 days, same DPI, up to 5)
that agree with each other are combined, which makes the result tighter. If your latest run found
something quite different from the ones before, SENSE shows that run alone and tells you so. That
usually means your aim is still settling in: warm up first, or calibrate again another day.

### How many runs

Usually 2 to 3, at most one a day, with a 5 to 10 minute warm-up first (**Warm up first** on the Home
screen). Stop when the result is Medium or High, or when your last two runs land close together. You
don't need to chase High.

### Practice

Six scenarios, always at your saved sensitivity:

| Scenario | What you do |
|---|---|
| Flick | Snap to targets that appear anywhere. |
| Tracking | Keep your crosshair on a moving target. |
| Target switching | Clear the highlighted target, then the next. |
| Micro-adjust | Small corrections on head-sized targets. |
| Speed | Targets vanish faster the longer you last. |
| Moving targets | Hit the moving target, then the next one. |

Flick spreads its targets evenly over three distances. History shows each session's habits compared
with your previous five sessions of the same scenario.

Sensitivity only changes during calibration.

### Supported games

Valorant, Counter-Strike 2 / CS:GO, Overwatch 2, Apex Legends (and other Source engine games), Fortnite,
Rainbow Six Siege, Call of Duty / Warzone, WARDOGS and PUBG: BATTLEGROUNDS.

Where a game has scope or ADS settings, SENSE shows the 0% monitor distance values: aim near the
crosshair feels the same zoomed in as not zoomed. They are relative to your hipfire sensitivity, so they
suit any cm/360°. PUBG's values come from community-reported scaling rather than measurement, and the
screen says so. Fortnite, Rainbow Six Siege and WARDOGS have no verified scope values yet.

### Controls

| Key | Action |
|---|---|
| Left click | Shoot, or select in the menu |
| Esc | Open or close the menu. During calibration: pause or resume |
| 1 to 6 | Start a practice scenario (practice only) |
| R | Retry the current practice scenario |
| C | Open My sensitivity (practice only) |

### Where your data is kept

Everything stays on your computer, in `%APPDATA%\AimTrainer` (type that into the File Explorer
address bar):

| Folder or file | Contents |
|---|---|
| `profiles\` | Each profile: DPI, main game, sensitivity, latest result |
| `history\` | Every session you played, per profile |
| `calibrations\` | Every calibration run, with what each test said per round |
| `settings.json` | Sound settings (effects on or off, effects volume, music volume) |

Close SENSE before editing or deleting any of these files.

---

## For developers

### Requirements

- [Rust](https://rustup.rs/) 1.80 or newer, with the MSVC toolchain
- Windows 10 or 11, with the Windows SDK (for the resource compiler that embeds the .exe icon)

### Commands

Run in development (opens with a console window for logs):

```bash
cargo run
```

Run the tests:

```bash
cargo test
```

Render every screen and HUD state to PNG files in `target/ui-preview/`, without playing:

```bash
cargo run --example ui_preview
```

### Building SENSE.exe to share

```bash
cargo build --release
```

```bash
cp target/release/aim-trainer.exe dist/SENSE.exe
```

The release build is self-contained:

- The C runtime is linked statically (`.cargo/config.toml`), so other PCs don't need the Visual C++
  Redistributable.
- Sounds, fonts, game logos, game data and shaders are embedded in the binary.
- It opens without a console window. Debug builds keep the console.
- `build.rs` embeds the icon (`assets/icon/sense.ico`) and the name SENSE into the .exe.

### Changing the icon

The window icon and the .exe icon are both drawn from the Phosphor crosshair in `src/ui/app_icon.rs`.
After changing it, regenerate the .ico (a test fails while the file is out of date):

```bash
cargo run --example make_icon
```

### Project layout

| Path | Contents |
|---|---|
| `src/engine/` | Window, rendering (`wgpu`), raw mouse input, audio, timing |
| `src/game/` | Targets, hit detection, practice tasks, the calibration run |
| `src/sensitivity/` | Paired tests, the joint fit, pooling runs, game conversion |
| `src/data/` | Profiles, history, calibration archive, app settings |
| `src/ui/` | Screens, HUD, widgets, theme, icon |
| `data/games.json` | Per-game conversion constants and scope values |
| `assets/` | Sounds, fonts, game logos, icon |
| `docs/superpowers/` | Design specs and plans |

### Sound and music

Effects and music loops were generated with [ElevenLabs](https://elevenlabs.io). To regenerate them
(the prompts live in the script):

```bash
ELEVENLABS_API_KEY=... python tools/generate_audio.py --all
```

### Fonts

Chakra Petch and Inter are embedded from `assets/fonts/` under the SIL Open Font License (see
`assets/fonts/OFL.txt` and `assets/fonts/OFL-Inter.txt`).
