use glam::{Quat, Vec3, Vec4};
use serde_json::Value;

use crate::model::{self, ColliderShape, Curved, ImmobileType, IntegrationType, Limit, MultiChildType, PhysBoneVersion};
use crate::Avatar;

/// The simulation step: PhysBones advance in steps of this length whatever the host's frame rate.
pub const STEP: f32 = 1.0 / 60.0;

/// A host gap longer than this (a pause, a suspend) restarts the chains from the current frame instead of
/// stepping through it.
const MAX_CATCH_UP: f64 = 0.25;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub translation: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
}

impl Transform {
    pub const IDENTITY: Transform = Transform { translation: Vec3::ZERO, rotation: Quat::IDENTITY, scale: Vec3::ONE };

    pub fn mul(&self, child: &Transform) -> Transform {
        Transform {
            translation: self.point(child.translation),
            rotation: (self.rotation * child.rotation).normalize(),
            scale: self.scale * child.scale,
        }
    }

    pub fn point(&self, local: Vec3) -> Vec3 {
        self.translation + self.rotation * (self.scale * local)
    }

    fn rigid(&self) -> Transform {
        Transform { scale: Vec3::ONE, ..*self }
    }

    fn inverse_rigid(&self) -> Transform {
        let rotation = self.rotation.inverse();
        Transform { translation: rotation * -self.translation, rotation, scale: Vec3::ONE }
    }

    fn lerp(&self, to: &Transform, s: f32) -> Transform {
        Transform {
            translation: self.translation.lerp(to.translation, s),
            rotation: slerp(self.rotation, to.rotation, s),
            scale: self.scale.lerp(to.scale, s),
        }
    }
}

/// A rotation normalized, or none for a zero quaternion.
fn unit(q: Quat) -> Quat {
    if q.length_squared() > 0.0 { q.normalize() } else { Quat::IDENTITY }
}

fn slerp(from: Quat, to: Quat, s: f32) -> Quat {
    let to = if from.dot(to) < 0.0 { -to } else { to };
    let (axis, angle) = (from.inverse() * to).to_axis_angle();
    if !angle.is_finite() || angle.abs() < 1e-7 {
        return from.lerp(to, s).normalize();
    }
    (from * Quat::from_axis_angle(axis, angle * s)).normalize()
}

/// What the host gives the core each frame, in the avatar's space: glTF axes, metres.
#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    /// The avatar root in the world. Its scale is not read: `nodes` carry the avatar's scale.
    pub root: Transform,
    /// Every node's pose relative to the avatar root, scale included, after animation and IK and before
    /// PhysBones. The core reads it for each PhysBone root's parent and each collider's node.
    pub nodes: Vec<Transform>,
    /// Every node's local pose as animation left it. The core reads it for the bones it simulates.
    pub locals: Vec<Transform>,
    /// Every node's own active flag as animation left it. A PhysBone runs while its object is active in
    /// the hierarchy and the component is enabled.
    pub active: Vec<bool>,
    /// Every PhysBone's enabled flag as animation left it.
    pub enabled: Vec<bool>,
}

/// The `{parameter}_…` values VRChat derives from a PhysBone.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Parameters {
    pub is_grabbed: bool,
    pub is_posed: bool,
    pub angle: f32,
    pub stretch: f32,
    pub squish: f32,
}

#[derive(Clone, Copy, Debug)]
struct Params {
    pull: f32,
    spring: f32,
    stiffness: f32,
    gravity: f32,
    gravity_falloff: f32,
    immobile: f32,
    radius: f32,
    max_stretch: f32,
    stretch_motion: f32,
    max_squish: f32,
    limit: LimitAt,
}

#[derive(Clone, Copy, Debug)]
enum LimitAt {
    None,
    Angle { max: f32, rotation: Quat },
    Hinge { max: f32, rotation: Quat },
    Polar { max_x: f32, max_z: f32, rotation: Quat },
}

#[derive(Clone, Debug)]
struct Bone {
    /// None for a chain end placed at the endpoint position.
    node: Option<usize>,
    /// The authored local pose, the rest of a chain that is not animated.
    authored: Transform,
    parent: Option<usize>,
    children: Vec<usize>,
    params: Params,
    position: Vec3,
    previous: Vec3,
}

