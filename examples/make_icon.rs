//! Writes `assets/icon/sense.ico`, the .exe icon, from the app's crosshair icon.
//! Run after changing `ui::app_icon`: `cargo run --example make_icon`.

use aim_trainer::ui::app_icon::{ico_bytes, ICO_SIZES};

fn main() -> std::io::Result<()> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets").join("icon");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("sense.ico");
    std::fs::write(&path, ico_bytes(&ICO_SIZES))?;
    println!("wrote {}", path.display());
    Ok(())
}
