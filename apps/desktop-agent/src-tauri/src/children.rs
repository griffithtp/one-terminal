//! Processes and windows the Desktop Agent starts on an app's behalf.
//!
//! A [`ChildRegistry`] hands out a [`Slot`] per launch, optionally capped per
//! key (the app id) and in total, and tracks spawned processes by PID so quit
//! can kill exactly those — never another process with the same name — and a
//! watcher thread reaps each one when it exits (no zombies on Unix).
//!
//! Two registries are managed as Tauri state:
//!
//! - [`TerminalRegistry`]: the `one-terminal` processes (no cap).
//! - [`HostRegistry`]: everything launched for an app — `tauri-webview-host`
//!   and Electron host processes, plugin binaries, and in-process app windows
//!   — capped at [`HOST_LIMITS`], because launches can be requested by any
//!   FDC3 client (and the bus doesn't yet authenticate them).

use std::collections::{HashMap, HashSet};
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// How many slots a registry hands out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Live slots per key (app id).
    pub per_key: usize,
    /// Live slots across all keys.
    pub total: usize,
}

/// Caps for [`HostRegistry`]: live windows and processes per app, and in all.
pub const HOST_LIMITS: Limits = Limits {
    per_key: 8,
    total: 32,
};

#[derive(Default)]
struct State {
    pids: HashSet<u32>,
    live: HashMap<String, usize>,
    total: usize,
}

struct Inner {
    /// What the registry tracks, for log lines (`one-terminal`, `app host`).
    what: &'static str,
    limits: Option<Limits>,
    state: Mutex<State>,
    /// Set just before an intentional kill so watcher threads don't log the
    /// resulting exits as crashes.
    quitting: AtomicBool,
}

/// Tracks the slots and PIDs of one kind of child. Cheap to clone.
#[derive(Clone)]
pub struct ChildRegistry(Arc<Inner>);

/// One launch's share of a registry's limits, released when dropped.
pub struct Slot {
    registry: ChildRegistry,
    key: String,
}

impl Drop for Slot {
    fn drop(&mut self) {
        let mut s = self.registry.0.state.lock().unwrap();
        if let Some(n) = s.live.get_mut(&self.key) {
            *n -= 1;
            if *n == 0 {
                s.live.remove(&self.key);
            }
        }
        s.total = s.total.saturating_sub(1);
    }
}

impl ChildRegistry {
    pub fn new(what: &'static str, limits: Option<Limits>) -> Self {
        Self(Arc::new(Inner {
            what,
            limits,
            state: Mutex::new(State::default()),
            quitting: AtomicBool::new(false),
        }))
    }

    /// Take a slot for `key`, or refuse when its cap or the total is reached.
    pub fn reserve(&self, key: &str) -> Result<Slot, String> {
        let mut s = self.0.state.lock().unwrap();
        if let Some(l) = self.0.limits {
            let live = s.live.get(key).copied().unwrap_or(0);
            if live >= l.per_key {
                return Err(format!(
                    "'{key}' already has {live} windows or processes open (limit {})",
                    l.per_key
                ));
            }
            if s.total >= l.total {
                return Err(format!(
                    "{} windows or processes are already open for apps (limit {})",
                    s.total, l.total
                ));
            }
        }
        *s.live.entry(key.to_string()).or_insert(0) += 1;
        s.total += 1;
        drop(s);
        Ok(Slot {
            registry: self.clone(),
            key: key.to_string(),
        })
    }

    /// Record a spawned child and wait on it in a background thread, which
    /// reaps it, forgets its PID, releases `slot`, and logs an unexpected
    /// (non-zero, unrequested) exit.
    pub fn track(&self, mut child: Child, slot: Slot) {
        let pid = child.id();
        self.0.state.lock().unwrap().pids.insert(pid);
        let reg = self.clone();
        std::thread::spawn(move || {
            let status = child.wait();
            reg.0.state.lock().unwrap().pids.remove(&pid);
            drop(slot);
            if reg.0.quitting.load(Ordering::SeqCst) {
                return;
            }
            let what = reg.0.what;
            match status {
                Ok(s) if s.success() => {}
                Ok(s) => tracing::warn!("{what} (pid {pid}) exited unexpectedly: {s}"),
                Err(e) => tracing::warn!("{what} (pid {pid}) wait() failed: {e}"),
            }
        });
    }