#[derive(Clone, Debug)]
struct Chain {
    /// The component's node and its ancestors.
    lineage: Vec<usize>,
    running: bool,
    reset_when_disabled: bool,
    version: PhysBoneVersion,
    integration: IntegrationType,
    immobile_type: ImmobileType,
    multi_child: MultiChildType,
    animated: bool,
    root_parent: Option<usize>,
    colliders: Vec<usize>,
    bones: Vec<Bone>,
    reference: Option<Transform>,
    rest: Vec<Transform>,
    world: Vec<Transform>,
    /// Each bone's final pose after the previous step.
    settled: Vec<Transform>,
    /// The bones a hand holds this step: the grabbed one and its ancestors below the root.
    held: Vec<bool>,
    parameters: Parameters,
    grab: Option<Grab>,
    posed: Option<Vec<Quat>>,
}

#[derive(Clone, Copy, Debug)]
struct Grab {
    bone: usize,
    target: Vec3,
    offset: Vec3,
    movement: f32,
}

#[derive(Clone, Debug)]
struct ColliderAt {
    node: usize,
    shape: ColliderShape,
    position: Vec3,
    rotation: Quat,
}

/// Who touches a PhysBone, for its collision, grab and pose filters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Toucher {
    Wearer,
    Other,
}

/// VRChat PhysBones for one avatar, stepped at [`STEP`].
#[derive(Clone, Debug)]
pub struct PhysBones {
    chains: Vec<Chain>,
    colliders: Vec<ColliderAt>,
    model: Vec<model::PhysBone>,
    /// The host's latest frame and its time.
    last: Option<(f64, Frame)>,
    now: f64,
    /// The time of the last simulated step, at most one step past `now`.
    stepped: f64,
    before: Vec<Vec<Transform>>,
    after: Vec<Vec<Transform>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Error {
    Hierarchy(&'static str),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Hierarchy(what) => write!(f, "glTF node hierarchy: {what}"),
        }
    }
}

impl std::error::Error for Error {}

/// The glTF node tree: each node's children and its rest pose.
#[derive(Clone, Debug)]
pub struct Nodes {
    pub children: Vec<Vec<usize>>,
    pub parent: Vec<Option<usize>>,
    pub rest: Vec<Transform>,
}

impl Nodes {
    pub fn read(avatar: &Avatar) -> Result<Nodes, Error> {
        let nodes = avatar.gltf.get("nodes").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]);
        let count = nodes.len();
        let mut children = vec![vec![]; count];
        let mut parent = vec![None; count];
        let mut rest = vec![Transform::IDENTITY; count];
        let floats = |v: Option<&Value>, n: usize| -> Option<Vec<f32>> {
            let a = v?.as_array()?;
            (a.len() == n).then(|| a.iter().map(|x| x.as_f64().unwrap_or(0.0) as f32).collect())
        };
        for (i, node) in nodes.iter().enumerate() {
            for c in node.get("children").and_then(Value::as_array).into_iter().flatten() {
                let c = c.as_u64().map(|c| c as usize).filter(|&c| c < count && c != i).ok_or(Error::Hierarchy("a child index does not resolve"))?;
                if parent[c].replace(i).is_some() {
                    return Err(Error::Hierarchy("a node has two parents"));
                }
                children[i].push(c);
            }
            if let Some(t) = floats(node.get("translation"), 3) {
                rest[i].translation = Vec3::new(t[0], t[1], t[2]);
            }
            if let Some(r) = floats(node.get("rotation"), 4) {
                rest[i].rotation = unit(Quat::from_xyzw(r[0], r[1], r[2], r[3]));
            }
            if let Some(s) = floats(node.get("scale"), 3) {
                rest[i].scale = Vec3::new(s[0], s[1], s[2]);
            }
            if let Some(m) = floats(node.get("matrix"), 16) {
                let (scale, rotation, translation) = glam::Mat4::from_cols_slice(&m).to_scale_rotation_translation();
                rest[i] = Transform { translation, rotation: unit(rotation), scale };
            }
        }
        for start in 0..count {
            let (mut at, mut depth) = (start, 0);
            while let Some(p) = parent[at] {
                at = p;
                depth += 1;
                if depth > count {
                    return Err(Error::Hierarchy("the parent links form a cycle"));
                }
            }
        }
        Ok(Nodes { children, parent, rest })
    }
}

fn at(field: &Curved, t: f32) -> f32 {
    field.value * field.curve.as_ref().map_or(1.0, |c| c.evaluate(t))
}

