//! Which App Directory URLs the Desktop Agent may hand to the OS opener.
//!
//! `native`, `citrix` and `other` apps (and `web` apps, which open in the
//! browser) are launched with the OS's default handler for their
//! `details.url`. A `file:` URL to an executable, or a custom scheme
//! registered by some installed program, runs that program, so:
//!
//! - `http` / `https`: always allowed (the user's browser opens them);
//! - `file:`: the target must exist and its canonical path must be inside a
//!   path listed in `nativeApps.allow` (also canonicalised, so symlinks and
//!   `..` can't escape);
//! - any other scheme: allowed only if `nativeApps.allow` lists it as
//!   `<scheme>:` (e.g. `zoommtg:`, `mailto:`);
//! - anything unparsable (including a bare path such as `C:\…` or
//!   `/Applications/…`, which must be written as a `file:` URL) is refused.
//!
//! The list comes from `agent.config.json` (`nativeApps.allow`) or
//! `OT_NATIVE_APP_ALLOW` (entries separated by `;`) and is empty by default.

use std::path::{Path, PathBuf};

use ot_core::widget_url::Url;

/// One parsed `nativeApps.allow` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Entry {
    /// `zoommtg:` → `zoommtg` (lowercased).
    Scheme(String),
    /// An absolute path prefix, canonicalised when it exists.
    Path(PathBuf),
}

fn parse_entry(raw: &str) -> Option<Entry> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    if let Some(scheme) = raw.strip_suffix(':') {
        let ok = scheme.len() > 1
            && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
        if ok {
            return Some(Entry::Scheme(scheme.to_ascii_lowercase()));
        }
    }
    let path = Path::new(raw);
    path.is_absolute()
        .then(|| Entry::Path(path.canonicalize().unwrap_or_else(|_| path.to_path_buf())))
}

/// Check `url` against `allow`. `Err` carries a reason to show the user.
pub fn check(url: &str, allow: &[String]) -> Result<(), String> {
    let parsed = Url::parse(url.trim()).map_err(|_| {
        format!(
            "'{url}' is not a URL; write a program path as a file:// URL and allow it in \
             nativeApps.allow"
        )
    })?;
    let scheme = parsed.scheme().to_ascii_lowercase();
    if scheme == "http" || scheme == "https" {
        return Ok(());
    }
    // A Windows path like `C:\x` parses as scheme `c`: say what it is.
    if scheme.len() == 1 {
        return Err(format!(
            "'{url}' looks like a file path; write it as a file:// URL and allow it in \
             nativeApps.allow"
        ));
    }
    let entries: Vec<Entry> = allow.iter().filter_map(|e| parse_entry(e)).collect();
    if scheme == "file" {
        let target = parsed
            .to_file_path()
            .map_err(|_| format!("'{url}' is not a local file URL"))?;
        let real = target
            .canonicalize()
            .map_err(|e| format!("'{}' can't be opened: {e}", target.display()))?;
        let allowed = entries
            .iter()
            .any(|e| matches!(e, Entry::Path(p) if real.starts_with(p)));
        return if allowed {
            Ok(())
        } else {
            Err(format!(
                "'{}' is not in an allowed folder; add it to nativeApps.allow \
                 (agent.config.json) or OT_NATIVE_APP_ALLOW",
                real.display()
            ))
        };
    }
    if entries
        .iter()
        .any(|e| matches!(e, Entry::Scheme(s) if *s == scheme))
    {
        Ok(())
    } else {
        Err(format!(
            "'{scheme}:' links are not allowed; add \"{scheme}:\" to nativeApps.allow \
             (agent.config.json) or OT_NATIVE_APP_ALLOW"
        ))
    }
}

/// Split an `OT_NATIVE_APP_ALLOW` value into entries (`;`-separated).
pub fn parse_env(value: &str) -> Vec<String> {
    value
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let d = std::env::temp_dir().join(format!(
            "da-native-{}-{}",
            std::process::id(),
            N.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(d.join("tools")).unwrap();
        std::fs::create_dir_all(d.join("other")).unwrap();
        std::fs::write(d.join("tools").join("app"), b"").unwrap();
        std::fs::write(d.join("other").join("evil"), b"").unwrap();
        d
    }

    fn file_url(p: &Path) -> String {
        Url::from_file_path(p).unwrap().to_string()
    }

    #[test]
    fn web_urls_always_open() {
        assert!(check("https://example.com/app", &[]).is_ok());
        assert!(check("http://localhost:3010/", &[]).is_ok());
    }

    #[test]
    fn other_schemes_need_listing() {
        let err = check("zoommtg://zoom.us/join?x=1", &[]).unwrap_err();
        assert!(err.contains("\"zoommtg:\""), "{err}");
        assert!(check("ZoomMtg://zoom.us/join", &["zoommtg:".into()]).is_ok());
        assert!(check("mailto:a@example.com", &["zoommtg:".into()]).is_err());
        assert!(check("mailto:a@example.com", &[" mailto: ".into()]).is_ok());
    }

    #[test]
    fn files_must_be_inside_an_allowed_folder() {
        let d = tmp();
        let allow = vec![d.join("tools").display().to_string()];
        assert!(check(&file_url(&d.join("tools").join("app")), &allow).is_ok());
        let err = check(&file_url(&d.join("other").join("evil")), &allow).unwrap_err();
        assert!(err.contains("not in an allowed folder"), "{err}");
        // `..` out of the allowed folder resolves to the real path first.
        let sneaky = d.join("tools").join("..").join("other").join("evil");
        assert!(check(&file_url(&sneaky), &allow).is_err());
        // Missing targets are refused.
        assert!(check(&file_url(&d.join("tools").join("missing")), &allow).is_err());
        // No list: no files.
        assert!(check(&file_url(&d.join("tools").join("app")), &[]).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_out_of_an_allowed_folder_is_refused() {
        let d = tmp();
        let link = d.join("tools").join("link");
        std::os::unix::fs::symlink(d.join("other").join("evil"), &link).unwrap();
        let allow = vec![d.join("tools").display().to_string()];
        assert!(check(&file_url(&link), &allow).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn non_urls_and_bare_paths_are_refused() {
        for u in ["", "not a url", "/usr/bin/true", "relative/path"] {
            assert!(check(u, &["/".into()]).is_err(), "{u}");
        }
        let err = check(r"C:\Windows\System32\calc.exe", &[]).unwrap_err();
        assert!(err.contains("file path"), "{err}");
    }

    #[test]
    fn entries_parse_as_schemes_or_absolute_paths() {
        assert_eq!(
            parse_entry("zoommtg:"),
            Some(Entry::Scheme("zoommtg".into()))
        );
        assert_eq!(
            parse_entry("ms-teams:"),
            Some(Entry::Scheme("ms-teams".into()))
        );
        assert_eq!(parse_entry("relative/dir"), None);
        assert_eq!(parse_entry("  "), None);
        let abs = if cfg!(windows) {
            r"C:\tools"
        } else {
            "/opt/tools"
        };
        assert!(matches!(parse_entry(abs), Some(Entry::Path(_))));
    }

    #[test]
    fn env_value_splits_on_semicolons() {
        assert_eq!(
            parse_env(" zoommtg: ; /opt/tools ;; "),
            vec!["zoommtg:".to_string(), "/opt/tools".to_string()]
        );
    }
}
