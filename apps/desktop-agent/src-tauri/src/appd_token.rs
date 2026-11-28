//! The embedded App Directory's write token.
//!
//! When the Desktop Agent serves the App Directory itself, it makes a random
//! token per run that `POST` / `PUT` / `DELETE /v2/apps` require
//! (`ot_appdirectory::WriteAuth`). Two clients need it:
//!
//! - the **management UI**, which the agent opens in the browser with the
//!   token in the URL *fragment* (`#ot-appd-token=…`). Browsers never send a
//!   fragment to the server, so fetching the UI page doesn't reveal it; the
//!   page reads it, keeps it in `sessionStorage` and removes it from the
//!   address bar;
//! - the scaffolder's `new-widget` wizard, which reads it from the
//!   [`token_path`] file in the user's `one-terminal` config folder (owner
//!   read/write only on macOS and Linux; Windows' per-user `%APPDATA%` ACL
//!   already excludes other users).
//!
//! The file is replaced on every start and removed on exit.

use std::io::Write;
use std::path::{Path, PathBuf};

/// File name of the token next to the user's `agent.config.json`.
pub const TOKEN_FILE: &str = "appd-token";

/// URL fragment key the management UI reads the token from.
pub const FRAGMENT_KEY: &str = "ot-appd-token";

/// The current run's token, managed as Tauri state when the App Directory is
/// embedded.
pub struct AppdToken(pub String);

/// 256 random bits from the OS, hex-encoded.
pub fn generate() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Where the token file lives: next to the per-user `agent.config.json`.
pub fn token_path() -> Option<PathBuf> {
    crate::config::user_config_path().map(|p| p.with_file_name(TOKEN_FILE))
}

/// Write `token` to `path`, replacing any previous file atomically, readable
/// and writable by the owner only (Unix).
pub fn write_file(path: &Path, token: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("tmp");
    let _ = std::fs::remove_file(&tmp);
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(&tmp)?;
    f.write_all(token.as_bytes())?;
    f.sync_all()?;
    drop(f);
    std::fs::rename(&tmp, path)
}

/// `base` with the token in its fragment, replacing any existing fragment.
pub fn ui_url(base: &str, token: &str) -> String {
    let base = base.split('#').next().unwrap_or(base);
    format!("{base}#{FRAGMENT_KEY}={token}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_long_hex_and_differ() {
        let a = generate();
        let b = generate();
        assert_eq!(a.len(), 64);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }

    #[test]
    fn ui_url_puts_the_token_in_the_fragment() {
        assert_eq!(
            ui_url("http://localhost:3005", "abc"),
            "http://localhost:3005#ot-appd-token=abc"
        );
        assert_eq!(
            ui_url("http://localhost:3005/#old", "abc"),
            "http://localhost:3005/#ot-appd-token=abc"
        );
    }

    #[test]
    fn write_file_replaces_and_restricts() {
        let dir = std::env::temp_dir().join(format!("da-appd-token-{}", std::process::id()));
        let path = dir.join(TOKEN_FILE);
        write_file(&path, "first").unwrap();
        write_file(&path, "second").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "second");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