fn limit_at(limit: &Limit, t: f32) -> LimitAt {
    let rotation = |r: &model::LimitRotation| {
        let k = |c: &Option<model::AnimationCurve>| c.as_ref().map_or(1.0, |c| c.evaluate(t));
        let e = Vec3::from(r.value) * Vec3::new(k(&r.x_curve), k(&r.y_curve), k(&r.z_curve));
        Quat::from_rotation_z(e.z.to_radians()) * Quat::from_rotation_y(e.y.to_radians()) * Quat::from_rotation_x(e.x.to_radians())
    };
    match limit {
        Limit::None {} => LimitAt::None,
        Limit::Angle { max_angle_x, rotation: r } => LimitAt::Angle { max: at(max_angle_x, t).to_radians(), rotation: rotation(r) },
        Limit::Hinge { max_angle_x, rotation: r } => LimitAt::Hinge { max: at(max_angle_x, t).to_radians(), rotation: rotation(r) },
        Limit::Polar { max_angle_x, max_angle_z, rotation: r } => LimitAt::Polar {
            max_x: at(max_angle_x, t).to_radians(),
            max_z: at(max_angle_z, t).to_radians(),
            rotation: rotation(r),
        },
    }
}

fn params(pb: &model::PhysBone, t: f32) -> Params {
    Params {
        pull: at(&pb.pull, t).clamp(0.0, 1.0),
        spring: at(&pb.spring, t).clamp(0.0, 1.0),
        stiffness: at(&pb.stiffness, t).clamp(0.0, 1.0),
        gravity: at(&pb.gravity, t).clamp(-1.0, 1.0),
        gravity_falloff: at(&pb.gravity_falloff, t).clamp(0.0, 1.0),
        immobile: at(&pb.immobile, t).clamp(0.0, 1.0),
        radius: at(&pb.radius, t).max(0.0),
        max_stretch: at(&pb.max_stretch, t).max(0.0),
        stretch_motion: at(&pb.stretch_motion, t).clamp(0.0, 1.0),
        max_squish: at(&pb.max_squish, t).clamp(0.0, 1.0),
        limit: limit_at(&pb.limit, t),
    }
}

/// The bones a PhysBone simulates, breadth first from its root, so that a bone's children are solved together.
/// A leaf gets a child at the endpoint position when it is set. Curves are read at each bone's depth, from 0 at
/// the root's children to 1 at the deepest bones.
fn chain(pb: &model::PhysBone, roots: &[usize], nodes: &Nodes) -> Chain {
    let root = pb.root.0 as usize;
    let endpoint = Vec3::from(pb.endpoint_position);
    let fresh = |node: Option<usize>, parent: Option<usize>| Bone {
        node,
        authored: node.map_or(Transform { translation: endpoint, ..Transform::IDENTITY }, |n| nodes.rest[n]),
        parent,
        children: vec![],
        params: params(pb, 0.0),
        position: Vec3::ZERO,
        previous: Vec3::ZERO,
    };
    let mut bones = vec![fresh(Some(root), None)];
    let mut depth = vec![0.0f32];
    let mut next = 0;
    while next < bones.len() {
        let i = next;
        next += 1;
        let Some(node) = bones[i].node else { continue };
        let kids: Vec<Option<usize>> = nodes.children[node]
            .iter()
            .copied()
            .filter(|c| !(pb.ignore_transforms.iter().any(|t| t.0 as usize == *c) || pb.ignore_other_phys_bones && roots.contains(c)))
            .map(Some)
            .collect();
        let kids = if kids.is_empty() && endpoint != Vec3::ZERO { vec![None] } else { kids };
        for c in kids {
            let at = bones.len();
            bones[i].children.push(at);
            bones.push(fresh(c, Some(i)));
            depth.push(depth[i] + 1.0);
        }
    }
    let deepest = depth.iter().copied().fold(0.0f32, f32::max);
    for (bone, d) in bones.iter_mut().zip(&depth) {
        bone.params = params(pb, if deepest > 1.0 { ((d - 1.0) / (deepest - 1.0)).max(0.0) } else { 0.0 });
    }
    let count = bones.len();
    let mut lineage = vec![pb.node.0 as usize];
    while let Some(p) = nodes.parent[*lineage.last().unwrap()] {
        lineage.push(p);
    }
    Chain {
        lineage,
        running: false,
        reset_when_disabled: pb.reset_when_disabled,
        version: pb.version,
        integration: pb.integration_type,
        immobile_type: pb.immobile_type,
        multi_child: pb.multi_child_type,
        animated: pb.is_animated,
        root_parent: nodes.parent[root],
        colliders: pb.colliders.iter().map(|c| c.0 as usize).collect(),
        bones,
        reference: None,
        rest: vec![Transform::IDENTITY; count],
        world: vec![Transform::IDENTITY; count],
        settled: vec![Transform::IDENTITY; count],
        held: vec![false; count],
        parameters: Parameters::default(),
        grab: None,
        posed: None,
    }
}

