//! Per-user log directory, shared by every OneTerminal process (one file per
//! service per day — see [`crate::logs`]).

use std::path::PathBuf;

const PRODUCT: &str = "one-terminal";

/// Default directory for log files:
/// - macOS:   `~/Library/Logs/one-terminal/`
/// - Windows: `%LOCALAPPDATA%\one-terminal\logs\`
/// - Linux:   `$XDG_STATE_HOME/one-terminal/logs/` or `~/.local/state/one-terminal/logs/`
///
/// `None` when the platform's base directory can't be determined; the caller
/// then logs to stderr only.
pub fn default_log_dir(env: &dyn Fn(&str) -> Option<String>) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        env("HOME").map(|home| {
            PathBuf::from(home)
                .join("Library")
                .join("Logs")
                .join(PRODUCT)
        })
    }

    #[cfg(target_os = "windows")]
    {
        env("LOCALAPPDATA").map(|dir| PathBuf::from(dir).join(PRODUCT).join("logs"))
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        env("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| env("HOME").map(|h| PathBuf::from(h).join(".local").join("state")))
            .map(|dir| dir.join(PRODUCT).join("logs"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_with(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        move |key| pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_uses_library_logs() {
        let dir = default_log_dir(&env_with(&[("HOME", "/Users/me")]));
        assert_eq!(
            dir,
            Some(PathBuf::from("/Users/me/Library/Logs/one-terminal"))
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_uses_local_app_data() {
        let dir = default_log_dir(&env_with(&[("LOCALAPPDATA", r"C:\Users\me\AppData\Local")]));
        assert_eq!(
            dir,
            Some(PathBuf::from(
                r"C:\Users\me\AppData\Local\one-terminal\logs"
            ))
        );
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn linux_prefers_xdg_state_home() {
        let dir = default_log_dir(&env_with(&[("XDG_STATE_HOME", "/s"), ("HOME", "/h")]));
        assert_eq!(dir, Some(PathBuf::from("/s/one-terminal/logs")));
        let dir = default_log_dir(&env_with(&[("HOME", "/h")]));
        assert_eq!(
            dir,
            Some(PathBuf::from("/h/.local/state/one-terminal/logs"))
        );
    }

    #[test]
    fn missing_base_dir_gives_none() {
        assert_eq!(default_log_dir(&env_with(&[])), None);
    }
}
