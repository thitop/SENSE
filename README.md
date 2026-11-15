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

## 🎮 Controls & Hotkeys

| Key | Action |
|---|---|
| **Left Click** | Shoot / Select UI elements |
| **ESC** | Toggle Full Menu Overlay / Unlock Mouse Cursor |
| **1** | Quick launch **Flick Shot** |
| **2** | Quick launch **Smooth Tracking** |
| **3** | Quick launch **Target Switching** |
| **4** | Quick launch **Micro-Adjustment** |
| **5** | Quick launch **Speed vs Precision** |
| **R** | Restart current scenario (with 3-second countdown) |
| **C** | Open Sensitivity Recommendation & Game Conversions table |

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

### Build Optimized Release Binary
```bash
cargo build --release
```
The standalone executable will be located at `target/release/aim-trainer.exe`.