impl PhysBones {
    pub fn new(avatar: &Avatar) -> Result<PhysBones, Error> {
        let nodes = Nodes::read(avatar)?;
        let model = &avatar.model;
        let roots: Vec<usize> = model.physbones.iter().map(|p| p.root.0 as usize).collect();
        let chains: Vec<Chain> = model.physbones.iter().map(|pb| chain(pb, &roots, &nodes)).collect();
        let colliders = model
            .colliders
            .iter()
            .map(|c| ColliderAt {
                node: c.node.0 as usize,
                shape: c.shape.clone(),
                position: Vec3::from(c.position),
                rotation: unit(Quat::from_array(c.rotation)),
            })
            .collect();
        let authored: Vec<Vec<Transform>> = chains.iter().map(|c| c.bones.iter().filter(|b| b.node.is_some()).map(|b| b.authored).collect()).collect();
        Ok(PhysBones { chains, colliders, model: model.physbones.clone(), last: None, now: 0.0, stepped: 0.0, before: authored.clone(), after: authored })
    }

    /// The nodes each chain poses, in the order [`PhysBones::local`] answers for them.
    pub fn bones(&self, physbone: usize) -> impl Iterator<Item = usize> + '_ {
        self.chains[physbone].bones.iter().filter_map(|b| b.node)
    }

    pub fn parameters(&self, physbone: usize) -> Parameters {
        self.chains[physbone].parameters
    }

    /// Advances the host's clock by `dt` seconds to `frame`, stepping the simulation as many times as it
    /// passed [`STEP`] boundaries. Steps between host frames see the frames interpolated; a step past the
    /// current frame sees it extrapolated from the last two, over at least half a step so a short frame's
    /// jitter is not magnified.
    pub fn advance(&mut self, dt: f32, frame: &Frame) {
        let Some((t0, last)) = self.last.take() else {
            for (i, c) in self.chains.iter_mut().enumerate() {
                c.reset(frame);
                c.running = c.on(frame, i);
            }
            self.after = self.chains.iter().map(|c| c.locals(frame)).collect();
            self.before = self.after.clone();
            self.last = Some((0.0, frame.clone()));
            return;
        };
        let t1 = t0 + dt.max(0.0) as f64;
        self.now = t1;
        if t1 - self.stepped > MAX_CATCH_UP {
            self.last = None;
            (self.now, self.stepped) = (0.0, 0.0);
            return self.advance(0.0, frame);
        }
        while self.stepped < t1 - 1e-9 {
            let time = self.stepped + STEP as f64;
            let s = if t1 > t0 { 1.0 + ((time - t1) / (t1 - t0).max(STEP as f64 / 2.0)) as f32 } else { 1.0 };
            let input = blend(&last, frame, s);
            self.before = self.after.clone();
            for (i, c) in self.chains.iter_mut().enumerate() {
                let on = c.on(&input, i);
                if on {
                    if !c.running {
                        c.reset(&input);
                    }
                    c.step(&input, &self.colliders);
                } else if c.running && c.reset_when_disabled {
                    c.reset(&input);
                } else if !c.running {
                    continue;
                }
                c.running = on;
                self.after[i] = c.locals(&input);
            }
            self.stepped = time;
        }
        self.last = Some((t1, frame.clone()));
    }

    /// Each simulated node's local pose at the host's current time, for `physbone`'s bones in
    /// [`PhysBones::bones`] order.
    pub fn local(&self, physbone: usize) -> Vec<Transform> {
        let s = 1.0 - ((self.stepped - self.now) / STEP as f64) as f32;
        self.before[physbone].iter().zip(&self.after[physbone]).map(|(a, b)| a.lerp(b, s.clamp(0.0, 1.0))).collect()
    }

    /// Grabs `node`, one of `physbone`'s bones below its root, with a hand at `at` (world space) for `who`, if
    /// the grab filter allows. Returns whether it holds. The bone keeps its offset from the hand unless the
    /// component snaps to hands.
    pub fn grab(&mut self, physbone: usize, node: usize, at: Vec3, who: Toucher) -> bool {
        let pb = &self.model[physbone];
        if !allowed(&pb.grabbing, who) {
            return false;
        }
        let chain = &mut self.chains[physbone];
        let Some(bone) = chain.bones.iter().skip(1).position(|b| b.node == Some(node)).map(|i| i + 1) else {
            return false;
        };
        let offset = if pb.snap_to_hand { Vec3::ZERO } else { chain.bones[bone].position - at };
        chain.grab = Some(Grab { bone, target: at, offset, movement: pb.grab_movement });
        chain.parameters.is_grabbed = true;
        true
    }

    /// Moves the hand holding `physbone` to `at` (world space).
    pub fn hold(&mut self, physbone: usize, at: Vec3) {
        if let Some(g) = &mut self.chains[physbone].grab {
            g.target = at;
        }
    }

    /// Lets go of `physbone`; with `pose`, and if its pose filter allows `who`, it keeps the held pose as its rest.
    pub fn release(&mut self, physbone: usize, pose: bool, who: Toucher) {
        let allow = pose && allowed(&self.model[physbone].posing, who);
        let chain = &mut self.chains[physbone];
        if chain.grab.take().is_none() {
            return;
        }
        chain.parameters.is_grabbed = false;
        chain.posed = allow.then(|| chain.pose());
        chain.parameters.is_posed = allow;
    }

    /// Ends a pose kept by [`PhysBones::release`].
    pub fn unpose(&mut self, physbone: usize) {
        let chain = &mut self.chains[physbone];
        chain.posed = None;
        chain.parameters.is_posed = false;
    }
}

