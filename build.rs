//! Embeds the app icon (`assets/icon/sense.ico`) and name into the Windows .exe, so Explorer
//! and shortcuts show the crosshair. Regenerate the icon with `cargo run --example make_icon`.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon/sense.ico");
    println!("cargo:rerun-if-changed=build.rs");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon/sense.ico");
        res.set("ProductName", "SENSE");
        res.set("FileDescription", "SENSE");
        // A missing resource compiler shouldn't stop the build; the exe just has no icon.
        if let Err(e) = res.compile() {
            println!("cargo:warning=Could not embed the exe icon: {e}");
        }
    }
}
