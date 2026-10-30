pub mod glb;
pub mod model;

pub use glb::{read, write, Avatar, Error};
pub use model::AwAvatar;

/// The JSON schema of the `AW_avatar` extension, generated from [`AwAvatar`].
pub fn schema() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(AwAvatar)).expect("a schema serializes")
}
