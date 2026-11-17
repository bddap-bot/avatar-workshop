//! VRChat's playable layers and parameters: Unity's state machines, blend trees and clip sampling, the
//! expression and built-in parameters, and the state behaviours, run as VRChat runs them.

mod face;
mod machine;
pub mod human;
pub mod motion;
mod random;

use std::collections::{HashMap, HashSet};


use crate::model::{
    Axis, Behaviour, BlendAlgorithm, BodyPart, ControlledLayer, DriverEntry, LayerBlending, MaterialProperty, Motion,
    ParameterValue, PlayableLayerType, TrackingType,
};
use crate::Avatar;

pub use machine::Kind;
use machine::{ControllerRt, Entered, From};
use motion::{axis_index, CompiledClip, Leaf, Target, Targets};
pub use random::UnityRandom;

/// VRChat's built-in parameters with the value each holds when the host gives none.
pub const BUILT_INS: &[(&str, Kind, f32)] = &[
    ("IsLocal", Kind::Bool, 1.0),
    ("Viseme", Kind::Int, 0.0),
    ("Voice", Kind::Float, 0.0),
    ("GestureLeft", Kind::Int, 0.0),
    ("GestureRight", Kind::Int, 0.0),
    ("GestureLeftWeight", Kind::Float, 0.0),
    ("GestureRightWeight", Kind::Float, 0.0),
    ("AngularY", Kind::Float, 0.0),
    ("VelocityX", Kind::Float, 0.0),
    ("VelocityY", Kind::Float, 0.0),
    ("VelocityZ", Kind::Float, 0.0),
    ("VelocityMagnitude", Kind::Float, 0.0),
    ("Upright", Kind::Float, 1.0),
    ("Grounded", Kind::Bool, 1.0),
    ("Seated", Kind::Bool, 0.0),
    ("AFK", Kind::Bool, 0.0),
    ("TrackingType", Kind::Int, 3.0),
    ("VRMode", Kind::Int, 0.0),
    ("MuteSelf", Kind::Bool, 0.0),
    ("InStation", Kind::Bool, 0.0),
    ("Earmuffs", Kind::Bool, 0.0),
    ("IsAnimatorEnabled", Kind::Bool, 1.0),
    ("PreviewMode", Kind::Int, 0.0),
    ("ScaleModified", Kind::Bool, 0.0),
    ("ScaleFactor", Kind::Float, 1.0),
    ("ScaleFactorInverse", Kind::Float, 1.0),
    ("EyeHeightAsMeters", Kind::Float, 0.0),
    ("EyeHeightAsPercent", Kind::Float, 0.0),
    ("IsOnFriendsList", Kind::Bool, 0.0),
    ("AvatarVersion", Kind::Int, 3.0),
];

/// The playable layers in the order VRChat stacks them.
pub const PLAYABLE_ORDER: [PlayableLayerType; 8] = [
    PlayableLayerType::Base,
    PlayableLayerType::Additive,
    PlayableLayerType::Sitting,
    PlayableLayerType::TPose,
    PlayableLayerType::IkPose,
    PlayableLayerType::Gesture,
    PlayableLayerType::Action,
    PlayableLayerType::Fx,
];

/// A parameter as the avatar holds it: an expression parameter, or one only a controller declares.
#[derive(Clone, Debug)]
pub struct Parameter {
    pub name: String,
    pub kind: Kind,
    pub value: f32,
    pub synced: bool,
    pub saved: bool,
    pub expression: bool,
    last: Option<f32>,
}

struct Blend {
    start: f32,
    goal: f32,
    duration: f32,
    began: f64,
}

impl Blend {
    fn at(&self, now: f64) -> (f32, bool) {
        if self.duration <= 0.0 {
            return (self.goal, true);
        }
        let amt = ((now - self.began) as f32) / self.duration;
        if amt >= 1.0 {
            return (self.goal, true);
        }
        (self.start + (self.goal - self.start) * amt.clamp(0.0, 1.0), false)
    }
}

struct Playable {
    /// What each layer last wrote of the human pose.
    layer_human: Vec<HashMap<u32, f32>>,
    /// What each layer last said, animator parameters included, with each target's cover, which a state without write
    /// defaults and without a motion repeats; a state without a motion that writes defaults clears it. An override
    /// layer's is its pose while a state with a motion played; an additive layer's the offsets it added on its last
    /// frame outside such a state.
    said: Vec<Vec<(u32, f32, f32)>>,
    kind: PlayableLayerType,
    rt: ControllerRt,
    weight: f32,
    blend: Option<Blend>,
    layer_blends: HashMap<usize, Blend>,
    /// Muscles the mask lets through; None passes everything.
    muscles: Option<Vec<BodyPart>>,
    bound: Vec<u32>,
    /// How many of the controller's layers bind each target.
    binders: Vec<u8>,
    /// Per layer, whether a clip it plays drives an animator parameter.
    drives_parameters: Vec<bool>,
    /// Per layer, the states that give the human pose: those whose motion holds a humanoid clip, at any weight, and
    /// those with no motion, which hold what the layer last wrote. None of a layer playing no humanoid clip.
    poses: Vec<std::collections::HashSet<crate::model::StateId>>,
    /// Per layer, every slot of each renderer it binds a slot of: Unity writes a renderer's slots together.
    slots: Vec<Vec<u32>>,
    /// Per layer, whether it has played a motion.
    played: Vec<bool>,
    /// Without a humanoid rig, what a state without write defaults holds each target at: a node's position at the
    /// controller's last output, any other property at what its override layers last wrote, from the defaults.
    held: Vec<f32>,
    /// Whether one of its layers has entered a state with a motion or a transition, at some weight.
    sounded: bool,
    /// The targets an additive layer of this controller adds to: those it binds, and every generic property another
    /// controller binds that no later playable does, which would write over it. Without a humanoid rig, every generic
    /// property any controller binds, which its states without write defaults also hold.
    reach: Vec<u32>,
    layer_masks: Vec<Option<Mask>>,
    /// The humanoid targets the controller binds that its masks admit.
    human: Vec<u32>,
    /// Per target, whether it is one of `human`.
    whole: Vec<bool>,
}

#[derive(Clone)]
struct Mask {
    parts: Vec<BodyPart>,
    nodes: Option<HashMap<u32, bool>>,
}

impl Accum {
    fn at_defaults(cov: &[f32], defaults: &[f32]) -> Accum {
        let n = cov.len();
        Accum { sum: cov.iter().zip(defaults).map(|(c, d)| c * d).collect(), cov: cov.to_vec(), touched: vec![false; n], wrote: vec![false; n], overridden: vec![None; n] }
    }
}

struct Accum {
    sum: Vec<f32>,
    cov: Vec<f32>,
    touched: Vec<bool>,
    /// Whether a layer wrote the target this frame, with some weight.
    wrote: Vec<bool>,
    /// The value after the last override layer whose pose wrote the target this frame.
    overridden: Vec<Option<f32>>,
}

/// The parts a tracking-control behaviour sets, in the order of its fields.
pub const TRACKED_PARTS: [&str; 10] =
    ["head", "leftHand", "rightHand", "hip", "leftFoot", "rightFoot", "leftFingers", "rightFingers", "eyes", "mouth"];
const EYES: usize = 8;
pub const MOUTH: usize = 9;

/// A temporary pose space waiting out its delays: the SDK's, then the emulator's, which waits `delay_time` seconds
/// again from the frame the SDK applies it. The SDK measures a fixed delay on Unity's clock rounded to single precision,
/// the emulator on the clock itself, so the same delay entered on different frames can take one frame more.
struct PoseSpace {
    enter: bool,
    playable: usize,
    layer: usize,
    state: crate::model::StateId,
    sdk_wait: SdkWait,
    delay: f32,
    applied: Option<f64>,
}

enum SdkWait {
    Since(f32),
    Normalized { seconds: f32, waited: f32 },
}

pub struct Animator {
    targets: Targets,
    /// On a humanoid rig, what the frame's result last was for each target some layer wrote: a controller's first
    /// layer playing a motion without write defaults gives it to every target no layer up to it wrote. A
    /// write-defaults state's target counts as written only when a leaf at weight writes it, so a value such a state
    /// fills in for its unweighted leaves is not kept, as the blend duration probe's first tree without write
    /// defaults shows. Without a humanoid rig each playable holds its own (`Playable::held`), fed by every write: the
    /// generic-rig hold probes' held positions follow writes no leaf at weight made.
    kept: Vec<f32>,
    /// Whether some clip writes the target other than at its rest value beside an animated component.
    curved: Vec<bool>,
    /// What tracking-control behaviours last set for each of [`TRACKED_PARTS`]; NoChange until one does.
    pub tracking: [TrackingType; 10],
    pub locomotion_disabled: bool,
    /// The view position in the avatar's space, moved by temporary pose spaces.
    pub view: glam::Vec3,
    default_view: glam::Vec3,
    /// The avatar root's scale, and the view position in metres in the head bone's rest space: the host gives the
    /// head's pose in metres from the root.
    head_view: Option<(glam::Vec3, glam::Vec3)>,
    /// The head bone's pose in the avatar's space after this frame's animation, from the host.
    head: Option<crate::physbones::Transform>,
    pose_spaces: Vec<PoseSpace>,
    /// Muscles of bones the avatar lacks, which Unity's human stream holds at rest.
    boneless: Vec<u32>,
    /// The rotation components a crossfade blends as quaternions, negating a destination in the other hemisphere from
    /// its source as Unity does: first the body's and IK goals', which Unity's human pose holds as unit quaternions,
    /// then each node's.
    rotations: Vec<[u32; 4]>,
    /// How many of `rotations` are the body's and IK goals'.
    human_rotations: usize,
    /// The body's targets (position, then rotation), and each IK goal's with its rest from the body: the animator
    /// blends a goal as the body it follows, and Unity's human stream holds it at its rest from there.
    body: Option<[u32; 7]>,
    /// Without a humanoid rig no playable carries a human pose, and each controller runs its own stream.
    humanoid: bool,
    /// The root motion's targets, a transform like the body's.
    motion: Option<[u32; 7]>,
    goals: Vec<([u32; 7], glam::Vec3, glam::Quat)>,
    /// Every target as Unity's stream holds it.
    values: Vec<f32>,
    clips: Vec<CompiledClip>,
    defaults: Vec<f32>,
    /// Every target as the layers blend it: an IK goal before its rest offset.
    blended: Vec<f32>,
    playables: Vec<Playable>,
    parameters: Vec<Parameter>,
    index: HashMap<String, usize>,
    behaviours: Vec<Behaviour>,
    /// The built-ins as the host last set them, and as normalized last frame.
    builtins: HashMap<String, f32>,
    normalized: HashMap<String, f32>,
    old_weights: [f32; 2],
    prev_seated: bool,
    time: f64,
    random: UnityRandom,
    /// Values the host's PhysBones and contacts gave this frame, entering the parameters for the next.
    dynamics: Vec<(String, f32)>,
    face: face::Face,
    /// Lip sync's and the eyelids' shapes no controller binds, which keep their last value once handed to animation.
    unbound_face: (Vec<u32>, Vec<u32>),
    blink: f32,
    look_y: f32,
}

pub struct LayerState {
    pub playable: usize,
    pub layer: usize,
    pub name: String,
    pub current: String,
    pub current_time: f32,
    pub next: Option<String>,
    pub next_time: Option<f32>,
}

/// A target's value as the avatar was authored, which it keeps while nothing animates it.
pub fn authored(avatar: &Avatar, t: &Target) -> f32 {
    let rest = crate::physbones::Nodes::read(avatar).map(|n| n.rest).unwrap_or_default();
    default_of(avatar, t, &rest)
}

/// Each target's value as the avatar was authored.
pub fn authored_all(avatar: &Avatar, targets: &Targets) -> Vec<f32> {
    let rest = crate::physbones::Nodes::read(avatar).map(|n| n.rest).unwrap_or_default();
    targets.list.iter().map(|t| default_of(avatar, t, &rest)).collect()
}