    /// PIDs currently tracked.
    pub fn pids(&self) -> Vec<u32> {
        self.0.state.lock().unwrap().pids.iter().copied().collect()
    }

    /// Kill every tracked process by PID. `force` sends SIGKILL / `taskkill
    /// /F` for immediate termination; otherwise SIGTERM / `taskkill`
    /// (graceful: a Tauri app can flush pending writes). `tree` also ends the
    /// process's descendants on Windows (`/T`; Electron and CEF hosts run
    /// helper processes); on macOS and Linux the hosts' helpers exit with
    /// their parent.
    pub fn kill_all(&self, force: bool, tree: bool) {
        self.0.quitting.store(true, Ordering::SeqCst);
        for pid in self.pids() {
            #[cfg(target_os = "windows")]
            {
                let mut cmd = std::process::Command::new("taskkill");
                cmd.args(["/PID", &pid.to_string()]);
                if tree {
                    cmd.arg("/T");
                }
                if force {
                    cmd.arg("/F");
                }
                let _ = cmd.output();
            }
            #[cfg(not(target_os = "windows"))]
            {
                let _ = tree;
                let sig = if force { "-KILL" } else { "-TERM" };
                let _ = std::process::Command::new("kill")
                    .args([sig, &pid.to_string()])
                    .output();
            }
        }
    }
}

/// The `one-terminal` processes this Desktop Agent spawned (no cap), so quit
/// kills precisely those instead of every `one-terminal` on the machine (a
/// bare `killall`/`taskkill /IM` would also kill an unrelated standalone
/// install), and a crash gets logged.
#[derive(Clone)]
pub struct TerminalRegistry(ChildRegistry);

impl Default for TerminalRegistry {
    fn default() -> Self {
        Self(ChildRegistry::new("one-terminal", None))
    }
}

impl TerminalRegistry {
    pub fn track(&self, child: Child) {
        let slot = self
            .0
            .reserve("one-terminal")
            .expect("the Terminal registry has no limits");
        self.0.track(child, slot);
    }

    pub fn kill_all(&self, force: bool) {
        self.0.kill_all(force, false);
    }

    #[cfg(test)]
    pub fn pids(&self) -> Vec<u32> {
        self.0.pids()
    }
}

/// Windows and processes launched for apps, capped per app id and in total
/// ([`HOST_LIMITS`]) and killed when the Desktop Agent quits.
#[derive(Clone)]
pub struct HostRegistry(ChildRegistry);

impl Default for HostRegistry {
    fn default() -> Self {
        Self(ChildRegistry::new("app host", Some(HOST_LIMITS)))
    }
}

impl std::ops::Deref for HostRegistry {
    type Target = ChildRegistry;
    fn deref(&self) -> &ChildRegistry {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limited(per_key: usize, total: usize) -> ChildRegistry {
        ChildRegistry::new("test", Some(Limits { per_key, total }))
    }

    #[test]
    fn per_key_cap_and_release() {
        let r = limited(2, 10);
        let a1 = r.reserve("a").unwrap();
        let _a2 = r.reserve("a").unwrap();
        let err = r.reserve("a").err().unwrap();
        assert!(err.contains("limit 2"), "{err}");
        // Other apps are unaffected.
        let _b = r.reserve("b").unwrap();
        drop(a1);
        assert!(r.reserve("a").is_ok());
    }

    #[test]
    fn total_cap_and_release() {
        let r = limited(10, 3);
        let s: Vec<_> = ["a", "b", "c"]
            .iter()
            .map(|k| r.reserve(k).unwrap())
            .collect();
        assert!(r.reserve("d").err().unwrap().contains("limit 3"));
        drop(s);
        assert!(r.reserve("d").is_ok());
    }

    #[test]
    fn no_limits_never_refuses() {
        let r = ChildRegistry::new("test", None);
        let slots: Vec<_> = (0..100).map(|_| r.reserve("x").unwrap()).collect();
        assert_eq!(slots.len(), 100);
    }

    #[test]
    fn host_limits_are_set() {
        let h = HostRegistry::default();
        let slots: Vec<_> = (0..HOST_LIMITS.per_key)
            .map(|_| h.reserve("spam").unwrap())
            .collect();
        assert!(h.reserve("spam").is_err());
        drop(slots);
    }
}

#[cfg(all(test, unix))]
mod process_tests {
    use super::*;
    use std::process::Command;
    use std::time::Duration;

