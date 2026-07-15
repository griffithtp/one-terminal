use std::path::Path;

/// Tauri's `externalBin` (see tauri.conf.json) requires the sidecar binary to
/// exist at `binaries/one-terminal-<target-triple>` even for a plain
/// `cargo check`/`cargo build` — `tauri_build::build()` validates the
/// resource path and fails the build script if it's missing. On a fresh
/// clone (or CI, or a freshly scaffolded workspace), nothing has run
/// `scripts/build-terminal-sidecar.ts` yet, so create a placeholder if
/// absent. A real `tauri build` / `npm run build:desktop-agent` overwrites
/// this with the actual compiled `one-terminal` binary before packaging
/// (see apps/desktop-agent/package.json's `build` script) — the placeholder
/// only needs to satisfy this existence check for `cargo check`/`test`/dev.
fn placeholder_sidecar_path() -> Option<std::path::PathBuf> {
    let triple = std::env::var("TARGET").ok()?;
    let suffix = if triple.contains("windows") {
        ".exe"
    } else {
        ""
    };
    Some(Path::new("binaries").join(format!("one-terminal-{triple}{suffix}")))
}

fn main() {
    if let Some(path) = placeholder_sidecar_path() {
        if !path.exists() {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).expect("create binaries/ dir for sidecar placeholder");
            }
            std::fs::write(
                &path,
                b"placeholder - run `npm run build:desktop-agent` for the real binary",
            )
            .expect("write sidecar placeholder");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = std::fs::metadata(&path).unwrap().permissions();
                perms.set_mode(0o755);
                std::fs::set_permissions(&path, perms).expect("chmod sidecar placeholder");
            }
        }
    }

    tauri_build::build()
}