fn allowed(p: &model::Permission, who: Toucher) -> bool {
    match who {
        Toucher::Wearer => p.allow_self,
        Toucher::Other => p.allow_others,
    }
}

fn blend(a: &Frame, b: &Frame, s: f32) -> Frame {
    let mix = |x: &[Transform], y: &[Transform]| x.iter().zip(y).map(|(x, y)| x.lerp(y, s)).collect();
    Frame {
        root: a.root.lerp(&b.root, s),
        nodes: mix(&a.nodes, &b.nodes),
        locals: mix(&a.locals, &b.locals),
        active: b.active.clone(),
        enabled: b.enabled.clone(),
    }
}

impl Chain {
    fn on(&self, frame: &Frame, index: usize) -> bool {
        frame.enabled.get(index).copied().unwrap_or(true) && self.lineage.iter().all(|&n| frame.active.get(n).copied().unwrap_or(true))
    }

    fn parent_world(&self, frame: &Frame) -> Transform {
        let parent = self.root_parent.and_then(|p| frame.nodes.get(p)).copied().unwrap_or(Transform::IDENTITY);
        frame.root.rigid().mul(&parent)
    }

    fn reference_frame(&self, frame: &Frame) -> Transform {
        match self.immobile_type {
            ImmobileType::AllMotion => self.parent_world(frame).rigid(),
            ImmobileType::World => Transform { translation: frame.root.translation, ..Transform::IDENTITY },
        }
    }

    fn local(&self, frame: &Frame, bone: usize) -> Transform {
        let bone = &self.bones[bone];
        match bone.node {
            Some(n) if self.animated => frame.locals.get(n).copied().unwrap_or(bone.authored),
            _ => bone.authored,
        }
    }

    fn animate(&mut self, frame: &Frame) {
        let parent = self.parent_world(frame);
        for i in 0..self.bones.len() {
            let base = self.bones[i].parent.map_or(parent, |p| self.rest[p]);
            self.rest[i] = base.mul(&self.local(frame, i));
        }
    }

    /// Each bone's rotation relative to its unaimed orientation: what a pose keeps.
    fn pose(&self) -> Vec<Quat> {
        (0..self.bones.len())
            .map(|i| {
                let base = match self.bones[i].parent {
                    None => self.rest[0].rotation,
                    Some(p) => self.world[p].rotation * (self.rest[p].rotation.inverse() * self.rest[i].rotation),
                };
                (base.inverse() * self.world[i].rotation).normalize()
            })
            .collect()
    }

    fn reset(&mut self, frame: &Frame) {
        self.animate(frame);
        for (b, r) in self.bones.iter_mut().zip(&self.rest) {
            b.position = r.translation;
            b.previous = r.translation;
        }
        self.world = self.rest.clone();
        self.settled = self.rest.clone();
        self.reference = Some(self.reference_frame(frame));
    }

