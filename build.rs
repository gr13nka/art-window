//! Embeds the Windows icon and application manifest into the executable.
//!
//! The manifest is what turns on Common Controls v6 — without it the favourites
//! window is drawn with Windows 95 buttons and has no `SetWindowSubclass` — and
//! per-monitor DPI awareness, without which the thumbnails are scaled up blurred on
//! every high-density screen. Nothing to do anywhere else.

fn main() {
    println!("cargo:rerun-if-changed=windows/art-window.ico");
    println!("cargo:rerun-if-changed=windows/art-window.manifest");

    // `winresource` is a build dependency of Windows *hosts* only, so a cross-check
    // from macOS compiles this to nothing and simply goes without the resources.
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut resources = winresource::WindowsResource::new();
        resources
            .set_icon("windows/art-window.ico")
            .set_manifest_file("windows/art-window.manifest");
        resources
            .compile()
            .expect("embedding the Windows icon and manifest");
    }
}
