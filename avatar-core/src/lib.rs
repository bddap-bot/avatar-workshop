pub mod animator;
pub mod constraints;
pub mod contacts;
pub mod curve;
pub mod evaluate;
pub mod glb;
pub mod humanoid;
pub mod model;
pub mod physbones;
pub mod validate;
pub mod vrm;

pub use glb::{read, write, Avatar, Error, MAX_JSON_BYTES};
pub use model::AwAvatar;

/// Whether `node` and each of its ancestors are active.
pub(crate) fn active_in_hierarchy(active: &[bool], parent: &[Option<usize>], node: usize) -> bool {
    std::iter::successors(Some(node), |&n| parent[n]).all(|n| active[n])
}

/// The JSON schema of the `AW_avatar` extension, generated from [`AwAvatar`].
pub fn schema() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(AwAvatar)).expect("a schema serializes")
}
