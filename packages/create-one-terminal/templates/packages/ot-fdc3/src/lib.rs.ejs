mod client;
mod commands;
mod state;
mod types;

use std::sync::Arc;
use tauri::{
    plugin::{Builder as PluginBuilder, TauriPlugin},
    AppHandle, Manager, Runtime,
};

pub use state::OtFdc3State;
pub use types::{FdcChannelJoinedEvent, FdcContextEvent, FdcIntentEvent};

use commands::*;

/// Initialise the `ot-fdc3` Tauri plugin.
///
/// Call once in your app's `tauri::Builder`:
/// ```rust
/// tauri::Builder::default()
///     .plugin(ot_fdc3::init())
///     // ...
/// ```
///
/// The FDC3 TCP client is **not** started here — it starts on the first
/// `fdc3_register` call from the TypeScript frontend.
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    PluginBuilder::new("ot-fdc3")
        .setup(|app, _api| {
            app.manage(Arc::new(OtFdc3State::new()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            fdc3_register,
            fdc3_get_system_channels,
            fdc3_get_registered_apps,
            fdc3_get_or_create_channel,
            fdc3_create_private_channel,
            fdc3_join_user_channel,
            fdc3_leave_current_channel,
            fdc3_get_current_channel,
            fdc3_get_current_context,
            fdc3_broadcast,
            fdc3_add_intent_listener,
            fdc3_remove_intent_listener,
            fdc3_find_intent,
            fdc3_find_intents_for_context,
            fdc3_raise_intent,
            fdc3_raise_intent_for_context,
        ])
        .build()
}

/// Retrieve the managed `OtFdc3State` from an `AppHandle`.
///
/// Useful for Tauri spoke apps that need to interact with the FDC3 state
/// from Rust (e.g. a mock streamer that broadcasts prices on a channel).
///
/// Panics if `ot_fdc3::init()` was not registered as a plugin.
pub fn get_state<R: Runtime>(app: &AppHandle<R>) -> Arc<OtFdc3State> {
    app.state::<Arc<OtFdc3State>>().inner().clone()
}