fn default_of(avatar: &Avatar, t: &Target, rest: &[crate::physbones::Transform]) -> f32 {
    let m = &avatar.model;
    match t {
        Target::NodeActive(n) => m.nodes.get(*n as usize).map_or(1.0, |x| x.active as u8 as f32),
        Target::RendererEnabled(n) => m.renderers.iter().find(|r| r.node.0 == *n).map_or(1.0, |r| r.enabled as u8 as f32),
        Target::BlendShape(n, name) => {
            let node = &avatar.gltf["nodes"][*n as usize];
            let Some(mesh) = node["mesh"].as_u64() else { return 0.0 };
            let names = &avatar.gltf["meshes"][mesh as usize]["extras"]["targetNames"];
            let Some(i) = names.as_array().and_then(|a| a.iter().position(|x| x.as_str() == Some(name))) else { return 0.0 };
            let weights = if node["weights"].is_array() { &node["weights"] } else { &avatar.gltf["meshes"][mesh as usize]["weights"] };
            weights[i].as_f64().unwrap_or(0.0) as f32 * 100.0
        }
        Target::MaterialProperty(n, prop) => {
            let (base, comp) = match prop.rsplit_once('.') {
                Some((b, c)) if c.len() == 1 && "rgbaxyzw".contains(c) => (b, Some(c)),
                _ => (prop.as_str(), None),
            };
            let Some(r) = m.renderers.iter().find(|r| r.node.0 == *n) else { return 0.0 };
            let Some(p) = r.materials.iter().filter_map(|i| m.materials.get(i.0 as usize)).find_map(|x| x.declared.get(base)) else { return 0.0 };
            let i = comp.map(|c| "rgbaxyzw".find(c).unwrap() % 4);
            match (p, i) {
                (MaterialProperty::Float(v), _) => *v,
                (MaterialProperty::Int(v), _) => *v as f32,
                (MaterialProperty::Color(v) | MaterialProperty::Vector(v), Some(i)) => v[i],
                (MaterialProperty::Color(v) | MaterialProperty::Vector(v), None) => v[0],
                (MaterialProperty::Texture(_), _) => 0.0,
            }
        }
        Target::Translation(n, a) => rest.get(*n as usize).map_or(0.0, |r| r.translation[axis_index(*a)]),
        Target::Rotation(n, a) => rest.get(*n as usize).map_or(0.0, |r| {
            let q = r.rotation;
            [q.x, q.y, q.z, q.w][axis_index(*a)]
        }),
        Target::Scale(n, a) => rest.get(*n as usize).map_or(1.0, |r| r.scale[axis_index(*a)]),
        Target::Slot(n, s) => m
            .renderers
            .iter()
            .find(|r| r.node.0 == *n)
            .and_then(|r| r.materials.get(*s as usize))
            .map_or(-1.0, |x| x.0 as f32),
        Target::PhysBone(i, f) => m.physbones.get(*i as usize).map_or(0.0, |p| {
            let permission = |x: &crate::model::Permission| match (x.allow_self, x.allow_others) {
                (false, false) => 0.0,
                (true, true) => 1.0,
                _ => 2.0,
            };
            match f.as_str() {
                "m_Enabled" => p.enabled as u8 as f32,
                "allowCollision" => permission(&p.collision),
                "allowGrabbing" => permission(&p.grabbing),
                "allowPosing" => permission(&p.posing),
                _ => 0.0,
            }
        }),
        Target::Muscle(n) if human::is_goal_w(n) => 1.0,
        Target::Constraint(i, f) => m.constraints.get(*i as usize).map_or(0.0, |c| constraint_field(c, f)),
        Target::ContactSender(i, f) => m.contacts.senders.get(*i as usize).map_or(0.0, |c| match f.as_str() {
            "m_Enabled" => c.enabled as u8 as f32,
            _ => 0.0,
        }),
        Target::ContactReceiver(i, f) => m.contacts.receivers.get(*i as usize).map_or(0.0, |c| match f.as_str() {
            "m_Enabled" => c.enabled as u8 as f32,
            "allowSelf" => c.allow_self as u8 as f32,
            "allowOthers" => c.allow_others as u8 as f32,
            "localOnly" => c.local_only as u8 as f32,
            "minVelocity" => match c.receiver_type {
                crate::model::ReceiverType::OnEnter { min_velocity } => min_velocity,
                _ => 0.0,
            },
            _ => 0.0,
        }),
        _ => 0.0,
    }
}

/// A constraint field's authored value, by the name a curve binds it with, signed as the model holds it.
fn constraint_field(c: &crate::model::Constraint, field: &str) -> f32 {
    use crate::model::ConstraintKind::*;
    let (name, axis) = match field.rsplit_once('.') {
        Some((n, a)) if a.len() == 1 && "xyz".contains(a) => (n, Some("xyz".find(a).unwrap())),
        _ => (field, None),
    };
    if let Some(rest) = name.strip_prefix("Sources.source") {
        let Some((k, f)) = rest.split_once('.') else { return 0.0 };
        let Some(src) = k.parse::<usize>().ok().and_then(|k| c.sources.get(k)) else { return 0.0 };
        return match (f, axis) {
            ("Weight", None) => src.weight,
            ("ParentPositionOffset", Some(a)) => src.position_offset[a],
            ("ParentRotationOffset", Some(a)) => src.rotation_offset[a],
            _ => 0.0,
        };
    }
    let channel = |which: &str| match (&c.kind, which) {
        (Parent { position, .. } | Position { position }, "Position") => Some(position),
        (Parent { rotation, .. } | Rotation { rotation } | Aim { rotation, .. } | LookAt { rotation, .. }, "Rotation") => Some(rotation),
        (Scale { scale }, "Scale") => Some(scale),
        _ => None,
    };
    match (name, axis) {
        ("m_Enabled", None) => c.enabled as u8 as f32,
        ("IsActive", None) => c.is_active as u8 as f32,
        ("GlobalWeight", None) => c.weight,
        ("FreezeToWorld", None) => c.freeze_to_world as u8 as f32,
        ("SolveInLocalSpace", None) => c.solve_in_local_space as u8 as f32,
        (n, Some(a)) => {
            let (which, part) = ["Offset", "AtRest"].iter().find_map(|p| n.strip_suffix(p).map(|w| (w, *p))).unwrap_or((n, ""));
            match (channel(which), part) {
                (Some(ch), "Offset") => ch.offset[a],
                (Some(ch), "AtRest") => ch.at_rest[a],
                _ => 0.0,
            }
        }
        _ => 0.0,
    }
}

fn controlled(l: ControlledLayer) -> PlayableLayerType {
    match l {
        ControlledLayer::Action => PlayableLayerType::Action,
        ControlledLayer::Fx => PlayableLayerType::Fx,
        ControlledLayer::Gesture => PlayableLayerType::Gesture,
        ControlledLayer::Additive => PlayableLayerType::Additive,
    }
}

/// The model's clips as the animator samples them, in the model's order, and the targets they write.
pub fn compile_clips(avatar: &Avatar) -> (Targets, Vec<CompiledClip>) {
    let m = &avatar.model;
    let mut targets = Targets::default();
    let mut clips: Vec<CompiledClip> = m.clips.iter().map(|c| CompiledClip::compile(c, &mut targets)).collect();
    let muscle_targets: Vec<u32> =
        (0..targets.len() as u32).filter(|&t| matches!(&targets.list[t as usize], Target::Muscle(n) if human::goal(n).is_none())).collect();
    for c in &mut clips {
        c.complete_human_pose(&muscle_targets, &targets, m.humanoid.rest.as_ref());
    }
    for i in 0..clips.len() {
        let Some((r, _)) = clips[i].reference else { continue };
        let referenced = clips[r as usize].writes.clone();
        let unreferenced = |v: &CompiledClip| -> Vec<(u32, f32)> {
            v.writes.iter().filter(|t| targets.list[**t as usize].generic() && !v.fills(**t) && referenced.binary_search(t).is_err()).map(|&t| (t, targets.list[t as usize].identity())).collect()
        };
        let c = &mut clips[i];
        c.unreferenced = unreferenced(c);
        if let Some(m) = c.mirrored.as_deref_mut() {
            m.unreferenced = unreferenced(m);
        }
    }
    (targets, clips)
}

