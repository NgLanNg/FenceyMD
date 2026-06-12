// Standalone test harness for the snapshot pipeline.
//
// What this verifies (independent of the WebView + the JS bridge):
//   1. xcap::Window::all() succeeds on this platform
//   2. We can find an MD Reader window
//   3. capture_image() returns a non-empty RGBA buffer
//   4. We can encode it as PNG and write to disk
//   5. We can push it to the clipboard via arboard
//
// Run with: `cargo run --release --example snapshot_test`
//
// Output goes to <repo>/target/snapshot-test-output.png and exits 0
// on success, non-zero on failure. This is a one-shot diagnostic —
// not a long-lived test.

use std::path::PathBuf;

fn main() {
    println!("[snapshot-test] start");

    let windows = match xcap::Window::all() {
        Ok(w) => w,
        Err(e) => { eprintln!("FAIL: xcap::Window::all: {e}"); std::process::exit(1); }
    };
    println!("[snapshot-test] found {} windows", windows.len());

    let pid = std::process::id();
    let me = windows.into_iter().find(|w| {
        let minimized = w.is_minimized().unwrap_or(true);
        if minimized { return false; }
        let wpid = w.pid().unwrap_or(0);
        if wpid == pid { return true; }
        w.app_name()
            .map(|n| n.to_lowercase().contains("md reader"))
            .unwrap_or(false)
    });
    let me = match me {
        Some(w) => w,
        None => { eprintln!("FAIL: no MD Reader window found"); std::process::exit(2); }
    };
    println!("[snapshot-test] picked window: app_name={:?} title={:?}",
             me.app_name().ok(), me.title().ok());

    let img = match me.capture_image() {
        Ok(i) => i,
        Err(e) => { eprintln!("FAIL: capture_image: {e}"); std::process::exit(3); }
    };
    let (w, h) = (img.width(), img.height());
    println!("[snapshot-test] captured {}x{} ({} bytes RGBA)",
             w, h, w as usize * h as usize * 4);
    if w == 0 || h == 0 {
        eprintln!("FAIL: zero-dimension image");
        std::process::exit(4);
    }

    // Write the PNG to disk so we can inspect it.
    let out_path: PathBuf = [env!("CARGO_MANIFEST_DIR"), "..", "target", "snapshot-test-output.png"]
        .iter().collect();
    if let Err(e) = img.save(&out_path) {
        eprintln!("FAIL: save {}: {e}", out_path.display());
        std::process::exit(5);
    }
    println!("[snapshot-test] wrote {}", out_path.display());

    // Try the clipboard too.
    let img2 = match image::open(&out_path) {
        Ok(i) => i.to_rgba8(),
        Err(e) => { eprintln!("FAIL: re-decode: {e}"); std::process::exit(6); }
    };
    let (w2, h2) = (img2.width(), img2.height());
    let data = arboard::ImageData {
        width: w2 as usize,
        height: h2 as usize,
        bytes: std::borrow::Cow::Owned(img2.into_raw()),
    };
    let mut cb = match arboard::Clipboard::new() {
        Ok(c) => c,
        Err(e) => { eprintln!("FAIL: clipboard new: {e}"); std::process::exit(7); }
    };
    if let Err(e) = cb.set_image(data) {
        eprintln!("FAIL: clipboard set: {e}"); std::process::exit(8);
    }
    println!("[snapshot-test] clipboard set ok");
    println!("[snapshot-test] OK");
}