    fn is_alive(pid: u32) -> bool {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    /// Polls `condition` until it holds or `timeout` passes. These tests wait on other
    /// processes and the watcher thread, so a fixed sleep is flaky on a loaded CI runner
    /// (tests run in parallel); polling passes as soon as the state is reached.
    fn wait_until(timeout: Duration, mut condition: impl FnMut() -> bool) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        while std::time::Instant::now() < deadline {
            if condition() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        condition()
    }

    /// Regression test for the bug `TerminalRegistry` replaced: a bare
    /// `killall one-terminal` / `taskkill /IM one-terminal.exe` kills every
    /// process with that name on the machine, including an unrelated,
    /// independently-launched standalone `one-terminal` — not just the
    /// instance(s) this `desktop-agent` process spawned. `kill_all` must
    /// only touch tracked PIDs.
    #[test]
    fn kill_all_only_kills_tracked_pids() {
        let registry = TerminalRegistry::default();

        let tracked_child = Command::new("sleep").arg("30").spawn().unwrap();
        let tracked_pid = tracked_child.id();
        registry.track(tracked_child);

        let mut unrelated_child = Command::new("sleep").arg("30").spawn().unwrap();
        let unrelated_pid = unrelated_child.id();

        assert!(is_alive(tracked_pid), "tracked child should be running");
        assert!(is_alive(unrelated_pid), "unrelated child should be running");

        registry.kill_all(true);

        assert!(
            wait_until(Duration::from_secs(10), || !is_alive(tracked_pid)),
            "tracked child should have been killed"
        );
        assert!(
            is_alive(unrelated_pid),
            "unrelated child must survive kill_all — this is the bug being fixed"
        );

        let _ = unrelated_child.kill();
        let _ = unrelated_child.wait();
    }

    /// The watcher thread must forget the PID once the child exits on its
    /// own — otherwise the registry accumulates dead PIDs forever.
    #[test]
    fn track_removes_pid_after_natural_exit() {
        let registry = TerminalRegistry::default();
        let child = Command::new("sleep").arg("0.2").spawn().unwrap();
        let pid = child.id();
        registry.track(child);

        assert!(registry.pids().contains(&pid));
        assert!(
            wait_until(Duration::from_secs(10), || !registry.pids().contains(&pid)),
            "watcher thread should have pruned the PID after natural exit"
        );
    }

    /// A host's slot is held while the process runs and released when it
    /// exits, so a finished launch frees capacity; quit kills live hosts.
    #[test]
    fn host_slots_follow_the_process_and_quit_kills_hosts() {
        let hosts = ChildRegistry::new(
            "test host",
            Some(Limits {
                per_key: 1,
                total: 4,
            }),
        );

        let short = Command::new("sleep").arg("0.2").spawn().unwrap();
        hosts.track(short, hosts.reserve("app").unwrap());
        assert!(hosts.reserve("app").is_err(), "slot held while running");
        assert!(
            wait_until(Duration::from_secs(10), || hosts.reserve("app").is_ok()),
            "slot released after the process exited"
        );

        let long = Command::new("sleep").arg("30").spawn().unwrap();
        let pid = long.id();
        hosts.track(long, hosts.reserve("app").unwrap());
        hosts.kill_all(false, true);
        assert!(
            wait_until(Duration::from_secs(10), || !is_alive(pid)),
            "kill_all should end tracked hosts"
        );
    }
}