    /// Bone `i`'s orientation before it is aimed: its parent's simulated orientation and its own animated
    /// local rotation, or its held pose.
    fn base(&self, frame: &Frame, i: usize) -> Transform {
        let local = self.local(frame, i);
        let mut t = match self.bones[i].parent {
            None => self.rest[0],
            Some(p) => Transform { translation: self.bones[i].position, ..self.world[p].mul(&local) },
        };
        if let Some(pose) = &self.posed {
            t.rotation = (t.rotation * pose[i]).normalize();
        }
        t
    }

    fn step(&mut self, frame: &Frame, colliders: &[ColliderAt]) {
        self.animate(frame);
        let reference = self.reference_frame(frame);
        let moved = reference.mul(&self.reference.unwrap_or(reference).inverse_rigid());
        self.reference = Some(reference);
        let root = frame.root.rigid();
        let places: Vec<(ColliderAt, Transform)> = self
            .colliders
            .iter()
            .filter_map(|&c| colliders.get(c))
            .map(|c| (c.clone(), root.mul(&frame.nodes.get(c.node).copied().unwrap_or(Transform::IDENTITY))))
            .collect();
        self.bones[0].position = self.rest[0].translation;
        self.bones[0].previous = self.rest[0].translation;
        self.world[0] = self.base(frame, 0);
        self.held = vec![false; self.bones.len()];
        if let Some(g) = self.grab {
            self.reach(g);
        }
        for p in 0..self.bones.len() {
            let children = self.bones[p].children.clone();
            for &i in &children {
                self.solve(p, i, frame, moved, &places);
            }
            let driving = self.aim(p, frame);
            for &i in &children {
                if driving.len() != 1 || driving[0] != i {
                    let local = self.local(frame, i);
                    self.bones[i].position = self.world[p].point(local.translation);
                }
                self.world[i] = self.base(frame, i);
            }
        }
        self.settled = self.world.clone();
        self.measure();
    }

    /// Moves the held bone toward the hand and its ancestors after it, keeping their lengths (FABRIK).
    fn reach(&mut self, g: Grab) {
        let mut path = vec![g.bone];
        while let Some(p) = self.bones[*path.last().unwrap()].parent {
            path.push(p);
        }
        path.reverse();
        let lengths: Vec<f32> = path.windows(2).map(|w| (self.rest[w[1]].translation - self.rest[w[0]].translation).length()).collect();
        let held = self.bones[g.bone].position;
        let goal = held + (g.target + g.offset - held) * g.movement.clamp(0.0, 1.0);
        let mut at: Vec<Vec3> = path.iter().map(|&b| self.bones[b].position).collect();
        let root = at[0];
        for _ in 0..4 {
            *at.last_mut().unwrap() = goal;
            for k in (0..at.len() - 1).rev() {
                at[k] = at[k + 1] + (at[k] - at[k + 1]).normalize_or(Vec3::Y) * lengths[k];
            }
            at[0] = root;
            for k in 1..at.len() {
                at[k] = at[k - 1] + (at[k] - at[k - 1]).normalize_or(Vec3::Y) * lengths[k - 1];
            }
        }
        for (&b, x) in path.iter().zip(at).skip(1) {
            self.bones[b].position = x;
            self.bones[b].previous = x;
            self.held[b] = true;
        }
    }

