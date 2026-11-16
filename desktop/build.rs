// Tauri reads tauri.conf.json and the icon at build time, for the window only: a build without
// the `window` feature (the library and its tests) needs none of it.
fn main() {
    #[cfg(feature = "window")]
    tauri_build::build();
}
