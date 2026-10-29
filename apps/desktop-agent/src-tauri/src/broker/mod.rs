pub mod channel_manager;
pub mod intent_registry;
pub mod types;
pub mod window_manager;

pub use channel_manager::ChannelManager;
pub use intent_registry::IntentRegistry;
pub use types::{
    new_uuid, now_ms, AppIntentInfo, CdaConnectedEvent, CdaContextEvent, CdaDisconnectedEvent,
    CdaErrorCode, CdaIntentEvent, CdaRequest, CdaResponse, ChannelInfo, IntentAppInfo,
    IntentHandlerInfo, WindowHandle,
};
pub use window_manager::WindowManager;