    fn solve(&mut self, p: usize, i: usize, frame: &Frame, moved: Transform, places: &[(ColliderAt, Transform)]) {
        if self.held[i] {
            return;
        }
        let parent = self.world[p];
        let local = self.local(frame, i);
        let origin = self.bones[p].position;
        let rest = parent.point(local.translation) - origin;
        let rest_length = rest.length();
        if rest_length <= f32::EPSILON {
            self.bones[i].position = origin;
            self.bones[i].previous = origin;
            return;
        }
        let rest_direction = rest / rest_length;
        let q = self.bones[i].params;
        let gravity = q.gravity * (1.0 - q.gravity_falloff);
        let target = match self.version {
            PhysBoneVersion::V1_0 => origin + rest,
            PhysBoneVersion::V1_1 => {
                let down = if gravity >= 0.0 { Vec3::NEG_Y } else { Vec3::Y };
                origin + rest_direction.lerp(down, gravity.abs().min(1.0)).normalize_or(rest_direction) * rest_length
            }
        };
        let carried = match self.integration {
            IntegrationType::Simplified => 0.0,
            IntegrationType::Advanced => (q.stiffness * STIFFNESS_CARRY).min(1.0),
        };
        let carry = parent.translation - self.settled[p].translation;
        let bone = &mut self.bones[i];
        bone.position += carry * carried;
        bone.previous += carry * carried;
        bone.position += (moved.point(bone.position) - bone.position) * q.immobile;
        bone.previous += (moved.point(bone.previous) - bone.previous) * q.immobile;
        let velocity = bone.position - bone.previous;
        bone.previous = bone.position;
        let pull = (target - bone.position) * q.pull;
        bone.position += match self.integration {
            IntegrationType::Simplified => velocity * q.spring + pull * (1.0 - q.spring),
            IntegrationType::Advanced => (velocity * q.spring + pull) * (1.0 - q.stiffness),
        };
        if self.version == PhysBoneVersion::V1_0 {
            bone.position += Vec3::NEG_Y * (gravity * rest_length * GRAVITY_1_0);
        }
        let (low, high) = match self.version {
            PhysBoneVersion::V1_0 => (1.0, 1.0 + q.max_stretch),
            PhysBoneVersion::V1_1 => (1.0 - q.max_squish, 1.0 + q.max_stretch),
        };
        let offset = bone.position - origin;
        let free = self.grab.is_some() || (self.version == PhysBoneVersion::V1_1 && q.stretch_motion > 0.0);
        let length = if free { offset.length().clamp(rest_length * low, rest_length * high) } else { rest_length };
        let direction = limit(offset.normalize_or(rest_direction), local.translation, parent.rotation, q.limit);
        bone.position = origin + direction * length;
        for (collider, place) in places {
            bone.position = collide(bone.position, q.radius * place.scale.x.abs(), collider, place);
        }
    }

    /// Turns `bone` toward the children that drive it, which it returns.
    fn aim(&mut self, bone: usize, frame: &Frame) -> Vec<usize> {
        let children = &self.bones[bone].children;
        let driving: Vec<usize> = match (self.multi_child, children.len()) {
            (_, 0) => return vec![],
            (_, 1) => children.clone(),
            (MultiChildType::Ignore, _) => return vec![],
            (MultiChildType::First, _) => children[..1].to_vec(),
            (MultiChildType::Average, _) => children.clone(),
        };
        let w = self.world[bone];
        let mut sum = Vec4::ZERO;
        for &c in &driving {
            let rest = w.rotation * (w.scale * self.local(frame, c).translation);
            let turn = Vec4::from(Quat::from_rotation_arc(rest.normalize_or_zero(), (self.bones[c].position - w.translation).normalize_or_zero()));
            sum += if sum.dot(turn) < 0.0 { -turn } else { turn };
        }
        let turn = Quat::from_vec4(sum).normalize();
        self.world[bone].rotation = (turn * w.rotation).normalize();
        driving
    }

    /// `_Angle`: the turn of the chain from its root to its first end, against its rest, over 180°.
    /// `_Stretch` and `_Squish`: that path's length against its rest length, over the largest allowed.
    fn measure(&mut self) {
        let mut end = 0;
        while let Some(&c) = self.bones[end].children.first() {
            end = c;
        }
        if end == 0 {
            return;
        }
        let rest = self.rest[end].translation - self.rest[0].translation;
        let now = self.bones[end].position - self.bones[0].position;
        self.parameters.angle = if rest.length_squared() > 0.0 && now.length_squared() > 0.0 { rest.angle_between(now) / std::f32::consts::PI } else { 0.0 };
        let (mut length, mut rest_length, mut max_stretch, mut max_squish) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
        let mut at = end;
        while let Some(p) = self.bones[at].parent {
            length += (self.bones[at].position - self.bones[p].position).length();
            rest_length += (self.rest[at].translation - self.rest[p].translation).length();
            max_stretch = max_stretch.max(self.bones[at].params.max_stretch);
            max_squish = max_squish.max(self.bones[at].params.max_squish);
            at = p;
        }
        let ratio = if rest_length > 0.0 { length / rest_length - 1.0 } else { 0.0 };
        self.parameters.stretch = if max_stretch > 0.0 { (ratio / max_stretch).clamp(0.0, 1.0) } else { 0.0 };
        self.parameters.squish = if max_squish > 0.0 { (-ratio / max_squish).clamp(0.0, 1.0) } else { 0.0 };
    }