impl Animator {
    pub fn new(avatar: &Avatar) -> Animator {
        let m = &avatar.model;
        let humanoid = !m.humanoid.bones.is_empty();
        let (mut targets, clips) = compile_clips(avatar);
        let face = face::Face::new(&m.descriptor, &mut targets);
        let anim_params = |c: u32| -> Vec<String> {
            clips[c as usize]
                .writes
                .iter()
                .filter_map(|&t| match &targets.list[t as usize] {
                    Target::AnimatorParameter(n) => Some(n.clone()),
                    _ => None,
                })
                .collect()
        };
        let defaults = authored_all(avatar, &targets);
        let mut playables = Vec::new();
        for kind in PLAYABLE_ORDER {
            let Some(layer) = m.layers.get(&kind) else { continue };
            let Some(c) = &layer.controller else { continue };
            let rt = ControllerRt::new(c, &clips, &anim_params);
            let mut bound: Vec<u32> = rt.clip_ids().iter().flat_map(|&c| clips[c as usize].bindings()).collect();
            bound.sort_unstable();
            bound.dedup();
            let slots = (0..c.layers.len())
                .map(|li| {
                    let renderers: Vec<u32> = rt
                        .layer_clip_ids(li)
                        .iter()
                        .flat_map(|&c| clips[c as usize].writes.iter())
                        .filter_map(|&t| if let Target::Slot(n, _) = targets.list[t as usize] { Some(n) } else { None })
                        .collect();
                    (0..targets.len() as u32).filter(|&t| matches!(targets.list[t as usize], Target::Slot(n, _) if renderers.contains(&n))).collect()
                })
                .collect();
            let muscles = layer.mask.as_ref().map(|mk| mk.body_parts.iter().copied().collect());
            let layer_masks: Vec<Option<Mask>> = c
                .layers
                .iter()
                .map(|l| {
                    l.mask.as_ref().map(|mk| Mask {
                        parts: mk.body_parts.iter().copied().collect(),
                        nodes: (!mk.transforms.is_empty()).then(|| mk.transforms.iter().map(|t| (t.node.0, t.active)).collect()),
                    })
                })
                .collect();
            let weight = match kind {
                PlayableLayerType::Action | PlayableLayerType::Sitting | PlayableLayerType::TPose | PlayableLayerType::IkPose => 0.0,
                _ => 1.0,
            };
            let mut binders = vec![0u8; targets.len()];
            let mut drives_parameters = Vec::new();
            let mut poses = Vec::new();
            for li in 0..c.layers.len() {
                let mut ts: Vec<u32> = rt.layer_clip_ids(li).iter().flat_map(|&c| clips[c as usize].bindings()).collect();
                drives_parameters.push(ts.iter().any(|&t| matches!(targets.list[t as usize], Target::AnimatorParameter(_))));
                let human = |m: &Motion| {
                    let mut ids = Vec::new();
                    machine::clip_ids(m, &mut ids);
                    ids.iter().any(|&c| clips[c as usize].is_human(&targets))
                };
                let states = rt.state_ids(li).into_iter().map(|s| (s, rt.motion(li, s)));
                let mut posing: std::collections::HashSet<_> = states.filter(|(_, m)| m.as_ref().is_none_or(human)).map(|(s, _)| s).collect();
                if !rt.layer_clip_ids(li).iter().any(|&c| clips[c as usize].is_human(&targets)) {
                    posing.clear();
                }
                poses.push(posing);
                ts.sort_unstable();
                ts.dedup();
                for t in ts {
                    binders[t as usize] = binders[t as usize].saturating_add(1);
                }
            }
            let writable = |part: BodyPart| {
                muscles.as_ref().is_none_or(|ps: &Vec<BodyPart>| ps.contains(&part))
                    && c.layers.iter().any(|l| l.mask.as_ref().is_none_or(|mk| mk.body_parts.contains(&part)))
            };
            let human = bound
                .iter()
                .filter(|&&t| match &targets.list[t as usize] {
                    Target::Muscle(n) => writable(human::part(n)),
                    _ => false,
                })
                .copied()
                .collect::<Vec<_>>();
            let mut whole = vec![false; targets.len()];
            human.iter().for_each(|&t| whole[t as usize] = true);
            playables.push(Playable {
                kind,
                human,
                whole,
                rt,
                binders,
                drives_parameters,
                poses,
                weight,
                blend: None,
                layer_blends: HashMap::new(),
                muscles,
                bound,
                slots,
                played: vec![false; c.layers.len()],
                held: defaults.clone(),
                sounded: false,
                reach: Vec::new(),
                layer_human: vec![HashMap::new(); c.layers.len()],
                said: vec![Vec::new(); c.layers.len()],
                layer_masks,
            });
        }

        let mut parameters = Vec::new();
        let mut index = HashMap::new();
        for p in &m.parameters {
            if p.name.is_empty() || index.contains_key(&p.name) {
                continue;
            }
            let (kind, value) = match p.default {
                ParameterValue::Int(v) => (Kind::Int, v as f32),
                ParameterValue::Float(v) => (Kind::Float, v),
                ParameterValue::Bool(v) => (Kind::Bool, v as u8 as f32),
            };
            index.insert(p.name.clone(), parameters.len());
            parameters.push(Parameter { name: p.name.clone(), kind, value, synced: p.synced, saved: p.saved, expression: true, last: None });
        }
        for pl in &playables {
            for p in &pl.rt.params {
                if index.contains_key(&p.name) {
                    continue;
                }
                let kind = if p.kind == Kind::Trigger { Kind::Bool } else { p.kind };
                index.insert(p.name.clone(), parameters.len());
                parameters.push(Parameter {
                    name: p.name.clone(),
                    kind,
                    value: p.value,
                    synced: false,
                    saved: false,
                    expression: false,
                    last: Some(p.value),
                });
            }
        }
        let builtins: HashMap<String, f32> = BUILT_INS.iter().map(|(n, _, v)| (n.to_string(), *v)).collect();
        let view = m.descriptor.view_position;
        let view = glam::Vec3::new(view[0], view[1], view[2]);
        let head_view = m.humanoid.bones.get(&crate::model::HumanBodyBone::Head).and_then(|b| {
            let nodes = crate::physbones::Nodes::read(avatar).ok()?;
            let mut chain = vec![b.node.0 as usize];
            while let Some(p) = nodes.parent[*chain.last().unwrap()] {
                chain.push(p);
            }
            let root = nodes.rest[chain.pop()?];
            let rest = chain.iter().rev().fold(crate::physbones::Transform::IDENTITY, |t, &n| t.mul(&nodes.rest[n]));
            let s = root.scale;
            let rest_m = crate::physbones::Transform { translation: s * rest.translation, ..rest };
            Some((s, rest_m.rotation.inverse() * (s * view - rest_m.translation)))
        });
        for pi in 0..playables.len() {
            let bound_by = |ps: &[Playable], t: &u32| ps.iter().any(|p| p.bound.binary_search(t).is_ok());
            let mut reach: Vec<u32> = playables[pi].bound.clone();
            if humanoid {
                reach.extend(playables[..pi].iter().flat_map(|p| p.bound.iter().copied()).filter(|t| targets.list[*t as usize].generic() && !bound_by(&playables[pi..], t)));
            } else {
                reach.extend(playables.iter().flat_map(|p| p.bound.iter().copied()).filter(|t| targets.list[*t as usize].generic()));
            }
            reach.sort_unstable();
            reach.dedup();
            playables[pi].reach = reach;
        }
        let mut curved = vec![false; targets.list.len()];
        for c in clips.iter().flat_map(|c| std::iter::once(c).chain(c.mirrored.as_deref())) {
            for &t in c.writes.iter().filter(|&&t| !c.rests(t)) {
                curved[t as usize] = true;
            }
        }
        let mut rotations = human::GOALS
            .iter()
            .filter_map(|(g, _)| {
                let id = |c: &str| targets.index_of(&Target::Muscle(format!("{g}Q.{c}"))).map(|i| i as u32);
                Some([id("x")?, id("y")?, id("z")?, id("w")?])
            })
            .collect::<Vec<_>>();
        let human_rotations = rotations.len();
        rotations.extend((0..targets.len() as u32).filter_map(|t| {
            let &Target::Rotation(n, Axis::X) = &targets.list[t as usize] else { return None };
            let id = |a: Axis| targets.index_of(&Target::Rotation(n, a)).map(|i| i as u32);
            Some([t, id(Axis::Y)?, id(Axis::Z)?, id(Axis::W)?])
        }));
        let boneless = (0..targets.len() as u32)
            .filter(|&t| matches!(&targets.list[t as usize], Target::Muscle(n) if human::bone(n).is_some_and(|b| !m.humanoid.bones.contains_key(&b))))
            .collect();
        let ids = |g: &str| motion::goal_ids(&targets, g);
        let rests = motion::goal_rests(m.humanoid.rest.as_ref());
        let goals = human::GOALS.iter().filter(|(_, p)| *p != crate::model::BodyPart::Root).zip(rests).filter_map(|((g, _), (t, q))| Some((ids(g)?, t, q))).collect();
        let unbound = |v: Vec<u32>| -> Vec<u32> { v.into_iter().filter(|t| !playables.iter().any(|p| p.bound.binary_search(t).is_ok())).collect() };
        let (mouth, lids) = face.shapes();
        let unbound_face = (unbound(mouth), unbound(lids));
        let mut a = Animator {
            body: ids("Root"),
            humanoid,
            motion: ids("Motion"),
            goals,
            values: Vec::new(),
            boneless,
            rotations,
            human_rotations,
            kept: defaults.clone(),
            curved,
            tracking: [TrackingType::NoChange; 10],
            locomotion_disabled: false,
            view,
            default_view: view,
            head_view,
            head: None,
            pose_spaces: Vec::new(),
            blended: defaults.clone(),
            defaults,
            targets,
            clips,
            playables,
            parameters,
            index,
            behaviours: m.behaviours.clone(),
            normalized: builtins.clone(),
            builtins,
            old_weights: [0.0; 2],
            prev_seated: false,
            time: 0.0,
            random: UnityRandom::new(1),
            dynamics: Vec::new(),
            face,
            unbound_face,
            blink: 0.0,
            look_y: 0.0,
        };
        a.set_eye_height(avatar);
        a.place_goals();
        a
    }

    fn set_eye_height(&mut self, avatar: &Avatar) {
        let y = avatar.model.descriptor.view_position[1];
        self.builtins.insert("EyeHeightAsMeters".into(), y);
        let percent = (y as f64 - 0.2f32 as f64) / (5.0f32 - 0.2f32) as f64;
        self.builtins.insert("EyeHeightAsPercent".into(), (percent as f32).clamp(0.0, 1.0));
        self.normalized = self.builtins.clone();
    }

    /// What a state without write defaults in playable `pi` holds a target at.
    fn hold(&self, pi: usize, i: usize) -> f32 {
        if self.humanoid { self.kept[i] } else { self.playables[pi].held[i] }
    }

    pub fn targets(&self) -> &[Target] {
        &self.targets.list
    }

    pub fn values(&self) -> &[f32] {
        &self.values
    }

    fn place_goals(&mut self) {
        self.values.clone_from(&self.blended);
        let v = &self.blended;
        let read = |ids: &[u32; 7]| {
            let at = |i: usize| v[ids[i] as usize];
            (glam::Vec3::new(at(0), at(1), at(2)), glam::Quat::from_xyzw(at(3), at(4), at(5), at(6)))
        };
        for (ids, offset, turn) in &self.goals {
            let (t, q) = read(ids);
            let (gt, gq) = (t + q * *offset, q * *turn);
            for (id, x) in ids.iter().zip([gt.x, gt.y, gt.z, gq.x, gq.y, gq.z, gq.w]) {
                self.values[*id as usize] = x;
            }
        }
    }

    pub fn defaults(&self) -> &[f32] {
        &self.defaults
    }

    /// Sets a target's default as the host bound it, for a host that binds the animator after the avatar's serialized
    /// value has changed.
    pub fn bind_default(&mut self, target: &Target, value: f32) {
        if let Some(i) = self.targets.index_of(target) {
            self.defaults[i] = value;
            self.blended[i] = value;
            self.kept[i] = value;
            self.playables.iter_mut().for_each(|p| p.held[i] = value);
            self.place_goals();
        }
    }

    /// The head bone's pose in the avatar's space after the last frame's animation and IK, for this step: a
    /// temporary pose space takes the view position from it.
    pub fn set_head(&mut self, head: crate::physbones::Transform) {
        self.head = Some(head);
    }

    /// The eyelids' input: a blink from 0 (open) to 1 (shut) and the eyes' vertical look from -1 to 1.
    pub fn set_eyes(&mut self, blink: f32, look_y: f32) {
        self.blink = blink;
        self.look_y = look_y;
    }

    /// Sets a built-in parameter from the host's input; false for a name that is not one of [`BUILT_INS`].
    pub fn set_builtin(&mut self, name: &str, value: f32) -> bool {
        let Some(v) = self.builtins.get_mut(name) else { return false };
        *v = value;
        if matches!(name, "VelocityX" | "VelocityY" | "VelocityZ") {
            let b = &self.builtins;
            let m = (b["VelocityX"].powi(2) + b["VelocityY"].powi(2) + b["VelocityZ"].powi(2)).sqrt();
            self.builtins.insert("VelocityMagnitude".into(), m);
        }
        true
    }

    /// The avatar's parameters: its expression parameters, then those only its controllers declare.
    pub fn parameters(&self) -> &[Parameter] {
        &self.parameters
    }

    /// The saved expression parameters' values, for the host to keep and restore when the avatar loads again.
    pub fn saved(&self) -> Vec<(String, f32)> {
        self.parameters.iter().filter(|p| p.saved).map(|p| (p.name.clone(), p.value)).collect()
    }

    /// Restores values `saved` gave; names that are not saved parameters are ignored.
    pub fn restore(&mut self, saved: &[(String, f32)]) {
        for (name, value) in saved {
            if self.index.get(name).is_some_and(|&i| self.parameters[i].saved) {
                self.set(name, *value);
            }
        }
    }

    /// Sets an expression parameter, as a menu control does.
    pub fn set(&mut self, name: &str, value: f32) {
        if let Some(&i) = self.index.get(name) {
            let p = &mut self.parameters[i];
            p.value = match p.kind {
                Kind::Int => value.trunc(),
                Kind::Bool | Kind::Trigger => (value != 0.0) as u8 as f32,
                Kind::Float => value,
            };
        }
    }

    pub fn get(&self, name: &str) -> Option<f32> {
        self.index.get(name).map(|&i| self.parameters[i].value)
    }

    /// A PhysBone or contact parameter the host's dynamics produced this frame.
    pub fn set_dynamic(&mut self, name: &str, value: f32) {
        self.dynamics.push((name.to_string(), value));
    }

    /// Each playable's controller layers' states this frame, with their playable and layer indices.
    pub fn states(&self) -> Vec<LayerState> {
        let mut out = Vec::new();
        for (pi, p) in self.playables.iter().enumerate() {
            for (li, l) in p.rt.layers.iter().enumerate() {
                let name = |s| p.rt.names.get(&s).cloned().unwrap_or_default();
                let held = l.current.as_ref();
                let (current, current_time, next, next_time) = match &l.active {
                    Some(a) => {
                        let (cur, t) = match &a.from {
                            From::State(s) => (name(s.state), s.time),
                            From::Frozen(_) => (held.map(|c| name(c.state)).unwrap_or_default(), held.map_or(0.0, |c| c.time)),
                        };
                        (cur, t, Some(name(a.to.state)), Some(a.to.time))
                    }
                    None => (held.map_or_else(|| "#00000000".to_string(), |c| name(c.state)), held.map_or(0.0, |c| c.time), None, None),
                };
                out.push(LayerState { playable: pi, layer: li, name: p.rt.layer_defs[li].name.clone(), current, current_time, next, next_time });
            }
        }
        out
    }

    pub fn playable_kinds(&self) -> Vec<PlayableLayerType> {
        self.playables.iter().map(|p| p.kind).collect()
    }

    fn normalize_builtins(&mut self) {
        let b = &mut self.builtins;
        for (i, side) in ["GestureLeft", "GestureRight"].iter().enumerate() {
            let wk = format!("{side}Weight");
            let mut w = b[&wk];
            let mut g = b[*side];
            if w != self.old_weights[i] {
                self.old_weights[i] = w;
                if w > 0.01 && (g == 0.0 || w < 0.99) {
                    g = 1.0;
                }
            }
            if g == 0.0 {
                w = 0.0;
            } else if g != 1.0 {
                w = 1.0;
            }
            b.insert(side.to_string(), g);
            b.insert(wk, w);
        }
        if self.tracking[MOUTH] == TrackingType::Animation {
            let v = self.normalized["Viseme"];
            b.insert("Viseme".into(), v);
        }
        b.insert("IsOnFriendsList".into(), 0.0);
    }

    fn convert(kind: Kind, value: f32) -> f32 {
        match kind {
            Kind::Float => value,
            Kind::Int => value.round_ties_even(),
            Kind::Bool => (value != 0.0) as u8 as f32,
            Kind::Trigger => 1.0,
        }
    }

