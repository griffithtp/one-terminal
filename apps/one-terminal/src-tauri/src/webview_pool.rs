use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use tauri::{AppHandle, LogicalPosition, LogicalSize, Manager, WebviewBuilder, WebviewUrl};
use uuid::Uuid;

use crate::{OverlayState, WIN};

/// Pool of pre-warmed blank webviews, ready for immediate navigation.
///
/// Controlled by `OT_WEBVIEW_POOL_SIZE` (default 1; 0 = disabled). The inner
/// `Arc` lets `replenish` capture a reference across async task boundaries
/// while Tauri holds the outer value in managed state.
#[derive(Clone)]
pub struct WebviewPool {
    available: Arc<Mutex<VecDeque<String>>>,
    pub target_size: usize,
}

impl WebviewPool {
    pub fn new(target_size: usize) -> Self {
        Self {
            available: Arc::new(Mutex::new(VecDeque::new())),
            target_size,
        }
    }

    /// Record a pre-created blank webview label. Called from the setup closure
    /// on the main thread after each successful `win.add_child`.
    pub fn push(&self, label: String) {
        self.available.lock().unwrap().push_back(label);
    }

    /// Pop a pre-warmed webview label, if any are available and the pool is
    /// enabled. Returns `None` when pool size is 0 or all slots are in use.
    pub fn take(&self) -> Option<String> {
        if self.target_size == 0 {
            return None;
        }
        self.available.lock().unwrap().pop_front()
    }

    /// Spawn a background task to restore the pool to `target_size` by
    /// creating one additional blank webview child. No-op if already at
    /// capacity or the pool is disabled.
    ///
    /// The new webview lands after the overlay in z-order (above it), so the
    /// overlay is marked stale so `wm_ctx_menu_open` will push it back to
    /// the top on the next context-menu open.
    pub fn replenish(&self, app: &AppHandle, overlay: &OverlayState) {
        let depth = self.available.lock().unwrap().len();
        if self.target_size == 0 || depth >= self.target_size {
            return;
        }

        let available = Arc::clone(&self.available);
        let app = app.clone();
        let overlay = Arc::clone(overlay);

        tauri::async_runtime::spawn(async move {
            let label = pool_label(crate::WIN);
            let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
            let label_main = label.clone();
            let app_main = app.clone();

            if app
                .run_on_main_thread(move || {
                    let result = (|| -> Result<(), String> {
                        let win = app_main
                            .get_window(WIN)
                            .ok_or_else(|| "wm window not found".to_string())?;
                        let sf = win.scale_factor().unwrap_or(1.0);
                        let sz = win.inner_size().unwrap_or(tauri::PhysicalSize {
                            width: 1600,
                            height: 900,
                        });
                        let (w, h) = (sz.width as f64 / sf, sz.height as f64 / sf);
                        win.add_child(
                            WebviewBuilder::new(
                                &label_main,
                                WebviewUrl::External(
                                    "about:blank".parse().expect("about:blank is a valid URL"),
                                ),
                            ),
                            LogicalPosition::new(-20000.0, -20000.0),
                            LogicalSize::new(w, h),
                        )
                        .map_err(|e| e.to_string())?;
                        // New webview is above the overlay in z-order.
                        let mut inner = overlay.lock().unwrap();
                        inner.stale = true;
                        inner.is_ready = false;
                        Ok(())
                    })();
                    let _ = tx.send(result);
                })
                .is_err()
            {
                return;
            }

            match rx.recv() {
                Ok(Ok(())) => {
                    available.lock().unwrap().push_back(label.clone());
                    eprintln!("[pool] +1 ready: {label}");
                }
                Ok(Err(e)) => eprintln!("[pool] replenish failed: {e}"),
                Err(_) => {}
            }
        });
    }
}

pub fn pool_label(terminal_id: &str) -> String {
    format!("{}-pool-{}", terminal_id, &Uuid::new_v4().to_string()[..8])
}