    fn locals(&self, frame: &Frame) -> Vec<Transform> {
        let root_parent = self.parent_world(frame);
        self.bones
            .iter()
            .enumerate()
            .filter(|(_, b)| b.node.is_some())
            .map(|(i, b)| {
                let parent = b.parent.map_or(root_parent, |p| self.world[p]);
                let inverse = parent.rotation.inverse();
                let unscale = Vec3::select(parent.scale.cmpeq(Vec3::ZERO), Vec3::ZERO, parent.scale.recip());
                Transform {
                    translation: inverse * (self.world[i].translation - parent.translation) * unscale,
                    rotation: (inverse * self.world[i].rotation).normalize(),
                    scale: self.world[i].scale * unscale,
                }
            })
            .collect()
    }
}

/// Advanced integration carries a bone along with its parent's movement by this much per unit of stiffness,
/// measured against the reference traces.
const STIFFNESS_CARRY: f32 = 1.45;
/// Version 1.0 gravity moves a bone this many of its own lengths each step at gravity 1, measured likewise.
const GRAVITY_1_0: f32 = 0.5;

/// Keeps `direction`, the bone toward its child in the parent's frame `frame`, within the limit. The limit's
/// axis is the child's rest direction, turned by the limit rotation (Unity's X, then Y, then Z); a hinge swings
/// about the limit's X axis and polar limits bound the swing about X and about Z separately.
fn limit(direction: Vec3, local_rest: Vec3, frame: Quat, limit: LimitAt) -> Vec3 {
    let cone = |d: Vec3, max: f32| {
        let angle = d.angle_between(Vec3::Y);
        if angle <= max || angle.is_nan() { d } else { Quat::from_rotation_arc(Vec3::Y, d).slerp(Quat::IDENTITY, 1.0 - max / angle) * Vec3::Y }
    };
    let (rotation, d) = match limit {
        LimitAt::None => return direction,
        LimitAt::Angle { rotation, .. } | LimitAt::Hinge { rotation, .. } | LimitAt::Polar { rotation, .. } => {
            let frame = frame * Quat::from_rotation_arc(Vec3::Y, local_rest.normalize_or(Vec3::Y)) * rotation;
            (frame, frame.inverse() * direction)
        }
    };
    let limited = match limit {
        LimitAt::None => unreachable!(),
        LimitAt::Angle { max, .. } => cone(d, max),
        LimitAt::Hinge { max, .. } => {
            let angle = d.z.atan2(d.y).clamp(-max, max);
            Vec3::new(0.0, angle.cos(), angle.sin())
        }
        LimitAt::Polar { max_x, max_z, .. } => {
            let pitch = d.z.atan2(d.y).clamp(-max_x, max_x);
            let yaw = d.x.atan2(d.y.hypot(d.z)).clamp(-max_z, max_z);
            Vec3::new(yaw.sin(), yaw.cos() * pitch.cos(), yaw.cos() * pitch.sin())
        }
    };
    rotation * limited
}

/// Pushes a bone's end at `position` out of the collider, or keeps it inside an inside-bounds one. Bones collide
/// as spheres of their radius at their ends, and one of radius 0 passes through spheres and capsules: both as
/// the reference traces show, whatever the collider's bones-as-spheres setting.
fn collide(position: Vec3, radius: f32, collider: &ColliderAt, place: &Transform) -> Vec3 {
    let center = place.point(collider.position);
    let axis = (place.rotation * collider.rotation) * Vec3::Y;
    let scale = place.scale.x.abs();
    let (r, half, inside) = match collider.shape {
        ColliderShape::Sphere { radius: r, inside_bounds } => (r * scale, 0.0, inside_bounds),
        ColliderShape::Capsule { radius: r, height, inside_bounds } => (r * scale, ((height * 0.5 - r) * scale).max(0.0), inside_bounds),
        ColliderShape::Plane {} => {
            let d = (position - center).dot(axis) - radius;
            return if d < 0.0 { position - axis * d } else { position };
        }
    };
    let ab = axis * half;
    let core = center + ab * (if half > 0.0 { ((position - center).dot(axis) / half).clamp(-1.0, 1.0) } else { 0.0 });
    let d = position - core;
    if inside {
        let limit = (r - radius).max(0.0);
        return if d.length() > limit { core + d.normalize_or_zero() * limit } else { position };
    }
    let reach = r + radius;
    if radius <= 0.0 || d.length() >= reach {
        return position;
    }
    core + d.normalize_or(axis.any_orthonormal_vector()) * reach
}
