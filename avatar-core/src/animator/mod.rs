//! VRChat's playable layers and parameters: Unity's state machines, blend trees and clip sampling, the
//! expression and built-in parameters, and the state behaviours, run as VRChat runs them.

mod face;
mod machine;
pub mod motion;
mod random;

use std::collections::HashMap;


use crate::model::{
    Behaviour, BodyPart, ControlledLayer, DriverEntry, LayerBlending, MaterialProperty, ParameterValue,
    PlayableLayerType, TrackingType,
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
    kind: PlayableLayerType,
    rt: ControllerRt,
    weight: f32,
    blend: Option<Blend>,
    layer_blends: HashMap<usize, Blend>,
    /// Muscles the mask lets through; None passes everything.
    muscles: Option<Vec<BodyPart>>,
    bound: Vec<u32>,
    layer_masks: Vec<Option<Mask>>,
}

#[derive(Clone)]
struct Mask {
    parts: Vec<BodyPart>,
    nodes: Option<HashMap<u32, bool>>,
}

struct Accum {
    sum: Vec<f32>,
    cov: Vec<f32>,
    touched: Vec<bool>,
    held: Vec<bool>,
}

/// The parts a tracking-control behaviour sets, in the order of its fields.
pub const TRACKED_PARTS: [&str; 10] =
    ["head", "leftHand", "rightHand", "hip", "leftFoot", "rightFoot", "leftFingers", "rightFingers", "eyes", "mouth"];
const EYES: usize = 8;
const MOUTH: usize = 9;

/// A temporary pose space waiting out its delays: the SDK's own (seconds, or the state's normalized time), then
/// the emulator's, which waits `delay_time` seconds again once the SDK applies it.
struct PoseSpace {
    enter: bool,
    playable: usize,
    layer: usize,
    state: crate::model::StateId,
    sdk_wait: f32,
    then_wait: f32,
    waited: f32,
    applied: bool,
}

pub struct Animator {
    targets: Targets,
    /// Whether the target keeps its value into the next frame; one a transition last pulled to defaults starts the
    /// frame at its default.
    held: Vec<bool>,
    /// Whether some clip has a curve for the target, rather than only filling it beside an animated component.
    curved: Vec<bool>,
    /// What tracking-control behaviours last set for each of [`TRACKED_PARTS`]; NoChange until one does.
    pub tracking: [TrackingType; 10],
    pub locomotion_disabled: bool,
    /// The view position in the avatar's space, moved by temporary pose spaces.
    pub view: glam::Vec3,
    default_view: glam::Vec3,
    /// The view position in the head bone's rest space.
    head_view: Option<glam::Vec3>,
    /// The head bone's pose in the avatar's space after this frame's animation, from the host.
    head: Option<crate::physbones::Transform>,
    pose_spaces: Vec<PoseSpace>,
    clips: Vec<CompiledClip>,
    defaults: Vec<f32>,
    values: Vec<f32>,
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
    blink: f32,
    look_y: f32,
}