    fn push(&mut self, name: &str, value: f32) {
        for p in &mut self.playables {
            if let Some(&i) = p.rt.index.get(name) {
                if p.rt.curve_controlled[i] {
                    continue;
                }
                let k = p.rt.params[i].kind;
                p.rt.params[i].value = Self::convert(k, value);
            }
        }
    }

    /// Advances the animator by `dt` seconds and evaluates it.
    pub fn step(&mut self, dt: f32) {
        for (n, k, _) in BUILT_INS {
            let Some(&i) = self.index.get(*n) else { continue };
            if self.parameters[i].kind != *k || *n == "AvatarVersion" {
                continue;
            }
            let lagging = *n == "Viseme" || *n == "TrackingType";
            self.parameters[i].value = if lagging { self.normalized[*n] } else { self.builtins[*n] };
        }
        for p in &mut self.parameters {
            if p.expression && p.synced {
                match p.kind {
                    Kind::Float => p.value = p.value.clamp(-1.0, 1.0),
                    Kind::Int => p.value = p.value.clamp(0.0, 255.0),
                    _ => {}
                }
            }
        }
        let seated = self.builtins["Seated"] != 0.0;
        if seated != self.prev_seated {
            if let Some(p) = self.playables.iter_mut().find(|p| p.kind == PlayableLayerType::Sitting) {
                p.blend = Some(Blend { start: p.weight, goal: if seated { 1.0 } else { 0.0 }, duration: 0.25, began: self.time + dt as f64 });
                self.prev_seated = seated;
            }
        }
        self.normalize_builtins();
        self.normalized = self.builtins.clone();
        let pushes: Vec<(String, f32)> = self
            .parameters
            .iter()
            .filter(|p| p.last != Some(p.value))
            .map(|p| (p.name.clone(), p.value))
            .collect();
        let mut order: Vec<(usize, String, f32)> = pushes
            .into_iter()
            .map(|(n, v)| {
                let k = self.parameters[self.index[&n]].kind;
                let rank = match k {
                    Kind::Float => 0,
                    Kind::Int => 1,
                    _ => 2,
                };
                (rank, n, v)
            })
            .collect();
        order.sort_by_key(|x| x.0);
        for (_, n, v) in order {
            self.push(&n, v);
        }
        for p in &mut self.parameters {
            p.last = Some(p.value);
        }
        for (n, _, _) in BUILT_INS {
            let v = self.builtins[*n];
            self.push(n, v);
        }
        for (n, v) in std::mem::take(&mut self.dynamics) {
            if let Some(&i) = self.index.get(&n) {
                let p = &mut self.parameters[i];
                p.value = match p.kind {
                    Kind::Int => v.trunc(),
                    Kind::Bool | Kind::Trigger => (v != 0.0) as u8 as f32,
                    Kind::Float => v,
                };
            }
        }
        self.time += dt as f64;
        self.advance_pose_spaces(dt);
        let now = self.time;
        for p in &mut self.playables {
            if let Some(b) = &p.blend {
                let (w, done) = b.at(now);
                p.weight = w;
                if done {
                    p.blend = None;
                }
            }
            let mut finished = Vec::new();
            for (l, b) in &p.layer_blends {
                let (w, done) = b.at(now);
                if let Some(layer) = p.rt.layers.get_mut(*l) {
                    layer.weight = w;
                }
                if done {
                    finished.push(*l);
                }
            }
            for l in finished {
                p.layer_blends.remove(&l);
            }
        }

        let prev = self.blended.clone();
        let clips = std::mem::take(&mut self.clips);
        let mut all_entered = Vec::new();
        for pi in 0..self.playables.len() {
            let holds: Vec<f32> = (0..self.targets.len()).map(|i| self.hold(pi, i)).collect();
            let mut entered = Vec::new();
            {
                let defaults = &self.defaults;
                let prev_ref = &prev;
                let clips_ref = &clips;
                let bound = self.playables[pi].bound.clone();
                let slots = self.playables[pi].slots.clone();
                let targets = &self.targets.list;
                let beneath = |t: u32| prev_ref[t as usize];
                let kept = |t: u32| holds[t as usize];
                let quats = &self.rotations;
                let body = self.body;
                let frozen = |rt: &ControllerRt, li: usize| {
                    if rt.layer_defs[li].blending == LayerBlending::Additive {
                        let mut v: Vec<(u32, f32, f32)> = additive_offsets(rt, li, clips_ref, defaults, body, quats).into_iter().map(|(t, (v, c))| (t, v, c)).collect();
                        v.sort_by_key(|x| x.0);
                        return v;
                    }
                    layer_pose(rt, li, clips_ref, defaults, &bound, &slots[li], targets, quats, false, &beneath, &kept, &beneath, &beneath)
                };
                self.playables[pi].rt.update(dt, &clips, &mut entered, &frozen);
            }
            all_entered.extend(entered.into_iter().map(|e| (pi, e)));
        }
        let n = self.targets.len();
        let cov: Vec<f32> = (0..n).map(|i| (!self.targets.list[i].rests_at_default() || self.curved[i]) as u8 as f32).collect();
        let mut acc = Accum::at_defaults(&cov, &self.defaults);
        for pi in 0..self.playables.len() {
            let pw = self.playables[pi].weight;
            let below: Vec<(f32, f32, bool)> = self.playables[pi]
                .human
                .iter()
                .map(|&t| {
                    let i = t as usize;
                    let b = (acc.sum[i], acc.cov[i], acc.touched[i]);
                    (acc.sum[i], acc.cov[i], acc.touched[i]) = (0.0, 0.0, false);
                    b
                })
                .collect();
            let mut layer_human = std::mem::take(&mut self.playables[pi].layer_human);
            let mut said = std::mem::take(&mut self.playables[pi].said);
            let mut played = std::mem::take(&mut self.playables[pi].played);
            // Without a humanoid rig, once a controller has played a motion every generic property reads its stream,
            // which starts from the defaults: Unity resets them all the frame it first does.
            let stack = (!self.humanoid).then(|| std::mem::replace(&mut acc, Accum::at_defaults(&cov, &self.defaults)));
            let params = self.evaluate(pi, &clips, &prev, &mut acc, &mut layer_human, &mut said, &mut played);
            if let Some(stack) = stack {
                let stream = std::mem::replace(&mut acc, stack);
                let p = &mut self.playables[pi];
                p.sounded |= p.rt.layers.iter().enumerate().any(|(li, l)| {
                    let w = if li == 0 { 1.0 } else { l.weight } * pw;
                    w > 0.0 && (l.active.is_some() || l.current.as_ref().is_some_and(|c| p.rt.motion(li, c.state).is_some()))
                });
                if p.sounded {
                    let adds = p.kind == PlayableLayerType::Additive;
                    for i in (0..n).filter(|&i| self.targets.list[i].generic()) {
                        if self.targets.list[i].discrete() {
                            if stream.touched[i] && pw >= 0.5 {
                                acc.sum[i] = stream.sum[i];
                                acc.touched[i] = true;
                            }
                            continue;
                        }
                        let v = stream.sum[i] + (1.0 - stream.cov[i]) * self.defaults[i];
                        if adds {
                            // Unity adds the stream itself, not its offset from identity, as the generic-rig
                            // playable-weight probe shows; rotations compose below.
                            if !matches!(self.targets.list[i], Target::Rotation(..)) {
                                acc.sum[i] += v * pw;
                            }
                        } else {
                            acc.sum[i] = acc.sum[i] * (1.0 - pw) + v * pw;
                            acc.cov[i] = acc.cov[i] * (1.0 - pw) + pw;
                        }
                        acc.touched[i] |= pw > 0.0;
                    }
                    if adds && pw > 0.0 {
                        let bound = |q: &&[u32; 4]| self.playables.iter().any(|o| o.binders[q[0] as usize] > 0);
                        for q in self.rotations[self.human_rotations..].iter().filter(bound) {
                            let quat = |a: &Accum| {
                                let [x, y, z, w] = q.map(|t| a.sum[t as usize] + (1.0 - a.cov[t as usize]) * self.defaults[t as usize]);
                                motion::unit(glam::Quat::from_xyzw(x, y, z, w))
                            };
                            let r = quat(&acc) * motion::weighted(quat(&stream), pw);
                            for (&t, c) in q.iter().zip([r.x, r.y, r.z, r.w]) {
                                acc.sum[t as usize] = c;
                                acc.cov[t as usize] = 1.0;
                            }
                        }
                    }
                    for (w, &x) in acc.wrote.iter_mut().zip(&stream.wrote) {
                        *w |= x;
                    }
                }
                let held = &mut self.playables[pi].held;
                for i in (0..n).filter(|&i| stream.wrote[i]) {
                    let value = match self.targets.list[i] {
                        Target::Translation(..) => Some(stream.sum[i] + (1.0 - stream.cov[i]) * self.defaults[i]),
                        _ => stream.overridden[i],
                    };
                    if let Some(v) = value {
                        held[i] = v;
                    }
                }
            }
            self.playables[pi].layer_human = layer_human;
            self.playables[pi].said = said;
            self.playables[pi].played = played;
            if self.playables[pi].kind == PlayableLayerType::Additive {
                for &t in &self.playables[pi].human {
                    acc.sum[t as usize] += (1.0 - acc.cov[t as usize]) * self.defaults[t as usize];
                }
            }
            let quads: Vec<[u32; 7]> = self.body.into_iter().chain(self.motion).chain(self.goals.iter().map(|g| g.0)).collect();
            let p = &mut self.playables[pi];
            // The Additive playable appends its body and goals to the stack's as transforms, each scaled by its
            // weight, and adds its muscles.
            let mut appended: Vec<(u32, f32)> = Vec::new();
            if p.kind == PlayableLayerType::Additive {
                for ids in quads.iter().filter(|ids| ids.iter().all(|&t| p.whole[t as usize])) {
                    let under = |t: u32| p.human.iter().position(|&h| h == t).map(|k| below[k].0).unwrap();
                    let read = |f: &dyn Fn(u32) -> f32| (glam::Vec3::new(f(ids[0]), f(ids[1]), f(ids[2])), glam::Quat::from_xyzw(f(ids[3]), f(ids[4]), f(ids[5]), f(ids[6])));
                    let (bt, bq) = read(&under);
                    let (at, aq) = read(&|t| acc.sum[t as usize]);
                    let bq = motion::unit(bq);
                    let (t, q) = (bt + bq * (at * pw), bq * motion::weighted(motion::unit(aq), pw));
                    appended.extend(ids.iter().copied().zip([t.x, t.y, t.z, q.x, q.y, q.z, q.w]));
                }
            }
            if p.kind != PlayableLayerType::Additive {
                // A playable's own body and goal rotations enter the stack as directions: a crossfade inside it
                // between turns far apart sums short of unit length and would weigh less than the playable's weight
                // (corpus 4 and 6 entering an emote).
                for q in self.rotations[..self.human_rotations].iter().filter(|q| p.whole[q[3] as usize]) {
                    let (c, len) = (acc.cov[q[3] as usize], q.iter().map(|&t| acc.sum[t as usize].powi(2)).sum::<f32>().sqrt());
                    if c > 0.0 && len > 0.0 {
                        q.iter().for_each(|&t| acc.sum[t as usize] *= c / len);
                    }
                }
                for q in self.rotations[..self.human_rotations].iter().filter(|q| pw * acc.cov[q[3] as usize] < 1.0) {
                    let pos = |t: u32| p.human.iter().position(|&h| h == t);
                    let (Some(a), Some(b), Some(c), Some(d)) = (pos(q[0]), pos(q[1]), pos(q[2]), pos(q[3])) else { continue };
                    let k = [a, b, c, d];
                    let under = k.map(|j| below[j].0 + (1.0 - below[j].1) * self.defaults[p.human[j] as usize]);
                    if motion::opposed(q.map(|t| acc.sum[t as usize]), under) {
                        q.iter().for_each(|&t| acc.sum[t as usize] = -acc.sum[t as usize]);
                    }
                }
            }
            for (&t, (s0, c0, t0)) in p.human.iter().zip(below) {
                let i = t as usize;
                let (own, c) = (acc.sum[i], acc.cov[i]);
                (acc.sum[i], acc.cov[i], acc.touched[i]) = if p.kind == PlayableLayerType::Additive {
                    (appended.iter().find(|a| a.0 == t).map_or(s0 + (own - self.defaults[i]) * pw, |a| a.1), c0, t0 || pw > 0.0)
                } else {
                    let a = pw * c;
                    (s0 * (1.0 - a) + own * pw, c0 * (1.0 - a) + a, t0 || a > 0.0)
                };
            }
            if p.weight > 0.0 {
                for (i, v) in params {
                    let k = p.rt.params[i].kind;
                    p.rt.params[i].value = Self::convert(k, v);
                }
            }
        }
        for (pi, e) in all_entered {
            self.run_behaviours(pi, e, &clips);
        }
        let mut out = prev.clone();
        for i in 0..n {
            if self.targets.list[i].discrete() {
                out[i] = acc.sum[i];
                continue;
            }
            out[i] = acc.sum[i] + (1.0 - acc.cov[i]) * self.defaults[i];
        }
        for i in (0..n).filter(|&i| acc.wrote[i]) {
            self.kept[i] = out[i];
        }
        if self.tracking[MOUTH] != TrackingType::Animation {
            self.face.speak(self.builtins["Viseme"].max(0.0) as usize, &mut out);
        } else {
            self.unbound_face.0.iter().for_each(|&t| out[t as usize] = prev[t as usize]);
        }
        if self.tracking[EYES] != TrackingType::Animation {
            self.face.look(self.blink, self.look_y, &mut out);
        } else {
            self.unbound_face.1.iter().for_each(|&t| out[t as usize] = prev[t as usize]);
        }
        for &t in &self.boneless {
            out[t as usize] = self.defaults[t as usize];
        }
        for q in self.human_rotations() {
            let len = q.iter().map(|&t| out[t as usize] * out[t as usize]).sum::<f32>().sqrt();
            if len > 0.0 {
                q.iter().for_each(|&t| out[t as usize] /= len);
            }
        }
        self.clips = clips;
        self.blended = out;
        self.place_goals();
        self.head = None;
    }

