//! Real-time support chat plumbing: the wire protocol, the connection hub and
//! the per-connection rate limiter. Storage lives in `repos::support`, the
//! rules in `services::support`, the WebSocket transport in `wog-api`.

pub mod hub;
pub mod limiter;
pub mod protocol;

pub use hub::{
    Audience, ConnId, DynSupportHub, HubConfig, HubError, LocalHub, Registration, SupportHub,
    Unregistered,
};
pub use limiter::WindowLimiter;
pub use protocol::{ChatRole, ClientEvent, ServerEvent};
