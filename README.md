# 🎯 Aim Trainer — Find Your Perfect Sensitivity

A high-performance, ultra-low latency Aim Trainer built in **Rust** designed to empirically discover your biologically optimal mouse sensitivity and DPI through gameplay analysis.

---

## ⚡ Core Features

- **Ultra-Low Latency & High FPS**: Built on `wgpu` (DirectX 12 / Vulkan) with `AutoNoVsync` unthrottled presentation and 1000 Hz physics simulation.
- **Windows Raw Input**: Direct `WM_INPUT` mouse device capture bypassing all Windows pointer precision and mouse acceleration.
- **Dual Sensitivity Calibration Engine**:
  - **PSA Method (Perfect Sensitivity Approximation)**: Automatic binary search narrowing your cm/360° range based on trajectory overshoots vs. undershoots.
  - **Statistical Quadratic Analysis**: Polynomial regression curve fitting on composite scores ($\text{Accuracy} \times \text{Speed} \times \text{Path Efficiency}$) to calculate your performance vertex.
- **5 Aim Scenarios (Clean Minimalist Aesthetics)**:
  1. **Flick Shot**: Rapid reflexes to randomly spawned static targets.
  2. **Smooth Tracking**: Continuous crosshair tracking on a dynamically oscillating target.
  3. **Target Switching**: Target prioritization and snapping across multiple concurrent spheres.
  4. **Micro-Adjustment**: Sub-angle precision practice on head-sized targets close to crosshair.
  5. **Speed vs Precision**: Rapidly expiring targets testing speed-accuracy breakdown thresholds.
- **Game Sensitivity Converter**: Converts your optimal universal $cm/360^\circ$ sensitivity to:
  - Valorant
  - Counter-Strike 2 / CS:GO
  - Overwatch 2
  - Apex Legends
  - Fortnite
  - Rainbow Six Siege
  - Call of Duty / Warzone
- **Real-Time Visualizations (`egui_plot`)**:
  - Sensitivity Performance Curve
  - PSA Convergence Flow
  - Reaction Time Distribution
- **Procedural Audio**: Sub-millisecond synthesized audio tones (Hit Ding, Miss Thud, Countdown Beeps, Complete Fanfare) without external audio file loading latency.
- **Profile & History System**: Multi-profile management with auto-saved session metrics stored at `%APPDATA%/AimTrainer/`.

---

## 🧭 How It Works

### First run
A three-step setup asks for your mouse DPI, the game you play most, and your current in-game
sensitivity (converted to cm/360°). Profiles saved by earlier versions skip it; **Settings → Run setup again**
brings it back.

### Calibration
**Home → Start calibration** runs about 70 flick shots over up to 7 rounds. Each round tests one
sensitivity and moves the range based on whether you overshoot or undershoot. The HUD shows the round,
the sensitivity being tested, and the current range, and a toast appears whenever the sensitivity changes.
When it finishes, **My sensitivity** shows the result, your main game's value with a copy button, the other
games, and graphs of how the search converged. **Use this sensitivity** makes it your practice sensitivity.

Sensitivity only changes during calibration. Practice scenarios always run at your saved sensitivity.

---

## 🎮 Controls & Hotkeys

| Key | Action |
|---|---|
| **Left Click** | Shoot / Select UI elements |
| **ESC** | Open or close the menu. During calibration: pause or resume |
| **1** | Practice **Flick** (practice only) |
| **2** | Practice **Tracking** (practice only) |
| **3** | Practice **Target switching** (practice only) |
| **4** | Practice **Micro-adjust** (practice only) |
| **5** | Practice **Speed** (practice only) |
| **R** | Retry the current practice scenario |
| **C** | Open **My sensitivity** (practice only) |

---

## 🚀 Building & Running

### Prerequisites
- [Rust & Cargo](https://rustup.rs/) (1.80+ recommended)
- Windows 10 / 11

### Run Development
```bash
cargo run
```

### Run Automated Tests
```bash
cargo test
```

### Preview the UI without playing
```bash
cargo run --example ui_preview
```
Renders every screen and HUD state to PNG files in `target/ui-preview/`.

### Fonts
Chakra Petch and Inter are embedded from `assets/fonts/` under the SIL Open Font License (see `OFL.txt` and `OFL-Inter.txt`).

### Build Optimized Release Binary
```bash
cargo build --release
```
The standalone executable will be located at `target/release/aim-trainer.exe`.