    /// Blends one playable's layers into the frame's accumulation, returning the values its animator-parameter
    /// curves give its own parameters.
    fn evaluate(&self, pi: usize, clips: &[CompiledClip], prev: &[f32], acc: &mut Accum, layer_human: &mut [HashMap<u32, f32>], said: &mut [Vec<(u32, f32, f32)>], played: &mut [bool]) -> Vec<(usize, f32)> {
        let p = &self.playables[pi];
        let mut own: HashMap<u32, (f32, f32)> = HashMap::new();
        let mut written = vec![false; self.targets.len()];
        let mut keeps = vec![!self.humanoid; self.targets.len()];
        let mut held = false;
        // On a humanoid avatar the Base and Additive playables write the human pose and animator parameters alone.
        let writes_generic = !self.humanoid || !matches!(p.kind, PlayableLayerType::Base | PlayableLayerType::Additive);
        let parameter = |i: usize| matches!(self.targets.list[i], Target::AnimatorParameter(_));
        let holdable = |i: usize| !(self.targets.list[i].rests_at_default() && !self.curved[i]) && !parameter(i) && !p.whole[i];
        // Without a humanoid rig a layer also repeats the animator parameters it last wrote.
        let remembered = |i: usize| holdable(i) || (!self.humanoid && parameter(i));
        // Without a humanoid rig one animator binds every controller's generic properties, and each controller's
        // states without write defaults hold them all.
        let holds: &[u32] = if self.humanoid { &p.bound } else { &p.reach };
        for li in 0..p.rt.layers.len() {
            let lw = if li == 0 { 1.0 } else { p.rt.layers[li].weight };
            if lw * p.weight <= 0.0 {
                continue;
            }
            // Without a humanoid rig `step` blends the generic stream by the playable's weight, and the weight never
            // reaches the controller's animator parameters.
            let wl = if self.humanoid { lw * p.weight } else { lw };
            if self.humanoid {
                match kept_by(&p.rt, li, clips, self.targets.len()) {
                    Some(k) => keeps.iter_mut().zip(k).for_each(|(a, b)| *a |= b),
                    None => keeps.fill(true),
                }
            }
            let playable_adds = p.kind == PlayableLayerType::Additive && self.humanoid;
            let additive = playable_adds || p.rt.layer_defs[li].blending == LayerBlending::Additive;
            // On a humanoid avatar the Additive playable's own override layers build a human pose like any
            // controller's, which the playable then adds to the stack; its other properties add layer by layer.
            let stacked = playable_adds && p.rt.layer_defs[li].blending != LayerBlending::Additive;
            let mask = &p.layer_masks[li];
            let passes = |i: usize| -> bool {
                if !writes_generic && self.targets.list[i].generic() {
                    return false;
                }
                if let (Target::Muscle(name), Some(parts)) = (&self.targets.list[i], &p.muscles) {
                    if !parts.contains(&human::part(name)) {
                        return false;
                    }
                }
                let Some(mk) = mask else { return true };
                match &self.targets.list[i] {
                    Target::Muscle(name) => mk.parts.contains(&human::part(name)),
                    other => match (&mk.nodes, node_of(other)) {
                        (Some(nodes), Some(n)) => nodes.get(&n).copied().unwrap_or(false),
                        _ => true,
                    },
                }
            };
            let holding = p.rt.layers[li].current.as_ref().filter(|c| !held && !additive && p.rt.motion(li, c.state).is_some() && p.rt.state(li, c.state).is_some_and(|s| !s.write_defaults));
            // Without a humanoid rig the first layer also gives the controller's default to each animator parameter
            // it binds but does not animate.
            let resets_parameters = holding.is_some() && !self.humanoid && li == 0;
            if let Some(c) = holding {
                held = true;
                let own_keys = playing_pose(&p.rt, li, c, clips, &self.defaults, false);
                for &t in holds {
                    let i = t as usize;
                    if written[i] || own_keys.contains_key(&t) || !holdable(i) || !passes(i) {
                        continue;
                    }
                    if self.targets.list[i].discrete() {
                        if wl < 0.5 {
                            continue;
                        }
                        acc.sum[i] = self.hold(pi, i);
                    } else {
                        acc.sum[i] = acc.sum[i] * (1.0 - wl) + self.hold(pi, i) * wl;
                        acc.cov[i] = acc.cov[i] * (1.0 - wl) + wl;
                    }
                    acc.touched[i] = true;
                    written[i] = true;
                }
            }
            let own_now = &own;
            let beneath = |t: u32| {
                let i = t as usize;
                if let Target::AnimatorParameter(name) = &self.targets.list[i] {
                    let held = p.rt.index.get(name).map_or(0.0, |&j| p.rt.params[j].value);
                    return own_now.get(&t).map_or(held, |&(sum, cov)| sum + (1.0 - cov) * held);
                }
                if self.targets.list[i].discrete() {
                    return acc.sum[i];
                }
                acc.sum[i] + (1.0 - acc.cov[i]) * self.defaults[i]
            };
            let kept = |t: u32| self.hold(pi, t as usize);
            let through = |t: u32| if acc.touched[t as usize] { beneath(t) } else { prev[t as usize] };
            let reset = |t: u32| match (acc.touched[t as usize], p.binders[t as usize] > 1) {
                (true, _) => beneath(t),
                (false, true) => prev[t as usize],
                (false, false) => self.defaults[t as usize],
            };
            let whole_pose = (!additive || stacked).then(|| layer_pose(&p.rt, li, clips, &self.defaults, &p.bound, &p.slots[li], &self.targets.list, &self.rotations, p.kind == PlayableLayerType::Action, &beneath, &kept, &through, &reset));
            if additive {
                if let Some(mut pose) = whole_pose.clone().filter(|_| p.drives_parameters[li]) {
                    self.pull_parameters(&p.rt, &pulled(&p.rt, li, clips, &self.defaults), &mut pose);
                    for (t, v, c) in pose {
                        if matches!(self.targets.list[t as usize], Target::AnimatorParameter(_)) {
                            let a = wl * c;
                            let e = own.entry(t).or_insert((0.0, 0.0));
                            e.0 = e.0 * (1.0 - a) + v * a;
                            e.1 = e.1 * (1.0 - a) + a;
                        }
                    }
                }
                let idle = match (&p.rt.layers[li].active, &p.rt.layers[li].current) {
                    (None, Some(c)) if !stacked => match (p.rt.motion(li, c.state).is_some(), p.rt.state(li, c.state).is_some_and(|s| s.write_defaults)) {
                        (true, _) => None,
                        (false, wd) => Some(wd),
                    },
                    _ => None,
                };
                let now: HashMap<u32, (f32, f32)> = match idle {
                    Some(false) => said[li].iter().map(|&(t, v, c)| (t, (v, c))).collect(),
                    Some(true) => {
                        said[li].clear();
                        HashMap::new()
                    }
                    None => {
                        played[li] = true;
                        let now = additive_offsets(&p.rt, li, clips, &self.defaults, self.body, &self.rotations);
                        if !stacked {
                            said[li] = now.iter().map(|(&t, &(v, c))| (t, v, c)).collect();
                        }
                        now
                    }
                };
                // Until it has played a motion, a layer idle in an empty state without write defaults adds no generic
                // property's default.
                let adds_defaults = played[li] || idle == Some(true);
                for &t in &p.reach {
                    let i = t as usize;
                    if !passes(i) || self.targets.list[i].discrete() || self.goals.iter().any(|g| g.0.contains(&t)) || (p.kind == PlayableLayerType::Additive && p.whole[i]) {
                        continue;
                    }
                    let (v, c) = now.get(&t).copied().unwrap_or((0.0, 0.0));
                    let generic = self.targets.list[i].generic();
                    if c == 0.0 && generic && !adds_defaults {
                        continue;
                    }
                    let scale = if p.whole[i] { lw } else { wl };
                    // Unity's stream holds every generic property it binds at its default.
                    let cover = if generic { 1.0 } else { acc.cov[i] };
                    let unwritten = (self.defaults[i] - self.targets.list[i].identity()) * cover;
                    acc.sum[i] += (v + (1.0 - c) * unwritten) * scale;
                    if c > 0.0 {
                        acc.touched[i] = true;
                        written[i] = true;
                        }
                }
                if !self.humanoid && adds_defaults {
                    for &t in p.bound.iter().filter(|&&t| parameter(t as usize)) {
                        let Target::AnimatorParameter(name) = &self.targets.list[t as usize] else { continue };
                        let Some(&j) = p.rt.index.get(name) else { continue };
                        let (v, c) = now.get(&t).copied().unwrap_or((0.0, 0.0));
                        let d = p.rt.params[j].default;
                        let e = own.entry(t).or_insert((0.0, 0.0));
                        *e = (e.0 + (1.0 - e.1) * d + (v + (1.0 - c) * d) * wl, 1.0);
                    }
                }
            }
            let Some(mut pose) = whole_pose else { continue };
            if stacked {
                pose.retain(|x| p.whole[x.0 as usize]);
            }
            if let (None, Some(c)) = (&p.rt.layers[li].active, &p.rt.layers[li].current) {
                let wd = p.rt.state(li, c.state).is_some_and(|s| s.write_defaults);
                match (p.rt.motion(li, c.state).is_some(), wd) {
                    (true, _) => {
                        let mut last: Vec<(u32, f32, f32)> = pose.iter().filter(|x| remembered(x.0 as usize) && x.2 > 0.0).map(|x| (x.0, x.1, 1.0)).collect();
                        if !wd {
                            let animated: std::collections::HashSet<u32> = last.iter().map(|x| x.0).collect();
                            last.extend(holds.iter().filter(|&&t| remembered(t as usize) && !animated.contains(&t)).map(|&t| (t, self.hold(pi, t as usize), 1.0)));
                        }
                        said[li] = last;
                    }
                    (false, true) => said[li].clear(),
                    (false, false) => pose = said[li].clone(),
                }
            }
            // An override layer in a state gives the whole human pose: what the state does not write, or a
            // transition leaves uncovered, stays where the layer last wrote it, at rest until it has. A layer
            // without states, or in a state whose motion holds no humanoid clip, gives nothing.
            let posing = &p.poses[li];
            let l = &p.rt.layers[li];
            let gives_pose = match &l.active {
                Some(a) => posing.contains(&a.to.state) || matches!(&a.from, From::State(s) if posing.contains(&s.state)) || (matches!(&a.from, From::Frozen(_)) && !posing.is_empty()),
                None => l.current.as_ref().is_some_and(|c| posing.contains(&c.state)),
            };
            if gives_pose {
                let last = &mut layer_human[li];
                for x in pose.iter_mut().filter(|x| p.whole[x.0 as usize]) {
                    let kept = last.get(&x.0).copied().unwrap_or(self.defaults[x.0 as usize]);
                    *x = (x.0, x.1 * x.2 + kept * (1.0 - x.2), 1.0);
                    last.insert(x.0, x.1);
                }
                let kept: Vec<(u32, f32, f32)> = p.bound
                    .iter()
                    .filter(|&&t| p.whole[t as usize] && !pose.iter().any(|x| x.0 == t))
                    .map(|&t| (t, last.get(&t).copied().unwrap_or(self.defaults[t as usize]), 1.0))
                    .collect();
                pose.extend(kept);
            }
            self.align_rotations(&mut pose, acc, lw);
            if !stacked {
                self.pull_parameters(&p.rt, &pulled(&p.rt, li, clips, &self.defaults), &mut pose);
                if resets_parameters {
                    self.pull_parameters(&p.rt, &p.bound, &mut pose);
                }
            }
            for (t, v, c) in pose {
                let i = t as usize;
                if !passes(i) {
                    continue;
                }
                let a = if p.whole[i] { lw } else { wl } * c;
                if let Target::AnimatorParameter(name) = &self.targets.list[i] {
                    let e = own.entry(t).or_insert((0.0, 0.0));
                    e.0 = e.0 * (1.0 - a) + v * a;
                    e.1 = e.1 * (1.0 - a) + a;
                    if a > 0.0 && !self.humanoid {
                        let current = p.rt.index.get(name).map_or(0.0, |&j| p.rt.params[j].value);
                        acc.overridden[i] = Some(e.0 + (1.0 - e.1) * current);
                        written[i] = true;
                    }
                    continue;
                }
                if self.targets.list[i].discrete() {
                    if a >= 0.5 {
                        acc.sum[i] = v;
                        acc.touched[i] = true;
                        written[i] = true;
                        }
                    continue;
                }
                acc.sum[i] = acc.sum[i] * (1.0 - a) + v * a;
                acc.cov[i] = acc.cov[i] * (1.0 - a) + a;
                acc.touched[i] = true;
                written[i] |= a > 0.0;
                if a > 0.0 {
                    acc.overridden[i] = Some(acc.sum[i] + (1.0 - acc.cov[i]) * self.defaults[i]);
                }
            }
        }
        for ((w, &x), &k) in acc.wrote.iter_mut().zip(&written).zip(&keeps) {
            *w |= x && k;
        }
        own.into_iter()
            .filter_map(|(t, (sum, cov))| {
                let Target::AnimatorParameter(name) = &self.targets.list[t as usize] else { return None };
                let &i = p.rt.index.get(name)?;
                Some((i, sum + (1.0 - cov) * p.rt.params[i].value))
            })
            .collect()
    }