/// A target's value as the avatar was authored, which it keeps while nothing animates it.
pub fn authored(avatar: &Avatar, t: &Target) -> f32 {
    let rest = crate::physbones::Nodes::read(avatar).map(|n| n.rest).unwrap_or_default();
    default_of(avatar, t, &rest)
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
            for mat in &r.materials {
                if let Some(p) = m.materials.get(mat.0 as usize).and_then(|x| x.properties.get(base)) {
                    let i = comp.map(|c| "rgbaxyzw".find(c).unwrap() % 4);
                    return match (p, i) {
                        (MaterialProperty::Float(v), _) => *v,
                        (MaterialProperty::Int(v), _) => *v as f32,
                        (MaterialProperty::Color(v) | MaterialProperty::Vector(v), Some(i)) => v[i],
                        (MaterialProperty::Color(v) | MaterialProperty::Vector(v), None) => v[0],
                        (MaterialProperty::Texture(_), _) => 0.0,
                    };
                }
            }
            0.0
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
        Target::PhysBone(i, f) if f == "m_Enabled" => {
            m.physbones.get(*i as usize).map_or(1.0, |p| p.enabled as u8 as f32)
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

/// The humanoid body part a Unity muscle or root curve belongs to.
pub fn muscle_part(name: &str) -> BodyPart {
    if name.starts_with("Root") || name.starts_with("Motion") {
        return BodyPart::Root;
    }
    for (prefix, part) in [
        ("LeftFootT", BodyPart::LeftFootIK),
        ("LeftFootQ", BodyPart::LeftFootIK),
        ("RightFootT", BodyPart::RightFootIK),
        ("RightFootQ", BodyPart::RightFootIK),
        ("LeftHandT", BodyPart::LeftHandIK),
        ("LeftHandQ", BodyPart::LeftHandIK),
        ("RightHandT", BodyPart::RightHandIK),
        ("RightHandQ", BodyPart::RightHandIK),
        ("LeftHand.", BodyPart::LeftFingers),
        ("RightHand.", BodyPart::RightFingers),
    ] {
        if name.starts_with(prefix) {
            return part;
        }
    }
    let lower = name.to_ascii_lowercase();
    if lower.starts_with("left") {
        if ["upper leg", "lower leg", "foot", "toes"].iter().any(|k| lower.contains(k)) {
            return BodyPart::LeftLeg;
        }
        return BodyPart::LeftArm;
    }
    if lower.starts_with("right") {
        if ["upper leg", "lower leg", "foot", "toes"].iter().any(|k| lower.contains(k)) {
            return BodyPart::RightLeg;
        }
        return BodyPart::RightArm;
    }
    if ["neck", "head", "eye", "jaw"].iter().any(|k| lower.contains(k)) {
        return BodyPart::Head;
    }
    BodyPart::Body
}

impl Animator {
    pub fn new(avatar: &Avatar) -> Animator {
        let m = &avatar.model;
        let mut targets = Targets::default();
        let clips: Vec<CompiledClip> = m.clips.iter().map(|c| CompiledClip::compile(c, &mut targets)).collect();
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
        let mut playables = Vec::new();
        for kind in PLAYABLE_ORDER {
            let Some(layer) = m.layers.get(&kind) else { continue };
            let Some(c) = &layer.controller else { continue };
            let rt = ControllerRt::new(c, &clips, &anim_params);
            let mut bound: Vec<u32> = rt.clip_ids().iter().flat_map(|&c| clips[c as usize].writes.iter().copied()).collect();
            bound.sort_unstable();
            bound.dedup();
            let muscles = layer.mask.as_ref().map(|mk| mk.body_parts.iter().copied().collect());
            let layer_masks = c
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
            playables.push(Playable {
                kind,
                rt,
                weight,
                blend: None,
                layer_blends: HashMap::new(),
                muscles,
                bound,
                layer_masks,
            });
        }
        let rest = crate::physbones::Nodes::read(avatar).map(|n| n.rest).unwrap_or_default();
        let defaults: Vec<f32> = targets.list.iter().map(|t| default_of(avatar, t, &rest)).collect();

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
            chain.pop();
            let rest = chain.iter().rev().fold(crate::physbones::Transform::IDENTITY, |t, &n| t.mul(&nodes.rest[n]));
            Some(rest.rotation.inverse() * (view - rest.translation))
        });
        let mut curved = vec![false; targets.list.len()];
        for c in &clips {
            for &t in c.writes.iter().filter(|t| !c.fill.contains(t)) {
                curved[t as usize] = true;
            }
        }
        let mut a = Animator {
            held: vec![true; curved.len()],
            curved,
            tracking: [TrackingType::NoChange; 10],
            locomotion_disabled: false,
            view,
            default_view: view,
            head_view,
            head: None,
            pose_spaces: Vec::new(),
            values: defaults.clone(),
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
            blink: 0.0,
            look_y: 0.0,
        };
        a.set_eye_height(avatar);
        a
    }

    fn set_eye_height(&mut self, avatar: &Avatar) {
        let y = avatar.model.descriptor.view_position[1];
        self.builtins.insert("EyeHeightAsMeters".into(), y);
        self.builtins.insert("EyeHeightAsPercent".into(), ((y - 0.2) / (5.0 - 0.2)).clamp(0.0, 1.0));
        self.normalized = self.builtins.clone();
    }

    pub fn targets(&self) -> &[Target] {
        &self.targets.list
    }

    pub fn values(&self) -> &[f32] {
        &self.values
    }

    pub fn defaults(&self) -> &[f32] {
        &self.defaults
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
        match self.builtins.get_mut(name) {
            Some(v) => {
                *v = value;
                true
            }
            None => false,
        }
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

    /// Each playable layer and its controller layers' states this frame: (playable index, layer index, layer
    /// name, state, the state it is transitioning to).
    pub fn states(&self) -> Vec<(usize, usize, String, String, Option<String>)> {
        let mut out = Vec::new();
        for (pi, p) in self.playables.iter().enumerate() {
            for (li, l) in p.rt.layers.iter().enumerate() {
                let name = |s| p.rt.names.get(&s).cloned().unwrap_or_default();
                let (cur, next) = match &l.active {
                    Some(a) => (
                        match &a.from {
                            From::State(s) => name(s.state),
                            From::Frozen(_) => l.current.as_ref().map(|c| name(c.state)).unwrap_or_default(),
                        },
                        Some(name(a.to.state)),
                    ),
                    None => (l.current.as_ref().map_or_else(|| "#00000000".to_string(), |c| name(c.state)), None),
                };
                out.push((pi, li, p.rt.layer_defs[li].name.clone(), cur, next));
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
        let v = b["VelocityX"].powi(2) + b["VelocityY"].powi(2) + b["VelocityZ"].powi(2);
        b.insert("VelocityMagnitude".into(), v.sqrt());
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
        self.advance_pose_spaces(dt);
        self.time += dt as f64;
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

        let prev = self.values.clone();
        let clips = std::mem::take(&mut self.clips);
        for pi in 0..self.playables.len() {
            let mut entered = Vec::new();
            {
                let defaults = &self.defaults;
                let prev_ref = &prev;
                let clips_ref = &clips;
                let bound = self.playables[pi].bound.clone();
                let targets = &self.targets.list;
                let beneath = |t: u32| prev_ref[t as usize];
                let frozen = |rt: &ControllerRt, li: usize| layer_pose(rt, li, clips_ref, defaults, &bound, targets, &beneath, &beneath);
                self.playables[pi].rt.update(dt, &clips, &mut entered, &frozen);
            }
            for e in entered {
                self.run_behaviours(pi, e, &clips);
            }
        }
        let n = self.targets.len();
        let cov: Vec<f32> = (0..n).map(|i| (!self.targets.list[i].rests_at_default() || self.curved[i]) as u8 as f32).collect();
        let sum = (0..n).map(|i| cov[i] * if self.held[i] { prev[i] } else { self.defaults[i] }).collect();
        let mut acc = Accum { sum, cov, touched: vec![false; n], held: self.held.clone() };
        for pi in 0..self.playables.len() {
            let params = self.evaluate(pi, &clips, &prev, &mut acc);
            let p = &mut self.playables[pi];
            if p.weight > 0.0 {
                for (i, v) in params {
                    let k = p.rt.params[i].kind;
                    p.rt.params[i].value = Self::convert(k, v);
                }
            }
        }
        let mut out = prev.clone();
        for i in 0..n {
            if self.targets.list[i].discrete() {
                out[i] = acc.sum[i];
                continue;
            }
            out[i] = acc.sum[i] + (1.0 - acc.cov[i]) * self.defaults[i];
        }
        if self.tracking[MOUTH] != TrackingType::Animation {
            self.face.speak(self.builtins["Viseme"].max(0.0) as usize, &mut out);
        }
        if self.tracking[EYES] != TrackingType::Animation {
            self.face.look(self.blink, self.look_y, &mut out);
        }
        self.clips = clips;
        self.values = out;
        self.head = None;
        self.held = acc.held;
    }

    /// Blends one playable's layers into the frame's accumulation, returning the values its animator-parameter
    /// curves give its own parameters.
    fn evaluate(&self, pi: usize, clips: &[CompiledClip], prev: &[f32], acc: &mut Accum) -> Vec<(usize, f32)> {
        let p = &self.playables[pi];
        let mut own: HashMap<u32, (f32, f32)> = HashMap::new();
        for li in 0..p.rt.layers.len() {
            let wl = if li == 0 { 1.0 } else { p.rt.layers[li].weight } * p.weight;
            if wl <= 0.0 {
                continue;
            }
            let additive = p.kind == PlayableLayerType::Additive || p.rt.layer_defs[li].blending == LayerBlending::Additive;
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
            let held = |t: u32| prev[t as usize];
            let mask = &p.layer_masks[li];
            let passes = |i: usize| -> bool {
                if let (Target::Muscle(name), Some(parts)) = (&self.targets.list[i], &p.muscles) {
                    if !parts.contains(&muscle_part(name)) {
                        return false;
                    }
                }
                let Some(mk) = mask else { return true };
                match &self.targets.list[i] {
                    Target::Muscle(name) => mk.parts.contains(&muscle_part(name)),
                    other => match (&mk.nodes, node_of(other)) {
                        (Some(nodes), Some(n)) => nodes.get(&n).copied().unwrap_or(false),
                        _ => true,
                    },
                }
            };
            if additive {
                let (reference, now): (HashMap<u32, f32>, HashMap<u32, (f32, f32)>) = match &p.rt.layers[li].current {
                    Some(c) => (
                        motion_pose(&p.rt, li, c.state, 0.0, clips, &self.defaults).into_iter().map(|(t, (v, _))| (t, v)).collect(),
                        motion_pose(&p.rt, li, c.state, c.time, clips, &self.defaults).into_iter().map(|(t, (v, _))| (t, (v, 1.0))).collect(),
                    ),
                    None => (HashMap::new(), HashMap::new()),
                };
                for &t in &p.bound {
                    let i = t as usize;
                    if !passes(i) || self.targets.list[i].discrete() {
                        continue;
                    }
                    match now.get(&t) {
                        Some(&(v, c)) => {
                            let r = reference.get(&t).copied().unwrap_or(self.defaults[i]);
                            acc.sum[i] += (v - r) * wl * c;
                            acc.touched[i] = true;
                        }
                        None => {
                            if acc.cov[i] > 0.0 {
                                acc.sum[i] += (self.defaults[i] - self.targets.list[i].identity()) * wl * acc.cov[i];
                            }
                        }
                    }
                }
                continue;
            }
            let pose = layer_pose(&p.rt, li, clips, &self.defaults, &p.bound, &self.targets.list, &beneath, &held);
            let pulled = pulled(&p.rt, li, clips, &self.defaults);
            for (t, v, c) in pose {
                let i = t as usize;
                if !passes(i) {
                    continue;
                }
                let a = wl * c;
                if let Target::AnimatorParameter(_) = &self.targets.list[i] {
                    let e = own.entry(t).or_insert((0.0, 0.0));
                    e.0 = e.0 * (1.0 - a) + v * a;
                    e.1 = e.1 * (1.0 - a) + a;
                    continue;
                }
                if self.targets.list[i].discrete() {
                    if a >= 0.5 {
                        acc.sum[i] = v;
                        acc.touched[i] = true;
                    }
                    continue;
                }
                acc.sum[i] = acc.sum[i] * (1.0 - a) + v * a;
                acc.cov[i] = acc.cov[i] * (1.0 - a) + a;
                acc.touched[i] = true;
                if a > 0.0 {
                    acc.held[i] = !pulled.contains(&t);
                }
            }
        }
        own.into_iter()
            .filter_map(|(t, (sum, cov))| {
                let Target::AnimatorParameter(name) = &self.targets.list[t as usize] else { return None };
                let &i = p.rt.index.get(name)?;
                Some((i, sum + (1.0 - cov) * p.rt.params[i].value))
            })
            .collect()
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
                    let sdk_wait = if fixed_delay { delay_time } else { delay_time * p.rt.duration(e.layer, e.state, clips) };
                    self.pose_spaces.retain(|s| !(s.playable == pi && s.layer == e.layer));
                    self.pose_spaces.push(PoseSpace {
                        enter: enter_pose_space,
                        playable: pi,
                        layer: e.layer,
                        state: e.state,
                        sdk_wait,
                        then_wait: delay_time,
                        waited: 0.0,
                        applied: sdk_wait <= 0.0,
                    });
                }
            }
        }
    }

    fn advance_pose_spaces(&mut self, dt: f32) {
        let mut keep = Vec::new();
        for mut s in std::mem::take(&mut self.pose_spaces) {
            let l = &self.playables[s.playable].rt.layers[s.layer];
            let here = match &l.active {
                Some(a) => a.to.state == s.state,
                None => l.current.as_ref().is_some_and(|c| c.state == s.state),
            };
            if !s.applied && !here {
                continue;
            }
            s.waited += dt;
            if !s.applied {
                if s.waited >= s.sdk_wait {
                    s.applied = true;
                    s.waited = 0.0;
                }
                keep.push(s);
                continue;
            }
            if s.waited < s.then_wait {
                keep.push(s);
                continue;
            }
            self.view = match (s.enter, self.head, self.head_view) {
                (true, Some(h), Some(v)) => h.point(v),
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

/// The targets a layer's transition between two write-defaults states pulls to the destination's defaults:
/// those the source animates and the destination does not. Nothing holds them once it ends.
fn pulled(rt: &ControllerRt, li: usize, clips: &[CompiledClip], defaults: &[f32]) -> Vec<u32> {
    let l = &rt.layers[li];
    let Some(a) = &l.active else { return vec![] };
    let From::State(p) = &a.from else { return vec![] };
    let wd = |s: crate::model::StateId| rt.state(li, s).is_some_and(|s| s.write_defaults);
    if !wd(p.state) || !wd(a.to.state) {
        return vec![];
    }
    let to = motion_pose(rt, li, a.to.state, a.to.time, clips, defaults);
    motion_pose(rt, li, p.state, p.time, clips, defaults).into_keys().filter(|t| !to.contains_key(t)).collect()
}

/// One motion at one normalized time: the value of every target it writes, and whether only an animated
/// transform's undriven components put it there.
fn motion_pose(rt: &ControllerRt, layer: usize, state: crate::model::StateId, time: f32, clips: &[CompiledClip], defaults: &[f32]) -> HashMap<u32, (f32, bool)> {
    let mut ls: Vec<Leaf> = Vec::new();
    rt.state_leaves(layer, state, &mut ls);
    let offset = rt.cycle_offset(layer, state);
    let mut acc: HashMap<u32, (f32, bool)> = HashMap::new();
    let direct = ls.iter().any(|l| l.direct);
    for l in &ls {
        let c = &clips[l.clip as usize];
        for &t in &c.writes {
            let filled = c.fill.contains(&t);
            let e = acc.entry(t).or_insert((0.0, true));
            e.1 &= filled;
        }
    }
    for l in &ls {
        let c = &clips[l.clip as usize];
        let mut seen: std::collections::HashSet<u32> = std::collections::HashSet::new();
        c.sample(time + offset + l.cycle_offset, defaults, &mut |t, v| {
            acc.get_mut(&t).unwrap().0 += l.weight * v;
            seen.insert(t);
        });
        if !direct {
            for (t, e) in acc.iter_mut() {
                if !seen.contains(t) {
                    e.0 += l.weight * defaults[*t as usize];
                }
            }
        }
    }
    acc
}

/// A layer's pose this frame: each target it writes, with how much of it the layer covers. Outside a
/// transition a state covers what its motion animates. During one, the source covers by the remaining weight
/// what it animates, and without write defaults also every other bound target but transforms at the value
/// beneath; the destination covers by the transition's weight what it animates, and under write defaults every
/// other bound target at its rest value.
#[allow(clippy::too_many_arguments)]
fn layer_pose(rt: &ControllerRt, li: usize, clips: &[CompiledClip], defaults: &[f32], bound: &[u32], targets: &[Target], beneath: &dyn Fn(u32) -> f32, held: &dyn Fn(u32) -> f32) -> Vec<(u32, f32, f32)> {
    let l = &rt.layers[li];
    let Some(cur) = &l.current else { return vec![] };
    let steady = match &l.active {
        Some(a) if a.progress() > 0.0 => None,
        Some(a) => match &a.from {
            From::State(p) => Some(p),
            From::Frozen(v) => return v.clone(),
        },
        None => Some(cur),
    };
    let animated = |m: HashMap<u32, (f32, bool)>| -> HashMap<u32, f32> { m.into_iter().filter(|(_, (_, f))| !f).map(|(t, (v, _))| (t, v)).collect() };
    match (&l.active, steady) {
        (_, Some(cur)) => {
            let mut pose = animated(motion_pose(rt, li, cur.state, cur.time, clips, defaults));
            if rt.state(li, cur.state).is_some_and(|s| s.write_defaults) {
                for &t in bound.iter().filter(|&&t| targets[t as usize].discrete()) {
                    pose.entry(t).or_insert(defaults[t as usize]);
                }
            }
            if let Some(m) = rt.motion(li, cur.state) {
                let mut ids = Vec::new();
                machine::clip_ids(&m, &mut ids);
                for c in ids {
                    for &t in &clips[c as usize].writes {
                        if !targets[t as usize].rests_at_default() {
                            pose.entry(t).or_insert_with(|| held(t));
                        }
                    }
                }
            }
            let mut v: Vec<(u32, f32, f32)> = pose.into_iter().map(|(t, v)| (t, v, 1.0)).collect();
            v.sort_by_key(|x| x.0);
            v
        }
        (None, None) => vec![],
        (Some(a), None) => {
            let w = a.progress();
            let (from, from_wd): (HashMap<u32, (f32, f32)>, bool) = match &a.from {
                From::State(p) => (
                    animated(motion_pose(rt, li, p.state, p.time, clips, defaults)).into_iter().map(|(t, v)| (t, (v, 1.0))).collect(),
                    rt.state(li, p.state).is_some_and(|s| s.write_defaults),
                ),
                From::Frozen(v) => (v.iter().map(|&(t, v, c)| (t, (v, c))).collect(), true),
            };
            let to = animated(motion_pose(rt, li, a.to.state, a.to.time, clips, defaults));
            let to_wd = rt.state(li, a.to.state).is_some_and(|s| s.write_defaults);
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
                let f = from.get(&t).copied().or_else(|| {
                    if from_wd && to_wd {
                        Some((defaults[t as usize], 1.0))
                    } else {
                        (!targets[t as usize].rests_at_default()).then(|| (beneath(t), 1.0))
                    }
                });
                if let Some((v, c)) = f {
                    sum += v * c * (1.0 - w);
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
