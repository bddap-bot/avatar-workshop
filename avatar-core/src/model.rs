// Model records exist once per avatar, so variant size does not matter.
#![allow(clippy::large_enum_variant)]

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub type Vec2 = [f32; 2];
pub type Vec3 = [f32; 3];
pub type Vec4 = [f32; 4];
/// x, y, z, w
pub type Quat = [f32; 4];
/// Column-major, as glTF.
pub type Mat4 = [f32; 16];

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct NodeIndex(pub u32);

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct MaterialIndex(pub u32);

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct TextureIndex(pub u32);

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct ClipIndex(pub u32);

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct BehaviourIndex(pub u32);

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct ColliderIndex(pub u32);

/// Unique among the states of one controller.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct StateId(pub u32);

/// The `AW_avatar` extension on the glTF root.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[schemars(title = "AW_avatar")]
pub struct AwAvatar {
    pub source: Source,
    pub humanoid: Humanoid,
    pub descriptor: Descriptor,
    pub renderers: Vec<Renderer>,
    /// One entry per glTF node, in node order.
    pub nodes: Vec<Node>,
    /// One entry per glTF material, in material order.
    pub materials: Vec<Material>,
    pub physbones: Vec<PhysBone>,
    pub colliders: Vec<Collider>,
    pub contacts: Contacts,
    pub constraints: Vec<Constraint>,
    pub parameters: Vec<Parameter>,
    pub menu: Menu,
    pub layers: BTreeMap<PlayableLayerType, PlayableLayer>,
    pub behaviours: Vec<Behaviour>,
    pub clips: Vec<Clip>,
    pub unsupported: Vec<Unsupported>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct StateMachineId(pub u32);

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct PhysBoneIndex(pub u32);

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct ConstraintIndex(pub u32);

/// Into the senders or the receivers, as the binding names.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub struct ContactIndex(pub u32);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Source {
    pub importer: String,
    pub importer_version: String,
    pub format: SourceFormat,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub licence: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum SourceFormat {
    Vrchat,
    Vrm0,
    Vrm1,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Humanoid {
    pub bones: BTreeMap<HumanBodyBone, HumanBone>,
    /// The source's rest pose as given.
    pub skeleton: Vec<SkeletonBone>,
    pub arm_stretch: f32,
    pub leg_stretch: f32,
    pub upper_arm_twist: f32,
    pub lower_arm_twist: f32,
    pub upper_leg_twist: f32,
    pub lower_leg_twist: f32,
    pub feet_spacing: f32,
    pub has_translation_do_f: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HumanBone {
    pub node: NodeIndex,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<MuscleLimit>,
}

/// Degrees; absent on a bone that uses Unity's default limits.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MuscleLimit {
    pub min: Vec3,
    pub max: Vec3,
    pub center: Vec3,
    pub axis_length: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkeletonBone {
    pub node: NodeIndex,
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
pub enum HumanBodyBone {
    Hips,
    LeftUpperLeg,
    RightUpperLeg,
    LeftLowerLeg,
    RightLowerLeg,
    LeftFoot,
    RightFoot,
    Spine,
    Chest,
    UpperChest,
    Neck,
    Head,
    LeftShoulder,
    RightShoulder,
    LeftUpperArm,
    RightUpperArm,
    LeftLowerArm,
    RightLowerArm,
    LeftHand,
    RightHand,
    LeftToes,
    RightToes,
    LeftEye,
    RightEye,
    Jaw,
    LeftThumbProximal,
    LeftThumbIntermediate,
    LeftThumbDistal,
    LeftIndexProximal,
    LeftIndexIntermediate,
    LeftIndexDistal,
    LeftMiddleProximal,
    LeftMiddleIntermediate,
    LeftMiddleDistal,
    LeftRingProximal,
    LeftRingIntermediate,
    LeftRingDistal,
    LeftLittleProximal,
    LeftLittleIntermediate,
    LeftLittleDistal,
    RightThumbProximal,
    RightThumbIntermediate,
    RightThumbDistal,
    RightIndexProximal,
    RightIndexIntermediate,
    RightIndexDistal,
    RightMiddleProximal,
    RightMiddleIntermediate,
    RightMiddleDistal,
    RightRingProximal,
    RightRingIntermediate,
    RightRingDistal,
    RightLittleProximal,
    RightLittleIntermediate,
    RightLittleDistal,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Descriptor {
    pub view_position: Vec3,
    pub lip_sync: LipSync,
    pub eyes: Eyes,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "mode", rename_all = "camelCase", deny_unknown_fields)]
pub enum LipSync {
    Default {},
    #[serde(rename_all = "camelCase")]
    JawFlapBone {
        bone: NodeIndex,
        closed: Quat,
        open: Quat,
    },
    #[serde(rename_all = "camelCase")]
    JawFlapBlendShape {
        mesh: NodeIndex,
        blend_shape: String,
    },
    #[serde(rename_all = "camelCase")]
    VisemeBlendShape {
        mesh: NodeIndex,
        visemes: Visemes,
    },
    VisemeParameterOnly {},
}

/// Blendshape names, in VRChat's viseme order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Visemes {
    pub sil: String,
    #[serde(rename = "PP")]
    pub pp: String,
    #[serde(rename = "FF")]
    pub ff: String,
    #[serde(rename = "TH")]
    pub th: String,
    #[serde(rename = "DD")]
    pub dd: String,
    pub kk: String,
    #[serde(rename = "CH")]
    pub ch: String,
    #[serde(rename = "SS")]
    pub ss: String,
    pub nn: String,
    #[serde(rename = "RR")]
    pub rr: String,
    pub aa: String,
    #[serde(rename = "E")]
    pub e: String,
    pub ih: String,
    pub oh: String,
    pub ou: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Eyes {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub look: Option<EyeLook>,
    pub eyelids: Eyelids,
    pub eye_movement: EyeMovement,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EyeMovement {
    pub confidence: f32,
    pub excitement: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EyeLook {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub left_eye: Option<NodeIndex>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub right_eye: Option<NodeIndex>,
    pub straight: EyeRotations,
    pub up: EyeRotations,
    pub down: EyeRotations,
    pub left: EyeRotations,
    pub right: EyeRotations,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EyeRotations {
    pub linked: bool,
    pub left: Quat,
    pub right: Quat,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum Eyelids {
    None {},
    #[serde(rename_all = "camelCase")]
    Bones {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        left_upper: Option<NodeIndex>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        right_upper: Option<NodeIndex>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        left_lower: Option<NodeIndex>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        right_lower: Option<NodeIndex>,
        default: EyelidRotations,
        closed: EyelidRotations,
        looking_up: EyelidRotations,
        looking_down: EyelidRotations,
    },
    #[serde(rename_all = "camelCase")]
    BlendShapes {
        mesh: NodeIndex,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        blink: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        looking_up: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        looking_down: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EyelidRotations {
    pub upper: EyeRotations,
    pub lower: EyeRotations,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Renderer {
    pub node: NodeIndex,
    pub enabled: bool,
    /// The renderer's material slots, in slot order. A mesh shared by several renderers keeps
    /// its own primitive materials as the generic viewer's fallback.
    pub materials: Vec<MaterialIndex>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Node {
    /// After evaluating every playable layer's default state at time 0.
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Material {
    pub shader: ShaderFamily,
    /// The source shader's own name.
    pub shader_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub properties: BTreeMap<String, MaterialProperty>,
    pub keywords: Vec<String>,
    /// The render state the shader pass resolves from its properties.
    pub render_queue: i32,
    pub cull: Cull,
    /// Absent for opaque and cut-out materials.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blend: Option<Blend>,
    pub depth_write: bool,
    pub depth_test: CompareFunction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ShaderFamily {
    Poiyomi,
    Liltoon,
    VrchatMobile,
    Mtoon,
    Standard,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum MaterialProperty {
    Float(f32),
    Int(i32),
    Color(Vec4),
    Vector(Vec4),
    Texture(TextureProperty),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextureProperty {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub texture: Option<TextureIndex>,
    pub tiling: Vec2,
    pub offset: Vec2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Cull {
    Off,
    Front,
    Back,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Blend {
    pub src_color: BlendFactor,
    pub dst_color: BlendFactor,
    pub src_alpha: BlendFactor,
    pub dst_alpha: BlendFactor,
    pub color_op: BlendOp,
    pub alpha_op: BlendOp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum BlendFactor {
    Zero,
    One,
    DstColor,
    SrcColor,
    OneMinusDstColor,
    SrcAlpha,
    OneMinusSrcColor,
    DstAlpha,
    OneMinusDstAlpha,
    SrcAlphaSaturate,
    OneMinusSrcAlpha,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum BlendOp {
    Add,
    Subtract,
    ReverseSubtract,
    Min,
    Max,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum CompareFunction {
    Never,
    Less,
    Equal,
    LessEqual,
    Greater,
    NotEqual,
    GreaterEqual,
    Always,
}

/// Unity's `AnimationCurve`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnimationCurve {
    pub keys: Vec<Keyframe>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Keyframe {
    pub time: f32,
    pub value: f32,
    pub in_tangent: f32,
    pub out_tangent: f32,
    pub in_weight: f32,
    pub out_weight: f32,
    pub weighted_mode: WeightedMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum WeightedMode {
    None,
    In,
    Out,
    Both,
}

/// A VRChat field with its optional curve over the chain.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Curved {
    pub value: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub curve: Option<AnimationCurve>,
}

/// Who may collide with, grab or pose a component.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Permission {
    pub allow_self: bool,
    pub allow_others: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PhysBone {
    /// The node the component sits on.
    pub node: NodeIndex,
    pub enabled: bool,
    pub version: PhysBoneVersion,
    pub root: NodeIndex,
    pub ignore_transforms: Vec<NodeIndex>,
    pub endpoint_position: Vec3,
    pub multi_child_type: MultiChildType,
    pub integration_type: IntegrationType,
    pub pull: Curved,
    /// Called momentum under the advanced integration type.
    pub spring: Curved,
    pub stiffness: Curved,
    pub gravity: Curved,
    pub gravity_falloff: Curved,
    pub immobile_type: ImmobileType,
    pub immobile: Curved,
    pub limit: Limit,
    pub radius: Curved,
    pub max_stretch: Curved,
    pub stretch_motion: Curved,
    pub max_squish: Curved,
    pub colliders: Vec<ColliderIndex>,
    pub collision: Permission,
    pub grabbing: Permission,
    pub posing: Permission,
    pub grab_movement: f32,
    pub snap_to_hand: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameter: Option<String>,
    pub is_animated: bool,
    pub reset_when_disabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum PhysBoneVersion {
    #[serde(rename = "1.0")]
    V1_0,
    #[serde(rename = "1.1")]
    V1_1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum MultiChildType {
    Ignore,
    First,
    Average,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum IntegrationType {
    Simplified,
    Advanced,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ImmobileType {
    AllMotion,
    World,
}

/// Angles in degrees.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum Limit {
    None {},
    #[serde(rename_all = "camelCase")]
    Angle {
        max_angle_x: Curved,
        rotation: LimitRotation,
    },
    #[serde(rename_all = "camelCase")]
    Hinge {
        max_angle_x: Curved,
        rotation: LimitRotation,
    },
    #[serde(rename_all = "camelCase")]
    Polar {
        max_angle_x: Curved,
        max_angle_z: Curved,
        rotation: LimitRotation,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LimitRotation {
    pub value: Vec3,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x_curve: Option<AnimationCurve>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y_curve: Option<AnimationCurve>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub z_curve: Option<AnimationCurve>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Collider {
    pub node: NodeIndex,
    pub shape: ColliderShape,
    pub position: Vec3,
    pub rotation: Quat,
    pub bones_as_spheres: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum ColliderShape {
    #[serde(rename_all = "camelCase")]
    Sphere {
        radius: f32,
        inside_bounds: bool,
    },
    #[serde(rename_all = "camelCase")]
    Capsule {
        radius: f32,
        height: f32,
        inside_bounds: bool,
    },
    Plane {},
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Contacts {
    pub senders: Vec<ContactSender>,
    pub receivers: Vec<ContactReceiver>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum ContactShape {
    Sphere { radius: f32 },
    Capsule { radius: f32, height: f32 },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContactSender {
    pub node: NodeIndex,
    pub enabled: bool,
    pub shape: ContactShape,
    pub position: Vec3,
    pub rotation: Quat,
    pub collision_tags: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContactReceiver {
    pub node: NodeIndex,
    pub enabled: bool,
    pub shape: ContactShape,
    pub position: Vec3,
    pub rotation: Quat,
    pub collision_tags: Vec<String>,
    pub allow_self: bool,
    pub allow_others: bool,
    pub local_only: bool,
    pub receiver_type: ReceiverType,
    pub parameter: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum ReceiverType {
    Constant {},
    #[serde(rename_all = "camelCase")]
    OnEnter {
        min_velocity: f32,
    },
    Proximity {},
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Constraint {
    /// The node the component sits on.
    pub node: NodeIndex,
    pub enabled: bool,
    pub is_active: bool,
    /// The node the constraint moves.
    pub target: NodeIndex,
    pub weight: f32,
    pub sources: Vec<ConstraintSource>,
    pub solve_in_local_space: bool,
    pub freeze_to_world: bool,
    pub rebake_offsets_when_unfrozen: bool,
    pub kind: ConstraintKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConstraintSource {
    pub source: SourceTarget,
    pub weight: f32,
    /// Parent constraints only.
    pub position_offset: Vec3,
    /// Parent constraints only; Euler degrees.
    pub rotation_offset: Vec3,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum SourceTarget {
    Node(NodeIndex),
    World(Mat4),
}

/// At-rest and offset values with the axes the constraint affects.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Channel {
    pub at_rest: Vec3,
    pub offset: Vec3,
    pub affects: [bool; 3],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum ConstraintKind {
    Parent {
        position: Channel,
        rotation: Channel,
    },
    Position {
        position: Channel,
    },
    Rotation {
        rotation: Channel,
    },
    #[serde(rename_all = "camelCase")]
    Aim {
        rotation: Channel,
        aim_axis: Vec3,
        up_axis: Vec3,
        world_up: WorldUp,
    },
    #[serde(rename_all = "camelCase")]
    LookAt {
        rotation: Channel,
        roll: f32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        world_up_node: Option<NodeIndex>,
    },
    Scale {
        scale: Channel,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum WorldUp {
    SceneUp,
    ObjectUp { node: NodeIndex },
    ObjectRotationUp { node: NodeIndex, vector: Vec3 },
    Vector { vector: Vec3 },
    None {},
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Parameter {
    pub name: String,
    pub default: ParameterValue,
    pub saved: bool,
    pub synced: bool,
}

/// The variant is the parameter's type.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum ParameterValue {
    Int(i32),
    Float(f32),
    Bool(bool),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Menu {
    pub controls: Vec<Control>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Control {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<TextureIndex>,
    pub kind: ControlKind,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum ControlKind {
    Button {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parameter: Option<String>,
        value: f32,
    },
    Toggle {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parameter: Option<String>,
        value: f32,
    },
    #[serde(rename_all = "camelCase")]
    SubMenu {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parameter: Option<String>,
        value: f32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sub_menu: Option<Menu>,
    },
    /// Labels are up, right, down, left.
    #[serde(rename_all = "camelCase")]
    TwoAxisPuppet {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parameter: Option<String>,
        value: f32,
        sub_parameters: [Option<String>; 2],
        labels: [Label; 4],
    },
    /// Sub-parameters and labels are up, right, down, left.
    #[serde(rename_all = "camelCase")]
    FourAxisPuppet {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parameter: Option<String>,
        value: f32,
        sub_parameters: [Option<String>; 4],
        labels: [Label; 4],
    },
    #[serde(rename_all = "camelCase")]
    RadialPuppet {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        parameter: Option<String>,
        value: f32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sub_parameter: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Label {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<TextureIndex>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlayableLayer {
    /// Absent when the avatar uses VRChat's default controller for this layer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controller: Option<Controller>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask: Option<AvatarMask>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub enum PlayableLayerType {
    Base,
    Additive,
    Gesture,
    Action,
    #[serde(rename = "fx")]
    Fx,
    Sitting,
    TPose,
    IkPose,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Controller {
    pub name: String,
    pub parameters: Vec<AnimatorParameter>,
    pub layers: Vec<AnimatorLayer>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnimatorParameter {
    pub name: String,
    pub default: AnimatorParameterValue,
}

/// The variant is the parameter's type.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum AnimatorParameterValue {
    Float(f32),
    Int(i32),
    Bool(bool),
    Trigger(bool),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AnimatorLayer {
    pub name: String,
    pub default_weight: f32,
    pub blending: LayerBlending,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mask: Option<AvatarMask>,
    pub ik_pass: bool,
    pub content: LayerContent,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum LayerContent {
    #[serde(rename_all = "camelCase")]
    StateMachine { state_machine: StateMachine },
    /// Runs the state machine of the layer at index `layer` with this layer's own motion and
    /// behaviours for each state it lists.
    #[serde(rename_all = "camelCase")]
    Synced {
        layer: u32,
        affects_timing: bool,
        states: Vec<SyncedState>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SyncedState {
    pub state: StateId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion: Option<Motion>,
    pub behaviours: Vec<BehaviourIndex>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum LayerBlending {
    Override,
    Additive,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AvatarMask {
    /// Humanoid body parts the mask lets through.
    #[serde(deserialize_with = "unique")]
    pub body_parts: BTreeSet<BodyPart>,
    /// Transforms the mask names, each let through or not.
    pub transforms: Vec<MaskTransform>,
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
pub enum BodyPart {
    Root,
    Body,
    Head,
    LeftLeg,
    RightLeg,
    LeftArm,
    RightArm,
    LeftFingers,
    RightFingers,
    LeftFootIK,
    RightFootIK,
    LeftHandIK,
    RightHandIK,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MaskTransform {
    pub node: NodeIndex,
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StateMachine {
    pub id: StateMachineId,
    pub name: String,
    pub states: Vec<State>,
    pub state_machines: Vec<StateMachine>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_state: Option<StateId>,
    pub any_state_transitions: Vec<Transition>,
    pub entry_transitions: Vec<EntryTransition>,
    /// Transitions from this machine's exit node, which Unity keeps on its parent.
    pub state_machine_transitions: Vec<EntryTransition>,
    pub behaviours: Vec<BehaviourIndex>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct State {
    pub id: StateId,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tag: String,
    pub speed: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed_parameter: Option<String>,
    pub cycle_offset: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cycle_offset_parameter: Option<String>,
    pub mirror: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mirror_parameter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_parameter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion: Option<Motion>,
    pub write_defaults: bool,
    pub ik_on_feet: bool,
    pub behaviours: Vec<BehaviourIndex>,
    pub transitions: Vec<Transition>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum Destination {
    State { id: StateId },
    StateMachine { id: StateMachineId },
    Exit {},
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Transition {
    pub destination: Destination,
    pub conditions: Vec<Condition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_time: Option<f32>,
    pub duration: Duration,
    pub offset: f32,
    pub interruption_source: InterruptionSource,
    pub ordered_interruption: bool,
    pub can_transition_to_self: bool,
    pub mute: bool,
    pub solo: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EntryTransition {
    pub destination: Destination,
    pub conditions: Vec<Condition>,
    pub mute: bool,
    pub solo: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum Duration {
    /// Seconds.
    Fixed(f32),
    /// A fraction of the source state's length.
    Normalized(f32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum InterruptionSource {
    None,
    Source,
    Destination,
    SourceThenDestination,
    DestinationThenSource,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Condition {
    pub mode: ConditionMode,
    pub parameter: String,
    pub threshold: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ConditionMode {
    If,
    IfNot,
    Greater,
    Less,
    Equals,
    NotEqual,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub enum Motion {
    Clip(ClipIndex),
    BlendTree(Box<BlendTree>),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BlendTree {
    pub name: String,
    pub algorithm: BlendAlgorithm,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum BlendAlgorithm {
    #[serde(rename_all = "camelCase")]
    Simple1D {
        blend_parameter: String,
        children: Vec<Child1D>,
    },
    #[serde(rename = "simpleDirectional2D", rename_all = "camelCase")]
    SimpleDirectional2D {
        blend_parameter: String,
        blend_parameter_y: String,
        children: Vec<Child2D>,
    },
    #[serde(rename = "freeformDirectional2D", rename_all = "camelCase")]
    FreeformDirectional2D {
        blend_parameter: String,
        blend_parameter_y: String,
        children: Vec<Child2D>,
    },
    #[serde(rename = "freeformCartesian2D", rename_all = "camelCase")]
    FreeformCartesian2D {
        blend_parameter: String,
        blend_parameter_y: String,
        children: Vec<Child2D>,
    },
    #[serde(rename_all = "camelCase")]
    Direct {
        normalized_blend_values: bool,
        children: Vec<ChildDirect>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Child1D {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion: Option<Motion>,
    pub threshold: f32,
    pub time_scale: f32,
    pub cycle_offset: f32,
    pub mirror: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Child2D {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion: Option<Motion>,
    pub position: Vec2,
    pub time_scale: f32,
    pub cycle_offset: f32,
    pub mirror: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChildDirect {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion: Option<Motion>,
    pub direct_blend_parameter: String,
    pub time_scale: f32,
    pub cycle_offset: f32,
    pub mirror: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum Behaviour {
    #[serde(rename_all = "camelCase")]
    ParameterDriver {
        local_only: bool,
        parameters: Vec<DriverEntry>,
    },
    #[serde(rename_all = "camelCase")]
    TrackingControl {
        head: TrackingType,
        left_hand: TrackingType,
        right_hand: TrackingType,
        hip: TrackingType,
        left_foot: TrackingType,
        right_foot: TrackingType,
        left_fingers: TrackingType,
        right_fingers: TrackingType,
        eyes: TrackingType,
        mouth: TrackingType,
    },
    #[serde(rename_all = "camelCase")]
    LocomotionControl { disable_locomotion: bool },
    #[serde(rename_all = "camelCase")]
    PlayableLayerControl {
        layer: ControlledLayer,
        goal_weight: f32,
        blend_duration: f32,
    },
    #[serde(rename_all = "camelCase")]
    AnimatorLayerControl {
        playable: ControlledLayer,
        layer: u32,
        goal_weight: f32,
        blend_duration: f32,
    },
    #[serde(rename_all = "camelCase")]
    TemporaryPoseSpace {
        enter_pose_space: bool,
        fixed_delay: bool,
        delay_time: f32,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum DriverEntry {
    Set {
        name: String,
        value: f32,
    },
    Add {
        name: String,
        value: f32,
    },
    /// A bool destination uses `chance`; int and float destinations draw from `min..=max`.
    #[serde(rename_all = "camelCase")]
    Random {
        name: String,
        value_min: f32,
        value_max: f32,
        chance: f32,
    },
    #[serde(rename_all = "camelCase")]
    Copy {
        source: String,
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        convert_range: Option<ConvertRange>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConvertRange {
    pub source_min: f32,
    pub source_max: f32,
    pub dest_min: f32,
    pub dest_max: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum TrackingType {
    NoChange,
    Tracking,
    Animation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ControlledLayer {
    Action,
    #[serde(rename = "fx")]
    Fx,
    Gesture,
    Additive,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Clip {
    pub name: String,
    /// Seconds.
    pub length: f32,
    #[serde(rename = "loop")]
    pub looping: bool,
    pub curves: Vec<Curve>,
    pub slot_curves: Vec<SlotCurve>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Curve {
    pub binding: Binding,
    pub curve: AnimationCurve,
}

/// A material swap: which material fills a renderer's slot over time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SlotCurve {
    pub renderer: NodeIndex,
    pub slot: u32,
    pub keys: Vec<SlotKey>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SlotKey {
    pub time: f32,
    pub material: MaterialIndex,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum Binding {
    NodeActive {
        node: NodeIndex,
    },
    RendererEnabled {
        node: NodeIndex,
    },
    BlendShape {
        node: NodeIndex,
        name: String,
    },
    /// `property` is the shader property, with `.r`/`.x`-style component suffixes as Unity binds them.
    MaterialProperty {
        node: NodeIndex,
        property: String,
    },
    Transform {
        node: NodeIndex,
        property: TransformProperty,
        component: Axis,
    },
    /// Unity's humanoid muscle or root curve name.
    Muscle {
        name: String,
    },
    AnimatorParameter {
        name: String,
    },
    PhysBone {
        index: PhysBoneIndex,
        field: String,
    },
    Constraint {
        index: ConstraintIndex,
        field: String,
    },
    ContactSender {
        index: ContactIndex,
        field: String,
    },
    ContactReceiver {
        index: ContactIndex,
        field: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum TransformProperty {
    Translation,
    Rotation,
    EulerRotation,
    Scale,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Axis {
    X,
    Y,
    Z,
    W,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Unsupported {
    pub kind: String,
    pub count: u32,
}

fn unique<'de, D, T>(deserializer: D) -> Result<BTreeSet<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + Ord,
{
    let items = Vec::<T>::deserialize(deserializer)?;
    let count = items.len();
    let set = BTreeSet::from_iter(items);
    if set.len() != count {
        return Err(serde::de::Error::custom("duplicate entries in a set"));
    }
    Ok(set)
}