    /// Animator parameters a transition pulls to their defaults take the controller's default for them.
    fn pull_parameters(&self, rt: &ControllerRt, pulled: &[u32], pose: &mut Vec<(u32, f32, f32)>) {
        for &t in pulled {
            let Target::AnimatorParameter(name) = &self.targets.list[t as usize] else { continue };
            if let (false, Some(&j)) = (pose.iter().any(|x| x.0 == t), rt.index.get(name)) {
                pose.push((t, rt.params[j].default, 1.0));
            }
        }
    }

    fn human_rotations(&self) -> &[[u32; 4]] {
        &self.rotations[..self.human_rotations]
    }

    /// Negates a layer's rotation lying in the other hemisphere from the rotation beneath it, as Unity's blend does.
    /// The body and the IK goals are negated only in a layer below full weight: a layer at full weight writes them
    /// with the sign its clips give.
    fn align_rotations(&self, pose: &mut [(u32, f32, f32)], acc: &Accum, weight: f32) {
        let mut quats: HashMap<u32, [Option<usize>; 4]> = HashMap::new();
        let mut human = vec![[None; 4]; self.human_rotations];
        for (k, &(t, _, _)) in pose.iter().enumerate() {
            if let Target::Rotation(n, a) = &self.targets.list[t as usize] {
                quats.entry(*n).or_default()[axis_index(*a)] = Some(k);
            }
            for (slots, q) in human.iter_mut().zip(self.human_rotations()) {
                if let Some(j) = q.iter().position(|&x| x == t) {
                    slots[j] = Some(k);
                }
            }
        }
        for (slots, q) in human.into_iter().zip(self.human_rotations()).filter(|_| weight < 1.0) {
            let [Some(x), Some(y), Some(z), Some(w)] = slots else { continue };
            let below = q.map(|t| acc.sum[t as usize] + (1.0 - acc.cov[t as usize]) * self.defaults[t as usize]);
            if motion::opposed([pose[x].1, pose[y].1, pose[z].1, pose[w].1], below) {
                [x, y, z, w].into_iter().for_each(|i| pose[i].1 = -pose[i].1);
            }
        }
        for (n, slots) in quats {
            let [Some(x), Some(y), Some(z), Some(w)] = slots else { continue };
            let below = |a: Axis| {
                let Some(i) = self.targets.index_of(&Target::Rotation(n, a)) else { return 0.0 };
                acc.sum[i] + (1.0 - acc.cov[i]) * self.defaults[i]
            };
            let dot = pose[x].1 * below(Axis::X) + pose[y].1 * below(Axis::Y) + pose[z].1 * below(Axis::Z) + pose[w].1 * below(Axis::W);
            if dot < 0.0 {
                for k in [x, y, z, w] {
                    pose[k].1 = -pose[k].1;
                }
            }
        }
    }

    fn run_behaviours(&mut self, pi: usize, e: Entered, clips: &[CompiledClip]) {
        for b in e.behaviours {
            let Some(beh) = self.behaviours.get(b.0 as usize).cloned() else { continue };
            match beh {
                Behaviour::ParameterDriver { local_only, parameters } => {
                    if local_only && self.builtins["IsLocal"] == 0.0 {
                        continue;
                    }
                    for entry in parameters {
                        self.drive(&entry);
                    }
                }
                Behaviour::PlayableLayerControl { layer, goal_weight, blend_duration } => {
                    let kind = controlled(layer);
                    let now = self.time;
                    if let Some(p) = self.playables.iter_mut().find(|p| p.kind == kind) {
                        p.blend = Some(Blend { start: p.weight, goal: goal_weight, duration: blend_duration, began: now });
                    }
                }
                Behaviour::AnimatorLayerControl { playable, layer, goal_weight, blend_duration } => {
                    let kind = controlled(playable);
                    let now = self.time;
                    if let Some(p) = self.playables.iter_mut().find(|p| p.kind == kind) {
                        if let Some(l) = p.rt.layers.get(layer as usize) {
                            let start = l.weight;
                            p.layer_blends.insert(layer as usize, Blend { start, goal: goal_weight, duration: blend_duration, began: now });
                        }
                    }
                }
                Behaviour::TrackingControl { head, left_hand, right_hand, hip, left_foot, right_foot, left_fingers, right_fingers, eyes, mouth } => {
                    let set = [head, left_hand, right_hand, hip, left_foot, right_foot, left_fingers, right_fingers, eyes, mouth];
                    for (cur, new) in self.tracking.iter_mut().zip(set) {
                        if new != TrackingType::NoChange {
                            *cur = new;
                        }
                    }
                }
                Behaviour::LocomotionControl { disable_locomotion } => self.locomotion_disabled = disable_locomotion,
                Behaviour::TemporaryPoseSpace { enter_pose_space, fixed_delay, delay_time } => {
                    let p = &self.playables[pi];
                    let sdk_wait = if fixed_delay {
                        SdkWait::Since(self.time as f32)
                    } else {
                        SdkWait::Normalized { seconds: delay_time * p.rt.duration(e.layer, e.state, clips), waited: 0.0 }
                    };
                    self.pose_spaces.retain(|s| !(s.playable == pi && s.layer == e.layer));
                    self.pose_spaces.push(PoseSpace {
                        enter: enter_pose_space,
                        playable: pi,
                        layer: e.layer,
                        state: e.state,
                        sdk_wait,
                        delay: delay_time,
                        applied: (delay_time <= 0.0).then_some(self.time),
                    });
                }
            }
        }
    }

    fn advance_pose_spaces(&mut self, dt: f32) {
        let now = self.time;
        let mut keep = Vec::new();
        for mut s in std::mem::take(&mut self.pose_spaces) {
            let Some(applied) = s.applied else {
                let l = &self.playables[s.playable].rt.layers[s.layer];
                let here = match &l.active {
                    Some(a) => a.to.state == s.state,
                    None => l.current.as_ref().is_some_and(|c| c.state == s.state),
                };
                if !here {
                    continue;
                }
                let due = match &mut s.sdk_wait {
                    SdkWait::Since(entered) => now as f32 - *entered >= s.delay,
                    SdkWait::Normalized { seconds, waited } => {
                        *waited += dt;
                        *waited >= *seconds
                    }
                };
                if due {
                    s.applied = Some(now);
                }
                keep.push(s);
                continue;
            };
            if now < applied + s.delay as f64 {
                keep.push(s);
                continue;
            }
            self.view = match (s.enter, self.head, self.head_view) {
                (true, Some(h), Some((scale, v))) => h.point(v) / scale,
                (true, _, _) => self.view,
                (false, _, _) => self.default_view,
            };
        }
        self.pose_spaces = keep;
    }

    /// Each playable's weight and its controller layers' weights, the first layer's always 1.
    pub fn weights(&self) -> Vec<(f32, Vec<f32>)> {
        self.playables
            .iter()
            .map(|p| (p.weight, p.rt.layers.iter().enumerate().map(|(i, l)| if i == 0 { 1.0 } else { l.weight }).collect()))
            .collect()
    }

    /// Each playable's animator parameters an animation curve drives, with their values.
    pub fn curve_parameters(&self) -> Vec<Vec<(String, f32)>> {
        self.playables
            .iter()
            .map(|p| {
                p.rt.params
                    .iter()
                    .zip(&p.rt.curve_controlled)
                    .filter(|(_, c)| **c)
                    .map(|(q, _)| (q.name.clone(), q.value))
                    .collect()
            })
            .collect()
    }

    fn source_value(&self, name: &str) -> f32 {
        self.get(name).unwrap_or(0.0)
    }

    fn drive(&mut self, entry: &DriverEntry) {
        let name = match entry {
            DriverEntry::Set { name, .. }
            | DriverEntry::Add { name, .. }
            | DriverEntry::Random { name, .. }
            | DriverEntry::Copy { name, .. } => name.clone(),
        };
        let Some(&i) = self.index.get(&name) else { return };
        let kind = self.parameters[i].kind;
        let old = self.parameters[i].value;
        let copy = |s: &Self, source: &str, r: &Option<crate::model::ConvertRange>| -> f32 {
            let v = s.source_value(source);
            match r {
                None => v,
                Some(r) => {
                    if r.dest_max != r.dest_min {
                        let t = if r.source_max != r.source_min { ((v - r.source_min) / (r.source_max - r.source_min)).clamp(0.0, 1.0) } else { 0.0 };
                        r.dest_min + (r.dest_max - r.dest_min) * t
                    } else {
                        r.dest_min
                    }
                }
            }
        };
        let new = match (kind, entry) {
            (Kind::Int, DriverEntry::Set { value, .. }) => value.trunc(),
            (Kind::Int, DriverEntry::Add { value, .. }) => old + value.trunc(),
            (Kind::Int, DriverEntry::Random { value_min, value_max, .. }) => {
                self.random.range_int(*value_min as i32, (*value_max as i32).saturating_add(1)) as f32
            }
            (Kind::Int, DriverEntry::Copy { source, convert_range, .. }) => copy(self, source, convert_range).trunc(),
            (Kind::Float, DriverEntry::Set { value, .. }) => *value,
            (Kind::Float, DriverEntry::Add { value, .. }) => old + value,
            (Kind::Float, DriverEntry::Random { value_min, value_max, .. }) => self.random.range_float(*value_min, *value_max),
            (Kind::Float, DriverEntry::Copy { source, convert_range, .. }) => copy(self, source, convert_range),
            (_, DriverEntry::Set { value, .. }) => (*value != 0.0) as u8 as f32,
            (_, DriverEntry::Add { value, .. }) => (((old != 0.0) as u8 as f64 + *value as f64) != 0.0) as u8 as f32,
            (_, DriverEntry::Random { chance, .. }) => (self.random.range_float(0.0, 1.0) < *chance) as u8 as f32,
            (_, DriverEntry::Copy { source, convert_range, .. }) => (copy(self, source, convert_range) != 0.0) as u8 as f32,
        };
        let p = &mut self.parameters[i];
        if matches!(kind, Kind::Bool | Kind::Trigger) && !p.synced {
            p.value = new;
            p.last = Some(new);
            for pl in &mut self.playables {
                if let Some(&j) = pl.rt.index.get(&name) {
                    if pl.rt.curve_controlled[j] {
                        continue;
                    }
                    let k = pl.rt.params[j].kind;
                    pl.rt.params[j].value = Self::convert(k, new);
                }
            }
            return;
        }
        p.value = new;
    }
}

