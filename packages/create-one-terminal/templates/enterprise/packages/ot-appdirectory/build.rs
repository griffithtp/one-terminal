//! `rust-embed`'s `#[folder = "../../apps/app-directory/ui/dist"]` (src/ui.rs)
//! requires that directory to exist at compile time. On a machine that has
//! never run `npm run build:app-directory` (a fresh clone, or a CI job that
//! only runs `cargo check`), the folder won't exist yet. Create a minimal
//! placeholder so `cargo check`/`test`/`clippy` never hard-fail on missing
//! frontend output — matches the "stub frontend dist dirs" pattern already
//! used for the Tauri apps in .github/workflows/ci.yml.

use std::path::Path;

fn main() {
    let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/app-directory/ui/dist");
    if !dist.exists() {
        std::fs::create_dir_all(&dist).expect("create placeholder app-directory ui/dist");
        std::fs::write(
            dist.join("index.html"),
            "<!doctype html><html><body>App Directory UI not built — run `npm run build:app-directory`.</body></html>",
        )
        .expect("write placeholder index.html");
    }
    println!("cargo:rerun-if-changed={}", dist.display());
}
