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

    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows_attributes()))
        .expect("tauri-build failed");
}

// ── Windows application manifest ─────────────────────────────────────────────

/// The Common-Controls v6 manifest `tauri-build` embeds by default
/// (tauri-build/src/windows-app-manifest.xml).
const WINDOWS_APP_MANIFEST: &str = r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity
        type="win32"
        name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0"
        processorArchitecture="*"
        publicKeyToken="6595b64144ccf1df"
        language="*"
      />
    </dependentAssembly>
  </dependency>
</assembly>
"#;

/// On Windows MSVC, embeds the app manifest with linker arguments and tells `tauri-build`
/// not to add its own. `tauri-build` links the manifest into binaries only, so `cargo test`
/// executables load comctl32 v5 and exit before any test runs with
/// `STATUS_ENTRYPOINT_NOT_FOUND` (0xc0000139). `cargo:rustc-link-arg` applies to every
/// target, tests included. Other targets keep `tauri-build`'s default manifest.
fn windows_attributes() -> tauri_build::WindowsAttributes {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_os != "windows" || target_env != "msvc" {
        return tauri_build::WindowsAttributes::new();
    }
    let out_dir = std::env::var("OUT_DIR").expect("cargo sets OUT_DIR for build scripts");
    let path = std::path::Path::new(&out_dir).join("windows-app-manifest.xml");
    std::fs::write(&path, WINDOWS_APP_MANIFEST).expect("write windows-app-manifest.xml");
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", path.display());
    tauri_build::WindowsAttributes::new_without_app_manifest()
}