fn node_of(t: &Target) -> Option<u32> {
    match t {
        Target::NodeActive(n)
        | Target::RendererEnabled(n)
        | Target::BlendShape(n, _)
        | Target::MaterialProperty(n, _)
        | Target::Translation(n, _)
        | Target::Rotation(n, _)
        | Target::Scale(n, _)
        | Target::Slot(n, _) => Some(*n),
        _ => None,
    }
}

/// What a zero-length transition from a blend tree into a state with a motion, both writing defaults, pulls to the
/// controller's defaults: the animator parameters among what the source animates and the destination does not.
fn pulled(rt: &ControllerRt, li: usize, clips: &[CompiledClip], defaults: &[f32]) -> Vec<u32> {
    let l = &rt.layers[li];
    let (None, Some(p), Some(to)) = (&l.active, &l.snapped, &l.current) else { return vec![] };
    let wd = |s: crate::model::StateId| rt.state(li, s).is_some_and(|s| s.write_defaults);
    if rt.motion(li, to.state).is_none() || matches!(rt.motion(li, p.state), Some(Motion::Clip(_))) || !wd(p.state) || !wd(to.state) {
        return vec![];
    }
    let dest = playing_pose(rt, li, to, clips, defaults, false);
    playing_pose(rt, li, p, clips, defaults, false).into_keys().filter(|t| !dest.contains_key(t)).collect()
}

/// What an additive layer adds this frame, by target: the sum of its shares' offsets and of their weights. A
/// crossfade's destination offset turns into the source's hemisphere, and an interrupted one fades from the offsets
/// the layer added when interrupted, as corpus 3's emote pack and the crossfade probe show.
fn additive_offsets(rt: &ControllerRt, li: usize, clips: &[CompiledClip], defaults: &[f32], body: Option<[u32; 7]>, rotations: &[[u32; 4]]) -> HashMap<u32, (f32, f32)> {
    let offset = |c: &machine::Playing| -> HashMap<u32, f32> {
        let reference = motion_pose(rt, li, c.state, Sample::Reference, &[], clips, defaults, true);
        let r = |t: u32| reference.get(&t).map_or(defaults[t as usize], |r| r.0);
        let pose = playing_pose(rt, li, c, clips, defaults, true);
        let mut d: HashMap<u32, f32> = pose.iter().map(|(&t, &(v, _))| (t, v - r(t))).collect();
        if let Some(b) = body.filter(|b| b.iter().all(|t| pose.contains_key(t))) {
            let read = |f: &dyn Fn(u32) -> f32| (glam::Vec3::new(f(b[0]), f(b[1]), f(b[2])), glam::Quat::from_xyzw(f(b[3]), f(b[4]), f(b[5]), f(b[6])));
            let ((clip_t, clip_q), (ref_t, ref_q)) = (read(&|t| pose[&t].0), read(&r));
            let back = motion::unit(ref_q).conjugate();
            let (dt, dq) = (back * (clip_t - ref_t), back * motion::unit(clip_q));
            for (&id, v) in b.iter().zip([dt.x, dt.y, dt.z, dq.x, dq.y, dq.z, dq.w - 1.0]) {
                d.insert(id, v);
            }
        }
        d
    };
    let whole = |d: HashMap<u32, f32>| -> HashMap<u32, (f32, f32)> { d.into_iter().map(|(t, v)| (t, (v, 1.0))).collect() };
    let l = &rt.layers[li];
    let (under, mut over, w) = match (&l.active, &l.current) {
        (Some(a @ machine::Active { from: From::State(from), .. }), _) if a.progress() > 0.0 => (whole(offset(from)), offset(&a.to), a.progress()),
        (Some(a @ machine::Active { from: From::Frozen(held), .. }), _) if rt.layer_defs[li].blending == LayerBlending::Additive => (held.iter().map(|&(t, v, c)| (t, (v, c))).collect(), offset(&a.to), a.progress()),
        (_, Some(c)) => return whole(offset(c)),
        _ => return HashMap::new(),
    };
    const UNIT: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
    for q in rotations {
        let o = q.map(|t| over.get(&t).copied());
        let u = q.map(|t| under.get(&t).map(|&(v, c)| if c > 0.0 { v / c } else { 0.0 }));
        let (Some(o), Some(u)) = (o.iter().copied().collect::<Option<Vec<f32>>>(), u.iter().copied().collect::<Option<Vec<f32>>>()) else { continue };
        let (o, u) = ([o[0], o[1], o[2], o[3] + 1.0], [u[0], u[1], u[2], u[3] + 1.0]);
        if motion::opposed(o, u) {
            for ((&t, v), unit) in q.iter().zip(o).zip(UNIT) {
                over.insert(t, -v - unit);
            }
        }
    }
    let mut now: HashMap<u32, (f32, f32)> = HashMap::new();
    for (t, (v, c)) in under {
        let e = now.entry(t).or_default();
        e.0 += (1.0 - w) * v;
        e.1 += (1.0 - w) * c;
    }
    for (t, v) in over {
        let e = now.entry(t).or_default();
        e.0 += w * v;
        e.1 += w;
    }
    now
}

/// A playing state's pose, its motion sampled where its time parameter puts it.
fn playing_pose(rt: &ControllerRt, layer: usize, p: &machine::Playing, clips: &[CompiledClip], defaults: &[f32], additive: bool) -> HashMap<u32, (f32, bool)> {
    motion_pose(rt, layer, p.state, Sample::At(rt.sampled(layer, p.state, p.time)), &p.direct_starts, clips, defaults, additive)
}

/// One motion at one normalized time: the value of every target it writes, and whether only an animated
/// transform's undriven components put it there. Under a direct blend tree a leaf at weight zero, its own weight
/// parameter below zero or its 1D tree's end, still writes its share, nothing, but for transforms and muscles; a
/// leaf whose parameter is zero does not play.
#[derive(Clone, Copy)]
enum Sample {
    /// A normalized state time.
    At(f32),
    /// The pose an additive layer subtracts: each clip's reference pose, or its first frame.
    Reference,
}

fn motion_pose(rt: &ControllerRt, layer: usize, state: crate::model::StateId, sample: Sample, starts: &[(u32, f32)], clips: &[CompiledClip], defaults: &[f32], additive: bool) -> HashMap<u32, (f32, bool)> {
    let time = match sample {
        Sample::At(t) => t,
        Sample::Reference => 0.0,
    };
    let mut ls: Vec<Leaf> = Vec::new();
    rt.state_leaves(layer, state, &mut ls);
    let offset = rt.cycle_offset(layer, state);
    let mirror_phase = if rt.state_mirrors(layer, state) { 0.5 } else { 0.0 };
    let mut acc: HashMap<u32, (f32, bool)> = HashMap::new();
    let direct = ls.iter().any(|l| l.direct);
    for l in &ls {
        let c = clips[l.clip as usize].variant(l.mirror);
        for &t in &c.writes {
            let filled = c.rests(t);
            let e = acc.entry(t).or_insert((0.0, true));
            e.1 &= filled;
        }
    }
    let mut rotations: Vec<[u32; 4]> = ls.iter().flat_map(|l| clips[l.clip as usize].variant(l.mirror).rotations.iter().copied()).collect();
    rotations.sort_unstable();
    rotations.dedup();
    let mut started = false;
    let mut weighted = std::collections::HashSet::new();
    for l in ls.iter().filter(|l| l.weight != 0.0 || l.direct) {
        let c = clips[l.clip as usize].variant(l.mirror);
        let mut own: HashMap<u32, f32> = HashMap::new();
        let began = starts.iter().find(|(id, _)| l.direct && c.looping && *id == l.branch).map_or(0.0, |&(_, t)| t);
        let elapsed = time - began;
        let at = if l.time_scale < 0.0 { 1.0 - (elapsed + offset) } else { elapsed + offset };
        match (sample, c.reference) {
            (Sample::Reference, Some((r, seconds))) => {
                let r = &clips[r as usize];
                let r = if c.mirror == r.mirror { r.variant(l.mirror) } else { r.variant(!l.mirror) };
                r.sample(if r.length > 0.0 { seconds / r.length } else { 0.0 }, defaults, additive, &mut |t, v| {
                    if c.writes.binary_search(&t).is_ok() {
                        own.insert(t, v);
                    }
                });
                own.extend(c.unreferenced.iter().copied());
            }
            // The first frame an additive layer subtracts is the clip's own, whatever mirrors the state.
            (Sample::Reference, None) => clips[l.clip as usize].variant(false).sample(at + l.cycle_offset, defaults, additive, &mut |t, v| {
                own.insert(t, v);
            }),
            // A mirrored state plays its looping humanoid clips half a cycle on, whatever drives its time.
            (Sample::At(_), _) => c.sample(at + l.cycle_offset + c.cycle_offset + if clips[l.clip as usize].mirrored.is_some() && c.looping { mirror_phase } else { 0.0 }, defaults, additive, &mut |t, v| {
                own.insert(t, v);
            }),
        }
        if !direct {
            for t in acc.keys() {
                own.entry(*t).or_insert(defaults[*t as usize]);
            }
        }
        if started {
            for q in &rotations {
                if !q.iter().all(|t| own.contains_key(t) && acc.contains_key(t)) {
                    continue;
                }
                if q.iter().map(|t| own[t] * acc[t].0).sum::<f32>() < 0.0 {
                    for t in q {
                        own.insert(*t, -own[t]);
                    }
                }
            }
        }
        if l.weight == 0.0 {
            own.retain(|t, _| c.resting.binary_search(t).is_err());
        }
        for (t, v) in own {
            acc.get_mut(&t).unwrap().0 += l.weight * v;
            if direct {
                weighted.insert(t);
            }
        }
        started = true;
    }
    if direct {
        acc.retain(|t, _| weighted.contains(t));
        // Unity's human pose is whole: what a direct tree's weights leave short of one is the rest pose.
        let short = 1.0 - ls.iter().map(|l| l.weight).sum::<f32>();
        if short > 0.0 {
            for l in ls.iter().filter(|l| l.weight != 0.0) {
                for &t in clips[l.clip as usize].variant(l.mirror).human_slots() {
                    if let Some(e) = acc.get_mut(&t).filter(|_| weighted.remove(&t)) {
                        e.0 += short * defaults[t as usize];
                    }
                }
            }
        }
    }
    // A state's body and goal rotations leave it as directions: a blend tree's sum of turns far apart falls short of
    // unit length, and a crossfade from the identity into it would weigh it less (corpus 4 to 6 entering an emote).
    let human = |q: &&[u32; 4]| ls.iter().any(|l| clips[l.clip as usize].variant(l.mirror).human_slots().binary_search(&q[0]).is_ok());
    for q in rotations.iter().filter(human) {
        let Some(v) = whole(q.map(|t| acc.get(&t).map(|e| e.0))).map(glam::Quat::from_array).filter(|v| v.length_squared() > 0.0) else { continue };
        q.iter().zip(v.normalize().to_array()).for_each(|(t, c)| acc.get_mut(t).unwrap().0 = c);
    }
    acc
}

/// The targets a write-defaults state's direct-tree leaves write only at weight zero, which they cover not at all.
fn uncovered(rt: &ControllerRt, li: usize, state: crate::model::StateId, clips: &[CompiledClip], targets: &[Target]) -> std::collections::HashSet<u32> {
    let mut ls = Vec::new();
    rt.state_leaves(li, state, &mut ls);
    if !rt.state(li, state).is_some_and(|s| s.write_defaults) || !ls.iter().any(|l| l.direct) {
        return Default::default();
    }
    let mut weight: HashMap<u32, f32> = HashMap::new();
    for l in &ls {
        for &t in &clips[l.clip as usize].variant(l.mirror).writes {
            *weight.entry(t).or_insert(0.0) += l.weight.abs();
        }
    }
    weight.into_iter().filter(|&(t, w)| w == 0.0 && !matches!(targets[t as usize], Target::AnimatorParameter(_) | Target::Slot(..))).map(|(t, _)| t).collect()
}

