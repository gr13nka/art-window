//! Embeds the Windows icon and application manifest into the executable.
//!
//! The manifest is what turns on Common Controls v6 — without it the favourites
//! window is drawn with Windows 95 buttons and has no `SetWindowSubclass` — and
//! per-monitor DPI awareness, without which the thumbnails are scaled up blurred on
//! every high-density screen.
//!
//! On every platform it also compiles in `catalogue/dist/artists/`: the index of
//! painters and the one picture each is shown by. `include_bytes!` needs a path
//! written out in the source, and the set of pictures changes with the catalogue,
//! so the list of them is generated here rather than kept by hand.

use std::fmt::Write;
use std::path::Path;

fn main() {
    artists();

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

/// Writes `$OUT_DIR/artists.rs` for `src/art/artists.rs` to include: the index's
/// text, and every picture it names with its bytes.
///
/// A picture the index names and the directory lacks stops the build. The other
/// way round — a stray file — is merely not compiled in.
fn artists() {
    let dir = Path::new(&std::env::var("CARGO_MANIFEST_DIR").expect("set by cargo"))
        .join("catalogue")
        .join("dist")
        .join("artists");
    println!("cargo:rerun-if-changed={}", dir.display());

    let index = dir.join("index.tsv");
    let text = std::fs::read_to_string(&index)
        .unwrap_or_else(|e| panic!("reading {}: {e} — run catalogue/build.py", index.display()));

    let mut out = format!("const INDEX: &str = include_str!({index:?});\n");
    out.push_str("const PICTURES: &[(&str, &[u8])] = &[\n");
    for line in text
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        let Some(file) = line.split('\t').nth(6) else {
            continue;
        };
        let picture = dir.join(file);
        assert!(
            picture.is_file(),
            "{} is named by {} and is not there — run catalogue/build.py",
            picture.display(),
            index.display()
        );
        let _ = writeln!(out, "    ({file:?}, include_bytes!({picture:?})),");
    }
    out.push_str("];\n");

    let target = Path::new(&std::env::var("OUT_DIR").expect("set by cargo")).join("artists.rs");
    std::fs::write(&target, out).unwrap_or_else(|e| panic!("writing {}: {e}", target.display()));
}
