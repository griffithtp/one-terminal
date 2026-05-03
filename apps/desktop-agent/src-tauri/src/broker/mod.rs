pub mod channel_manager;
pub mod intent_registry;
pub mod types;
pub mod window_manager;

pub use channel_manager::ChannelManager;
pub use intent_registry::IntentRegistry;
pub use types::{
    CdaConnectedEvent, CdaContextEvent, CdaDisconnectedEvent, CdaIntentEvent,
    CdaErrorCode, CdaRequest, CdaResponse,
    ChannelInfo, IntentHandlerInfo, WindowHandle,
    new_uuid, now_ms,
};
pub use window_manager::WindowManager;