/// Which targets a playable's writes this frame give to `kept` on a humanoid rig, by layer: those the layer's leaves at
/// weight write, the source's too during a transition and the state it shows on a snap frame; None, every target, under
/// a motion without write defaults or a transition from a frozen pose, which no trace separates. The marks pool across
/// the playable's layers: the hold probe's held fill is kept while another layer plays a motion without write defaults.
fn kept_by(rt: &ControllerRt, li: usize, clips: &[CompiledClip], n: usize) -> Option<Vec<bool>> {
    let l = &rt.layers[li];
    let states: Vec<crate::model::StateId> = match (&l.active, &l.current) {
        (Some(a), _) => match &a.from {
            From::State(s) => vec![s.state, a.to.state],
            From::Frozen(_) => return None,
        },
        (None, Some(c)) => vec![snap_shown(rt, li).unwrap_or(c).state],
        (None, None) => vec![],
    };
    let mut out = vec![false; n];
    for state in states {
        if rt.motion(li, state).is_some() && rt.state(li, state).is_some_and(|s| !s.write_defaults) {
            return None;
        }
        let mut ls = Vec::new();
        rt.state_leaves(li, state, &mut ls);
        for l in ls.iter().filter(|l| l.weight != 0.0) {
            clips[l.clip as usize].variant(l.mirror).writes.iter().for_each(|&t| out[t as usize] = true);
        }
    }
    Some(out)
}

/// The source a layer still shows on a snap frame. On the frame a zero-length transition takes a controller's first
/// layer from a state with a motion into an empty state, both writing defaults, Unity shows the source, advanced, as
/// the blend duration and write-defaults layering probes show; a later layer returns at once.
fn snap_shown(rt: &ControllerRt, li: usize) -> Option<&machine::Playing> {
    let l = &rt.layers[li];
    let cur = l.current.as_ref()?;
    let wd = |s: crate::model::StateId| rt.state(li, s).is_some_and(|s| s.write_defaults);
    l.snapped.as_ref().filter(|p| li == 0 && l.active.is_none() && rt.motion(li, cur.state).is_none() && wd(cur.state) && rt.motion(li, p.state).is_some() && wd(p.state))
}

/// A layer's pose this frame: each target it writes, with how much of it the layer covers. Outside a transition a
/// state covers what its motion animates. During one, the source covers by the remaining weight what it animates, and
/// without write defaults also every other bound target but transforms at the value beneath; the destination covers
/// by the transition's weight what it animates, and under write defaults every other bound target at its rest value.
/// What a write-defaults state's motion binds but does not write this frame takes the value beneath, else last
/// frame's (`through`); under a direct blend tree, else last frame's only when another layer binds it, else its
/// default (`reset`): the traces show a lone direct-tree toggle returning to its default where a 2D tree's released
/// child with a length holds. A target other than an animator parameter or a material slot that only this state's
/// zero-length clips write takes the value beneath, else its default: the blend duration probe's 2D trees reset a
/// released zero-length child's shape the frame its weight reaches zero, alone or under a direct tree, while corpus
/// parameters driven by zero-length clips hold. The blend duration probe's trees without write defaults show a direct
/// tree feeding back each bound target from last frame, by the weight of the leaves that do not write it, with the
/// default making up what its weights leave short of one. Under write defaults a target only a direct tree's leaves
/// at weight zero write is covered by none of them, so it keeps the value beneath, as a toggle's far end over a
/// radial shows; an animator parameter they write still takes the zero share. A destination rotation of `quats` in the
/// other hemisphere from its source is negated before the two blend, unless, without `turns_over`, it is a humanoid
/// rotation only the source's clips key in part: then the source is negated (corpus 4 to 6). With `turns_over`, a crossfade from a playing
/// state whose humanoid source and destination rotations both lie in the other hemisphere from the value beneath
/// comes out negated. On a snap frame ([`snap_shown`]) the layer gives its source's pose, where what the source's
/// lasting clips bind but do not write this frame takes its default rather than last frame's.
#[allow(clippy::too_many_arguments)]
fn layer_pose(rt: &ControllerRt, li: usize, clips: &[CompiledClip], defaults: &[f32], bound: &[u32], slots: &[u32], targets: &[Target], quats: &[[u32; 4]], turns_over: bool, beneath: &dyn Fn(u32) -> f32, held: &dyn Fn(u32) -> f32, through: &dyn Fn(u32) -> f32, reset: &dyn Fn(u32) -> f32) -> Vec<(u32, f32, f32)> {
    let l = &rt.layers[li];
    let Some(cur) = &l.current else { return vec![] };
    let lag = snap_shown(rt, li);
    let steady = match &l.active {
        Some(a) if a.progress() > 0.0 => None,
        Some(a) => match &a.from {
            From::State(p) => Some(p),
            From::Frozen(v) => return v.clone(),
        },
        None => Some(lag.unwrap_or(cur)),
    };
    let animated = |m: HashMap<u32, (f32, bool)>| -> HashMap<u32, f32> { m.into_iter().filter(|(_, (_, f))| !f).map(|(t, (v, _))| (t, v)).collect() };
    match (&l.active, steady) {
        (_, Some(cur)) => {
            let mut pose = animated(playing_pose(rt, li, cur, clips, defaults, false));
            let wd = rt.state(li, cur.state).is_some_and(|s| s.write_defaults);
            let uncovered = uncovered(rt, li, cur.state, clips, targets);
            if rt.motion(li, cur.state).is_some() {
                let shown: std::collections::HashSet<u32> =
                    pose.keys().filter_map(|&t| if let Target::Slot(n, _) = targets[t as usize] { Some(n) } else { None }).collect();
                for &t in slots {
                    if !wd || matches!(targets[t as usize], Target::Slot(n, _) if shown.contains(&n)) {
                        pose.entry(t).or_insert(defaults[t as usize]);
                    }
                }
            }
            if let Some(m) = rt.motion(li, cur.state) {
                let direct = matches!(&m, Motion::BlendTree(b) if matches!(b.algorithm, BlendAlgorithm::Direct { .. }));
                let mut ids = Vec::new();
                machine::clip_ids(&m, &mut ids);
                if direct && !wd {
                    let mut ls = Vec::new();
                    rt.state_leaves(li, cur.state, &mut ls);
                    let sum: f32 = ls.iter().map(|l| l.weight).sum();
                    for &t in bound.iter().filter(|&&t| !targets[t as usize].rests_at_default() && !targets[t as usize].discrete()) {
                        let covered: f32 = ls.iter().filter(|l| clips[l.clip as usize].variant(l.mirror).writes.binary_search(&t).is_ok()).map(|l| l.weight).sum();
                        let own = pose.get(&t).copied().unwrap_or(0.0);
                        pose.insert(t, own + (sum - covered) * held(t) + (1.0 - sum).max(0.0) * defaults[t as usize]);
                    }
                }
                ids.sort_by_key(|&c| !(clips[c as usize].length > 0.0));
                for c in ids {
                    let lasting = clips[c as usize].length > 0.0;
                    for &t in &clips[c as usize].writes {
                        if !targets[t as usize].rests_at_default() {
                            let shown = !matches!(targets[t as usize], Target::AnimatorParameter(_) | Target::Slot(..));
                            pose.entry(t).or_insert_with(|| if !wd {
                                held(t)
                            } else if !lasting && shown {
                                beneath(t)
                            } else if lag.is_some() {
                                defaults[t as usize]
                            } else if direct {
                                reset(t)
                            } else {
                                through(t)
                            });
                        }
                    }
                }
            }
            let mut v: Vec<(u32, f32, f32)> = pose.into_iter().map(|(t, v)| (t, v, if uncovered.contains(&t) { 0.0 } else { 1.0 })).collect();
            v.sort_by_key(|x| x.0);
            v
        }
        (None, None) => vec![],
        (Some(a), None) => {
            let w = a.progress();
            let (mut from, from_wd): (HashMap<u32, (f32, f32)>, bool) = match &a.from {
                From::State(p) => (
                    {
                        let uncovered = uncovered(rt, li, p.state, clips, targets);
                        animated(playing_pose(rt, li, p, clips, defaults, false)).into_iter().map(|(t, v)| (t, (v, if uncovered.contains(&t) { 0.0 } else { 1.0 }))).collect()
                    },
                    rt.state(li, p.state).is_some_and(|s| s.write_defaults),
                ),
                From::Frozen(v) => (v.iter().map(|&(t, v, c)| (t, (v, c))).collect(), true),
            };
            let mut to = animated(playing_pose(rt, li, &a.to, clips, defaults, false));
            // Measured only in Action: corpus 4 to 6 entering an AFK pose turn over there, while corpus 4 leaving an emote
            // (the source alone opposite) and the composition probe's mirrored goals in Base keep their sign. No trace
            // shows another playable, an interrupted crossfade or a generic rotation.
            if turns_over && matches!(a.from, From::State(_)) {
                for q in quats.iter().filter(|q| matches!(targets[q[0] as usize], Target::Muscle(_))) {
                    let (Some(src), Some(dst)) = (whole(q.map(|t| from.get(&t).map(|s| s.0))), whole(q.map(|t| to.get(&t).copied()))) else { continue };
                    let under = q.map(|t| beneath(t));
                    if motion::opposed(src, under) && motion::opposed(dst, under) {
                        q.iter().for_each(|t| from.get_mut(t).unwrap().0 *= -1.0);
                    }
                }
            }
            let to_wd = rt.state(li, a.to.state).is_some_and(|s| s.write_defaults);
            let source = |t: u32| {
                from.get(&t).copied().or_else(|| {
                    if from_wd && to_wd {
                        Some((defaults[t as usize], 1.0))
                    } else {
                        (!targets[t as usize].rests_at_default()).then(|| (beneath(t), 1.0))
                    }
                })
            };
            let leaves = |state: crate::model::StateId| {
                let mut ls = Vec::new();
                rt.state_leaves(li, state, &mut ls);
                ls.retain(|l| l.weight != 0.0);
                ls
            };
            let (from_leaves, to_leaves) = (match &a.from { From::State(p) => leaves(p.state), From::Frozen(_) => vec![] }, leaves(a.to.state));
            let keyed_in_part = |ls: &[Leaf], q: &[u32; 4]| ls.iter().any(|l| clips[l.clip as usize].variant(l.mirror).keys_in_part(q));
            let mut yielding = HashSet::new();
            for q in quats {
                let (Some(over), Some(under)) = (whole(q.map(|t| to.get(&t).copied())), whole(q.map(|t| source(t).map(|s| s.0)))) else { continue };
                if !motion::opposed(over, under) {
                    continue;
                }
                if !turns_over && matches!(targets[q[0] as usize], Target::Muscle(_)) && keyed_in_part(&from_leaves, q) && !keyed_in_part(&to_leaves, q) {
                    yielding.extend(q.iter().copied());
                } else {
                    q.iter().for_each(|t| *to.get_mut(t).unwrap() *= -1.0);
                }
            }
            let mut out = Vec::new();
            let written: Vec<u32> = if from_wd && to_wd {
                let mut w: Vec<u32> = from.keys().chain(to.keys()).copied().collect();
                w.sort_unstable();
                w.dedup();
                w
            } else {
                bound.to_vec()
            };
            for t in written {
                let (mut sum, mut cov) = (0.0, 0.0);
                let f = source(t);
                if let Some((v, c)) = f {
                    sum += if yielding.contains(&t) { -v } else { v } * c * (1.0 - w);
                    cov += c * (1.0 - w);
                }
                let d = to.get(&t).copied().or_else(|| {
                    if to_wd {
                        Some(defaults[t as usize])
                    } else {
                        (!targets[t as usize].rests_at_default()).then(|| beneath(t))
                    }
                });
                if let Some(v) = d {
                    sum += v * w;
                    cov += w;
                }
                if f.is_none() && d.is_none() {
                    continue;
                }
                if targets[t as usize].discrete() {
                    let v = match (f, d) {
                        (Some((v, _)), None) => v,
                        (None, Some(v)) => v,
                        (Some((a, _)), Some(b)) => if w >= 0.5 { b } else { a },
                        (None, None) => unreachable!(),
                    };
                    out.push((t, v, cov));
                    continue;
                }
                if cov > 0.0 {
                    out.push((t, sum / cov, cov));
                } else if let Some((v, _)) = f {
                    out.push((t, v, 0.0));
                }
            }
            out
        }
    }
}

/// A quaternion's four components, when all are present.
fn whole(q: [Option<f32>; 4]) -> Option<[f32; 4]> {
    Some([q[0]?, q[1]?, q[2]?, q[3]?])
}
