use glam::{Quat, Vec3, Vec4};
use serde_json::Value;

use crate::contacts::{closest_on_segments, standard_hands};
use crate::model::{self, ColliderShape, ContactShape, Curved, ImmobileType, IntegrationType, Limit, MultiChildType, PhysBoneVersion};
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

    /// `child`, a pose in the same space as `self`, relative to `self`.
    pub fn relative(&self, child: &Transform) -> Transform {
        let r = self.rotation.inverse();
        Transform {
            translation: (r * (child.translation - self.translation)) / self.scale,
            rotation: (r * child.rotation).normalize(),
            scale: child.scale / self.scale,
        }
    }

    fn rigid(&self) -> Transform {
        Transform { scale: Vec3::ONE, ..*self }
    }

    fn inverse_rigid(&self) -> Transform {
        let rotation = self.rotation.inverse();
        Transform { translation: rotation * -self.translation, rotation, scale: Vec3::ONE }
    }

    fn lerp(&self, to: &Transform, s: f32) -> Transform {
        if self == to {
            return *self;
        }
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
    /// Other avatars' hand and finger colliders in the world.
    pub hands: Vec<HandCollider>,
}

/// Another avatar's hand or finger collider: a capsule on a transform, pushing each PhysBone whose collision filter
/// admits others.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HandCollider {
    /// The host's name for the avatar it belongs to; with `part`, names the collider across frames.
    pub player: u64,
    /// Its place among that avatar's hand and finger colliders.
    pub part: usize,
    /// The pose of the transform the capsule sits on, in the world.
    pub at: Transform,
    pub radius: f32,
    pub height: f32,
    pub position: Vec3,
    pub rotation: Quat,
}

impl HandCollider {
    fn touch(&self, owner: Toucher) -> Touch {
        let scale = self.at.scale.x.abs();
        let center = self.at.point(self.position);
        let half = (self.at.rotation * self.rotation) * Vec3::Y * ((self.height * 0.5 - self.radius) * scale).max(0.0);
        Touch { owner, a: center - half, b: center + half, radius: self.radius * scale }
    }
}

/// A hand or finger capsule in the world, by its core segment, and whose hand it is.
#[derive(Clone, Copy, Debug)]
struct Touch {
    owner: Toucher,
    a: Vec3,
    b: Vec3,
    radius: f32,
}

/// One of the avatar's own hand or finger colliders: the descriptor's, placed where its standard sender is.
#[derive(Clone, Copy, Debug)]
struct OwnHand {
    node: usize,
    radius: f32,
    height: f32,
    position: Vec3,
    rotation: Quat,
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
    /// Where the bone's transform is drawn. While its root lags a change of pose (see [`Chain::follow`]) the simulation
    /// runs from where the root has got to, and the drawn bone points from its parent's drawn place.
    shown: Vec3,
    /// The bone's length over its rest length, less one, after the last step.
    strain: f32,
    /// The bone has not moved by its own step since the chain was reset (see [`STEP_FROM_REST`]).
    resting: bool,
}

impl Bone {
    /// The bone `share` of the way from `self` to `to`, a later step of it; what a step does not move is `to`'s.
    fn lerp(&self, to: &Bone, share: f32) -> Bone {
        let Bone { node, authored, parent, children, params, position, previous, shown, strain, resting } = to.clone();
        Bone {
            node,
            authored,
            parent,
            children,
            params,
            position: self.position.lerp(position, share),
            previous: self.previous.lerp(previous, share),
            shown: self.shown.lerp(shown, share),
            strain: self.strain + (strain - self.strain) * share,
            resting,
        }
    }
}

#[derive(Clone, Debug)]
struct Chain {
    /// The component's node.
    node: usize,
    /// Another component earlier in the model has the same root: VRChat poses those bones by the first alone, so this
    /// one has none.
    shadowed: bool,
    motion: Motion,
    reset_when_disabled: bool,
    version: PhysBoneVersion,
    integration: IntegrationType,
    immobile_type: ImmobileType,
    multi_child: MultiChildType,
    animated: bool,
    root_parent: Option<usize>,
    colliders: Vec<usize>,
    collision: model::Permission,
    bones: Vec<Bone>,
    /// The frame immobile holds bones to, relative to the avatar root. Under all-motion immobile its place lags a change
    /// of pose as a pinned root does (see [`Chain::follow`]) while it turns at once.
    reference: Option<Transform>,
    /// The root's orientation relative to the avatar root, lagging a change of pose as a pinned root's place does.
    turned: Option<Quat>,
    /// The avatar root's place last step, which parts its own motion from a change of pose.
    last_root: Option<Transform>,
    rest: Vec<Transform>,
    world: Vec<Transform>,
    /// `world` before the last step, which a chain drawn behind its step (see [`DRAWN_SHARE`]) is drawn from.
    stepped_from: Vec<Transform>,
    /// Drawn [`DRAWN_SHARE`] of the way into each step since a collider met it at rest with a bone's end inside.
    drawn_behind: bool,
    /// Where the simulation has put each bone the chain pins to its animated place (its root, and a child that does not
    /// alone drive its parent's aim), relative to the avatar root. `solve` moves a pinned child before `step` puts it
    /// back, so its simulated position does not keep this.
    pinned: Vec<Vec3>,
    /// Each bone's direction from its parent at the chain's reset, relative to the avatar root: what gravity falloff
    /// measures a bone's tilt against. Against its animated direction, a chain whose parent turns after the reset
    /// settles where the SDK's does not; no trace turns the avatar root, so which of its frame or the world's holds the
    /// direction is not measured.
    reset_directions: Vec<Vec3>,
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
    /// A sphere or capsule outside which bones stay and that does not treat them as spheres: it meets each bone's segment.
    meets_segment: bool,
}

impl ColliderAt {
    /// A sphere's or capsule's core segment and radius in the world, placed at `place`; none for a plane.
    fn core(&self, place: &Transform) -> Option<(Vec3, Vec3, f32)> {
        let center = place.point(self.position);
        let axis = (place.rotation * self.rotation) * Vec3::Y;
        let scale = place.scale.x.abs();
        let (r, half) = match self.shape {
            ColliderShape::Sphere { radius, .. } => (radius * scale, 0.0),
            ColliderShape::Capsule { radius, height, .. } => (radius * scale, ((height * 0.5 - radius) * scale).max(0.0)),
            ColliderShape::Plane {} => return None,
        };
        Some((center - axis * half, center + axis * half, r))
    }
}

/// What a chain did in the last [`PhysBones::advance`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    /// Off since it last reset: animation poses its bones.
    Off,
    Running,
    /// Stopped after it last stepped, without resetting: [`PhysBones::local`] answers that step's pose, which the
    /// SDK's bones keep where animation does not write them, as Unity leaves a transform nothing writes.
    Frozen,
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
    hands: Vec<OwnHand>,
    model: Vec<model::PhysBone>,
    /// The chains in the order they step: one reading a node below another's bones steps after it.
    order: Vec<usize>,
    /// For each chain another reads, every node below its root, parents first.
    below: Vec<Option<Below>>,
    parent: Vec<Option<usize>>,
    /// The host's latest frame and its time.
    last: Option<(f64, Frame)>,
    now: f64,
    /// The time of the last simulated step, at most one step past `now`.
    stepped: f64,
    steps: u32,
    before: Vec<Vec<Transform>>,
    after: Vec<Vec<Transform>>,
    /// Each node animation poses, which a frozen chain leaves to it.
    animated: Vec<bool>,
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
    pub names: Vec<String>,
}

impl Nodes {
    pub fn read(avatar: &Avatar) -> Result<Nodes, Error> {
        Nodes::of(&avatar.gltf)
    }

    /// Every node's pose relative to the avatar root from each node's local pose.
    pub fn world(&self, locals: &[Transform]) -> Vec<Transform> {
        assert_eq!(locals.len(), self.parent.len(), "one local pose per node");
        let mut world: Vec<Option<Transform>> = vec![None; locals.len()];
        for start in 0..locals.len() {
            let mut path = vec![start];
            while let Some(p) = self.parent[*path.last().unwrap()].filter(|&p| world[p].is_none()) {
                path.push(p);
            }
            while let Some(i) = path.pop() {
                world[i] = Some(self.parent[i].and_then(|p| world[p]).map_or(locals[i], |p| p.mul(&locals[i])));
            }
        }
        world.into_iter().map(|t| t.unwrap_or(Transform::IDENTITY)).collect()
    }

    pub fn of(gltf: &serde_json::Map<String, Value>) -> Result<Nodes, Error> {
        let nodes = gltf.get("nodes").and_then(Value::as_array).map(Vec::as_slice).unwrap_or(&[]);
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
        let names = nodes.iter().map(|n| n.get("name").and_then(Value::as_str).unwrap_or_default().to_string()).collect();
        Ok(Nodes { children, parent, rest, names })
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
        gravity: at(&pb.gravity, t),
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
/// the root's children to 1 at the deepest bones, but the radius belongs to a bone's end and is read from 0 at the root
/// to 1 at the deepest ends: the radius curve rows of both versions fit that reading alone, while the pull curve probe
/// misses by 14% of chain length if pull is read that way too.
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
        shown: Vec3::ZERO,
        strain: 0.0,
        resting: true,
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
        bone.params.radius = at(&pb.radius, if deepest > 0.0 { d / deepest } else { 0.0 }).max(0.0);
    }
    let count = bones.len();
    Chain {
        node: pb.node.0 as usize,
        shadowed: false,
        motion: Motion::Off,
        reset_when_disabled: pb.reset_when_disabled,
        version: pb.version,
        integration: pb.integration_type,
        immobile_type: pb.immobile_type,
        multi_child: pb.multi_child_type,
        animated: pb.is_animated,
        root_parent: nodes.parent[root],
        colliders: pb.colliders.iter().map(|c| c.0 as usize).collect(),
        collision: pb.collision,
        bones,
        reference: None,
        turned: None,
        last_root: None,
        rest: vec![Transform::IDENTITY; count],
        world: vec![Transform::IDENTITY; count],
        stepped_from: vec![Transform::IDENTITY; count],
        drawn_behind: false,
        pinned: vec![Vec3::ZERO; count],
        reset_directions: vec![Vec3::ZERO; count],
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
        let mut chains: Vec<Chain> = model.physbones.iter().map(|pb| chain(pb, &roots, &nodes)).collect();
        for (i, c) in chains.iter_mut().enumerate() {
            if roots[..i].contains(&roots[i]) {
                *c = Chain { shadowed: true, bones: vec![], rest: vec![], world: vec![], stepped_from: vec![], pinned: vec![], reset_directions: vec![], held: vec![], ..c.clone() };
            }
        }
        let colliders: Vec<ColliderAt> = model
            .colliders
            .iter()
            .map(|c| ColliderAt {
                node: c.node.0 as usize,
                shape: c.shape.clone(),
                position: Vec3::from(c.position),
                rotation: unit(Quat::from_array(c.rotation)),
                meets_segment: matches!(c.shape, ColliderShape::Sphere { inside_bounds: false, .. } | ColliderShape::Capsule { inside_bounds: false, .. })
                    && !c.bones_as_spheres,
            })
            .collect();
        let hands = standard_hands(&model.descriptor, &model.humanoid, &nodes)
            .into_iter()
            .filter_map(|s| match s.shape {
                ContactShape::Capsule { radius, height } => Some(OwnHand {
                    node: s.node.0 as usize,
                    radius,
                    height,
                    position: Vec3::from(s.position),
                    rotation: unit(Quat::from_array(s.rotation)),
                }),
                ContactShape::Sphere { .. } => None,
            })
            .collect();
        let authored: Vec<Vec<Transform>> = chains.iter().map(|c| c.bones.iter().filter(|b| b.node.is_some()).map(|b| b.authored).collect()).collect();
        let (order, below) = schedule(&chains, &colliders, &nodes);
        Ok(PhysBones {
            chains,
            colliders,
            hands,
            model: model.physbones.clone(),
            order,
            below,
            parent: nodes.parent,
            last: None,
            now: 0.0,
            stepped: 0.0,
            steps: 0,
            before: authored.clone(),
            after: authored,
            animated: vec![],
        })
    }

    /// The avatar's hand and finger colliders in the world, from the avatar root in the world and every node's pose in
    /// its space, as `player`'s.
    pub fn hands(&self, player: u64, root: &Transform, space: &[Transform]) -> Vec<HandCollider> {
        let root = root.rigid();
        self.hands
            .iter()
            .enumerate()
            .map(|(part, h)| HandCollider {
                player,
                part,
                at: root.mul(&space.get(h.node).copied().unwrap_or(Transform::IDENTITY)),
                radius: h.radius,
                height: h.height,
                position: h.position,
                rotation: h.rotation,
            })
            .collect()
    }

    /// The nodes each chain poses, in the order [`PhysBones::local`] answers for them.
    pub fn bones(&self, physbone: usize) -> impl Iterator<Item = usize> + '_ {
        self.chains[physbone].bones.iter().filter_map(|b| b.node)
    }

    /// Whether a [`PhysBones::advance`] has started the simulation, its chains at that frame's pose.
    pub(crate) fn started(&self) -> bool {
        self.last.is_some()
    }

    /// Whether `physbone` simulated in the last [`PhysBones::advance`], stopped with its last step's pose, or neither.
    pub fn motion(&self, physbone: usize) -> Motion {
        self.chains[physbone].motion
    }

    /// Marks the nodes animation poses: a frozen chain leaves them to it and keeps its pose on the rest.
    pub fn set_animated(&mut self, animated: Vec<bool>) {
        self.animated = animated;
    }

    /// Whether a frozen chain leaves node `n` to animation.
    pub fn animates(&self, n: usize) -> bool {
        self.animated.get(n).copied().unwrap_or(false)
    }

    /// Whether an earlier component has `physbone`'s root, so `physbone` poses nothing and reports nothing.
    pub fn shadowed(&self, physbone: usize) -> bool {
        self.chains[physbone].shadowed
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
                c.motion = if c.on(frame, i, &self.parent) { Motion::Running } else { Motion::Off };
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
            let mut input = blend(&last, frame, s);
            self.before = self.after.clone();
            let touched = self.steps >= UNTOUCHED_STEPS;
            self.steps = (self.steps + 1).min(UNTOUCHED_STEPS);
            let own = if touched { self.hands(0, &input.root, &input.nodes) } else { vec![] };
            let others = input.hands.iter().filter(|_| touched);
            let hands: Vec<Touch> = own.iter().map(|h| h.touch(Toucher::Wearer)).chain(others.map(|h| h.touch(Toucher::Other))).collect();
            self.step_chains(&mut input, &hands, None);
            self.stepped = time;
        }
        self.last = Some((t1, frame.clone()));
    }

    /// Steps every chain once at `input`, or `carry` of a step, posing the nodes below each running chain for those after it.
    fn step_chains(&mut self, input: &mut Frame, hands: &[Touch], carry: Option<f32>) {
        for &i in &self.order {
            let c = &mut self.chains[i];
            let on = c.on(input, i, &self.parent);
            c.motion = match (on, c.motion) {
                (true, was) => {
                    if was != Motion::Running {
                        c.reset(input);
                    }
                    let was = carry.map(|share| (share, c.clone()));
                    c.step(input, &self.colliders, &self.parent, hands);
                    // A chain drawn behind its step takes the load step whole: drawn into it, that is where the SDK draws it.
                    if let Some((share, was)) = was.filter(|_| !c.drawn_behind) {
                        for (b, from) in c.bones.iter_mut().zip(&was.bones) {
                            *b = from.lerp(b, share);
                        }
                        for (w, from) in c.world.iter_mut().zip(&was.world) {
                            *w = from.lerp(w, share);
                        }
                    }
                    Motion::Running
                }
                (false, Motion::Running) if c.reset_when_disabled => {
                    c.reset(input);
                    Motion::Off
                }
                (false, Motion::Running | Motion::Frozen) => Motion::Frozen,
                (false, Motion::Off) => continue,
            };
            let animated = &self.animated;
            let frozen: Vec<(usize, Transform)> = match c.motion {
                Motion::Frozen => c
                    .bones
                    .iter()
                    .filter_map(|b| b.node)
                    .zip(self.after[i].iter().copied())
                    .filter(|(n, _)| !animated.get(*n).copied().unwrap_or(false))
                    .collect(),
                _ => {
                    self.after[i] = c.locals(input);
                    vec![]
                }
            };
            if let Some(below) = self.below[i].as_ref().filter(|_| c.motion != Motion::Off) {
                let unroot = input.root.rigid().inverse_rigid();
                for &(n, bone) in below {
                    let local = frozen.iter().find(|(f, _)| *f == n).map(|(_, l)| l).or(input.locals.get(n));
                    let posed = match (bone, self.parent[n]) {
                        (Some(b), _) if c.motion == Motion::Running => Some(unroot.mul(&c.world[b])),
                        (_, Some(p)) => input.nodes.get(p).zip(local).map(|(p, l)| p.mul(l)),
                        (_, None) => None,
                    };
                    if let (Some(posed), Some(at)) = (posed, input.nodes.get_mut(n)) {
                        *at = posed;
                    }
                }
            }
        }
    }

    /// Carries every running chain [`LOAD_CARRY`] of a step further at `frame`, the file's pose, and a whole step more for
    /// each of the `standing` frames the avatar stood loaded before its first evaluated one, and keeps the clock where it
    /// is: the step a fifth longer than the rest that Unity's first frame after load takes, and the SDK's step on each
    /// frame after it.
    pub(crate) fn carry_load(&mut self, frame: &Frame, standing: u32) {
        self.step_chains(&mut frame.clone(), &[], Some(LOAD_CARRY));
        for _ in 0..standing {
            self.step_chains(&mut frame.clone(), &[], None);
        }
        self.before = self.after.clone();
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
        if self.chains[physbone].shadowed || !allowed(&pb.grabbing, who) {
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

/// Nodes below a chain's root, each with the chain's bone that poses it, if one does.
type Below = Vec<(usize, Option<usize>)>;

/// The order chains step in, and what each chain another reads must refresh below it. A chain reads its root's
/// parent, its immobile node and its colliders' nodes; where one lies below another chain's bones, as when a
/// chain's root hangs from another chain, it steps after that chain and sees its simulated pose, as three-vrm
/// orders its joints after their parents' and colliders'. Chains that read each other step in model order.
fn schedule(chains: &[Chain], colliders: &[ColliderAt], nodes: &Nodes) -> (Vec<usize>, Vec<Option<Below>>) {
    let mut posed_by: Vec<Vec<usize>> = vec![vec![]; nodes.parent.len()];
    for (i, c) in chains.iter().enumerate() {
        for n in c.bones.iter().filter_map(|b| b.node) {
            posed_by[n].push(i);
        }
    }
    let reads: Vec<Vec<usize>> = chains
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let reference = match c.immobile_type {
                ImmobileType::Node(n) => Some(n.0 as usize),
                ImmobileType::AllMotion | ImmobileType::World => None,
            };
            let mut read: Vec<usize> = c.root_parent.into_iter().chain(reference).chain(c.colliders.iter().filter_map(|&k| colliders.get(k)).map(|k| k.node)).collect();
            let mut on = vec![];
            while let Some(n) = read.pop() {
                for &j in posed_by.get(n).into_iter().flatten() {
                    if j != i && !on.contains(&j) {
                        on.push(j);
                    }
                }
                read.extend(nodes.parent.get(n).copied().flatten());
            }
            on
        })
        .collect();
    fn visit(i: usize, reads: &[Vec<usize>], state: &mut [u8], order: &mut Vec<usize>) {
        if state[i] != 0 {
            return;
        }
        state[i] = 1;
        for &j in &reads[i] {
            visit(j, reads, state, order);
        }
        state[i] = 2;
        order.push(i);
    }
    let mut state = vec![0u8; chains.len()];
    let mut order = vec![];
    for i in 0..chains.len() {
        visit(i, &reads, &mut state, &mut order);
    }
    let below = (0..chains.len())
        .map(|i| {
            reads.iter().any(|r| r.contains(&i)).then(|| {
                let c = &chains[i];
                let bone = |n: usize| c.bones.iter().position(|b| b.node == Some(n));
                let mut out = vec![];
                let mut next = c.bones[0].node.into_iter().collect::<Vec<_>>();
                while let Some(n) = next.pop() {
                    out.push((n, bone(n)));
                    next.extend(nodes.children[n].iter().rev());
                }
                out
            })
        })
        .collect();
    (order, below)
}

/// A node's pose in the world, the avatar root's for none.
fn node_world(frame: &Frame, node: Option<usize>) -> Transform {
    frame.root.rigid().mul(&node.and_then(|n| frame.nodes.get(n)).copied().unwrap_or(Transform::IDENTITY))
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
        hands: b
            .hands
            .iter()
            .map(|y| match a.hands.iter().find(|x| (x.player, x.part) == (y.player, y.part)) {
                Some(x) => HandCollider { at: x.at.lerp(&y.at, s), ..*y },
                None => *y,
            })
            .collect(),
    }
}

impl Chain {
    fn on(&self, frame: &Frame, index: usize, parent: &[Option<usize>]) -> bool {
        !self.shadowed && frame.enabled.get(index).copied().unwrap_or(true) && crate::active_in_hierarchy(&frame.active, parent, self.node)
    }

    fn parent_world(&self, frame: &Frame) -> Transform {
        node_world(frame, self.root_parent)
    }

    fn reference_frame(&self, frame: &Frame) -> Transform {
        match self.immobile_type {
            ImmobileType::AllMotion => self.parent_world(frame).rigid(),
            ImmobileType::World => Transform { translation: frame.root.translation, ..Transform::IDENTITY },
            ImmobileType::Node(n) => node_world(frame, Some(n.0 as usize)).rigid(),
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
            b.shown = r.translation;
            b.strain = 0.0;
            b.resting = true;
        }
        self.world = self.rest.clone();
        self.stepped_from = self.rest.clone();
        self.drawn_behind = false;
        let unroot = frame.root.rigid().inverse_rigid();
        self.pinned = self.rest.iter().map(|r| unroot.point(r.translation)).collect();
        self.reset_directions = self
            .bones
            .iter()
            .zip(&self.rest)
            .map(|(b, r)| b.parent.map_or(Vec3::ZERO, |p| unroot.rotation * (r.translation - self.rest[p].translation).normalize_or_zero()))
            .collect();
        self.reference = Some(unroot.mul(&self.reference_frame(frame)));
        self.turned = None;
        self.last_root = Some(frame.root.rigid());
    }

    /// Bone `i`'s orientation before it is aimed: its parent's drawn orientation and its own animated
    /// local rotation, or its held pose.
    fn base(&self, frame: &Frame, i: usize) -> Transform {
        let local = self.local(frame, i);
        let mut t = match self.bones[i].parent {
            None => self.rest[0],
            Some(p) => Transform { translation: self.bones[i].shown, ..self.world[p].mul(&local) },
        };
        if let Some(pose) = &self.posed {
            t.rotation = (t.rotation * pose[i]).normalize();
        }
        t
    }

    fn step(&mut self, frame: &Frame, colliders: &[ColliderAt], parent: &[Option<usize>], hands: &[Touch]) {
        self.stepped_from = self.world.clone();
        let at_rest = self.bones.iter().all(|b| b.parent.is_none() || b.resting);
        let mut met_inside = false;
        self.animate(frame);
        let avatar = frame.root.rigid();
        let before = self.last_root.unwrap_or(avatar);
        let root_motion = avatar.mul(&before.inverse_rigid());
        self.last_root = Some(avatar);
        let reference = self.reference_frame(frame);
        let moved = match self.immobile_type {
            ImmobileType::World | ImmobileType::Node(_) => {
                let old = self.reference.unwrap_or(avatar.inverse_rigid().mul(&reference));
                reference.mul(&before.mul(&old).inverse_rigid())
            }
            ImmobileType::AllMotion => {
                let goal = avatar.inverse_rigid().mul(&reference);
                let old = self.reference.unwrap_or(goal);
                let share = self.follow_share();
                let new = Transform { translation: old.translation.lerp(goal.translation, share), rotation: goal.rotation, scale: old.scale.lerp(goal.scale, share) };
                self.reference = Some(new);
                avatar.mul(&new).mul(&before.mul(&old).inverse_rigid())
            }
        };
        if !matches!(self.immobile_type, ImmobileType::AllMotion) {
            self.reference = Some(avatar.inverse_rigid().mul(&reference));
        }
        let places: Vec<(ColliderAt, Transform)> = self
            .colliders
            .iter()
            .filter_map(|&c| colliders.get(c))
            .filter(|c| crate::active_in_hierarchy(&frame.active, parent, c.node))
            .map(|c| (c.clone(), node_world(frame, Some(c.node))))
            .collect();
        let touches: Vec<Touch> = hands.iter().copied().filter(|h| allowed(&self.collision, h.owner)).collect();
        let scale = self.parent_world(frame).scale.x.abs();
        let settled: Vec<Vec3> = self.bones.iter().map(|b| b.position).collect();
        let actual = avatar.rotation.inverse() * self.rest[0].rotation;
        let turned = slerp(self.turned.unwrap_or(actual), actual, self.follow_share());
        self.turned = Some(turned);
        self.bones[0].position = self.follow(frame, 0, self.rest[0].translation);
        self.bones[0].previous = self.bones[0].position;
        self.world[0] = self.base(frame, 0);
        self.held = vec![false; self.bones.len()];
        if let Some(g) = self.grab {
            self.reach(g);
        }
        for p in 0..self.bones.len() {
            let children = self.bones[p].children.clone();
            for &i in &children {
                met_inside |= self.solve(p, i, frame, moved, root_motion, &places, &touches, scale, &settled);
            }
            let driving = self.aim(p, frame);
            for &i in &children {
                if driving.len() != 1 || driving[0] != i {
                    let local = self.local(frame, i);
                    self.bones[i].shown = self.world[p].point(local.translation);
                    self.bones[i].position = self.follow(frame, i, self.bones[i].shown);
                    self.bones[i].strain = 0.0;
                }
                self.world[i] = self.base(frame, i);
            }
        }
        self.measure();
        self.drawn_behind |= at_rest && met_inside;
    }

    /// Where the simulation puts `bone`, which the chain pins to its animated place `at`, from where it put it last step.
    /// It takes the avatar root's own motion at once and a change of the avatar's pose by [`FOLLOW`] of what remains each
    /// step: the SDK's chains under a parent that jumps turn as if their root arrived that late, while a chain on an avatar
    /// moving through the world shows no such lag. Tracked motion is a change of pose too, so at a steady speed the
    /// simulated root trails by a fifth of a step.
    fn follow(&mut self, frame: &Frame, bone: usize, at: Vec3) -> Vec3 {
        let avatar = frame.root.rigid();
        let share = self.follow_share();
        let pinned = &mut self.pinned[bone];
        *pinned = pinned.lerp(avatar.inverse_rigid().point(at), share);
        avatar.point(*pinned)
    }

    fn follow_share(&self) -> f32 {
        match self.version {
            PhysBoneVersion::V1_0 => 1.0,
            PhysBoneVersion::V1_1 => FOLLOW,
        }
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
        let lag = self.world[0].translation - root;
        for (&b, x) in path.iter().zip(at).skip(1) {
            self.bones[b].position = x;
            self.bones[b].previous = x;
            self.bones[b].shown = x + lag;
            self.bones[b].strain = 0.0;
            self.bones[b].resting = false;
            self.held[b] = true;
        }
    }

    #[allow(clippy::too_many_arguments)]
    /// Steps bone `i`, a child of `p`, and returns whether a sphere or capsule met it with its end inside.
    fn solve(&mut self, p: usize, i: usize, frame: &Frame, moved: Transform, root_motion: Transform, places: &[(ColliderAt, Transform)], touches: &[Touch], scale: f32, settled: &[Vec3]) -> bool {
        if self.held[i] {
            return false;
        }
        // A child's rest direction and limit turn with its parent's simulated direction, not its drawn one: from the drawn
        // one a short bone behind a swinging bone turns over its parent where the SDK's falls back.
        let mut parent = self.world[p];
        if let Some(g) = self.bones[p].parent {
            let drawn = (parent.translation - self.world[g].translation).normalize_or_zero();
            let simulated = (self.bones[p].position - self.bones[g].position).normalize_or_zero();
            if drawn != Vec3::ZERO && simulated != Vec3::ZERO {
                parent.rotation = (Quat::from_rotation_arc(drawn, simulated) * parent.rotation).normalize();
            }
        }
        let local = self.local(frame, i);
        let origin = self.bones[p].position;
        let rest = parent.point(local.translation) - parent.translation;
        let rest_length = rest.length();
        if rest_length <= f32::EPSILON {
            self.bones[i].position = origin;
            self.bones[i].previous = origin;
            self.bones[i].shown = parent.translation;
            self.bones[i].strain = 0.0;
            return false;
        }
        let rest_direction = rest / rest_length;
        let q = self.bones[i].params;
        let radii = (self.bones[p].params.radius * scale, q.radius * scale);
        let at_reset = (frame.root.rigid().rotation * self.reset_directions[i]).normalize_or(rest_direction);
        let gravity = q.gravity * (1.0 - q.gravity_falloff * tilt_cosine((self.bones[i].position - origin).normalize_or(rest_direction), at_reset));
        let target = match self.version {
            PhysBoneVersion::V1_0 => origin + rest,
            PhysBoneVersion::V1_1 => {
                let down = if gravity >= 0.0 { Vec3::NEG_Y } else { Vec3::Y };
                origin + rest_direction.lerp(down, gravity.abs().min(1.0)).normalize_or(rest_direction) * rest_length
            }
        };
        let child = self.bones[p].parent.is_some();
        let carry = origin - settled[p];
        let world_carry = root_motion.point(settled[p]) - settled[p];
        // All-motion immobile carries its share of a change of pose below, so stiffness carries a share of the rest.
        let carried_by_immobile = if matches!(self.immobile_type, ImmobileType::AllMotion) { q.immobile } else { 0.0 };
        // A child's carry leaves out the share of its parent's turn that immobile gave the parent, which immobile gives the
        // child itself below: carried again, a chain under a parent turned in one step bends at its second bone where the
        // SDK's does not.
        let parent_immobile_turn = match (self.bones[p].parent, self.immobile_type) {
            (Some(g), ImmobileType::AllMotion) => {
                let qp = self.bones[p].params;
                immobile_turn(moved, settled[g], settled[p], qp.immobile * (1.0 - qp.stiffness))
            }
            _ => Vec3::ZERO,
        };
        let pose_carry = (carry - world_carry - parent_immobile_turn) * (1.0 - carried_by_immobile);
        let (carried, into_velocity) = match self.integration {
            IntegrationType::Simplified => (Vec3::ZERO, Vec3::ZERO),
            IntegrationType::Advanced => (
                world_carry * advanced_carry(q.stiffness) + pose_carry * (q.stiffness * POSE_CARRY_IN_PLACE),
                pose_carry * (q.stiffness * POSE_CARRY_IN_VELOCITY).min(1.0 - q.stiffness * POSE_CARRY_IN_PLACE),
            ),
        };
        let bone = &mut self.bones[i];
        bone.position += carried;
        bone.previous += carried;
        match self.integration {
            IntegrationType::Simplified => {
                bone.position += (moved.point(bone.position) - bone.position) * q.immobile;
                bone.previous += (moved.point(bone.previous) - bone.previous) * q.immobile;
            }
            IntegrationType::Advanced => {
                let shift = (moved.point(origin) - origin) * q.immobile;
                let share = q.immobile * (1.0 - q.stiffness);
                let (position, previous) = (immobile_turn(moved, origin, bone.position, share), immobile_turn(moved, origin, bone.previous, share));
                bone.position += shift + position;
                bone.previous += shift + previous;
            }
        }
        let velocity = bone.position - bone.previous;
        bone.previous = bone.position;
        bone.position += into_velocity;
        let weight = match self.version {
            PhysBoneVersion::V1_0 => Vec3::NEG_Y * (gravity * rest_length),
            PhysBoneVersion::V1_1 => Vec3::ZERO,
        };
        let pull = (target - bone.position) * q.pull * if matches!(self.integration, IntegrationType::Advanced) { child_pull_share(q, child) } else { 1.0 } + weight;
        let start = bone.position;
        bone.position += match self.integration {
            IntegrationType::Simplified => velocity * q.spring + pull * (1.0 - q.spring),
            IntegrationType::Advanced => (velocity * q.spring * advanced_damping(q) + pull) * (1.0 - q.stiffness),
        };
        // From rest the SDK leaves a bone where it is until its step first passes this, so a chain under a weak pull can
        // rest along its rest direction where gravity alone would bend it.
        if bone.resting && bone.position.distance(start) < rest_length * STEP_FROM_REST {
            bone.position = start;
        } else {
            bone.resting = false;
        }
        // Simplified integration's next velocity is this step's own move: keeping the return to the bone's length and
        // limits in it, as advanced integration does, misses the SDK's traces at spring 0.6 to 0.95 several times over.
        // Version 1.0 gravity along the bone leaves it too, or the bent gravity probes miss by over half as much again.
        let mut momentum = bone.position - start;
        let (low, high) = match self.version {
            PhysBoneVersion::V1_0 => (1.0, 1.0 + q.max_stretch),
            PhysBoneVersion::V1_1 => (1.0 - q.max_squish, 1.0 + q.max_stretch),
        };
        let offset = bone.position - origin;
        let share = match (self.grab, self.version, self.integration) {
            (Some(_), ..) => 1.0,
            (None, PhysBoneVersion::V1_0, _) => 0.0,
            (None, PhysBoneVersion::V1_1, IntegrationType::Simplified) => q.stretch_motion,
            (None, PhysBoneVersion::V1_1, IntegrationType::Advanced) => q.stretch_motion * (1.0 - q.stiffness * STRETCH_STIFFNESS_SHARE),
        };
        let clamped = if share > 0.0 { offset.length().clamp(rest_length * low, rest_length * high) } else { rest_length };
        let length = if share > 0.0 {
            let want = clamped / rest_length - 1.0;
            let from = if want * bone.strain > 0.0 { bone.strain } else { 0.0 };
            rest_length * (1.0 + if want.abs() > from.abs() { from + (want - from) * share } else { want })
        } else {
            rest_length
        };
        let limited = |d: Vec3| limit(d, local.translation, parent.rotation, q.limit);
        let lag = match (child, self.turned) {
            (false, Some(turned)) => ((frame.root.rigid().rotation * turned) * self.rest[0].rotation.inverse()).normalize(),
            _ => Quat::IDENTITY,
        };
        let direction = limit(offset.normalize_or(rest_direction), local.translation, lag * parent.rotation, q.limit);
        bone.position = origin + direction * length;
        let from = parent.translation;
        // Where the drawn bone points, kept within the limit; the colliders' and hands' pushes below move it as they
        // move the simulated bone.
        let drawn = from + limited((bone.position - from).normalize_or(direction)) * length - bone.position;
        bone.strain = length / rest_length - 1.0;
        momentum += direction * (length - clamped - (weight * (1.0 - q.spring)).dot(direction));
        let keep = |at: Vec3| {
            let pushed = at - origin;
            let kept = if share > 0.0 { pushed.length().clamp(rest_length * low, rest_length * high) } else { rest_length };
            origin + pushed.normalize_or(direction) * kept
        };
        let segment_push = match (self.version, self.integration) {
            (PhysBoneVersion::V1_1, _) => SegmentPush::Plain,
            (PhysBoneVersion::V1_0, IntegrationType::Advanced) => SegmentPush::TurnOver,
            (PhysBoneVersion::V1_0, IntegrationType::Simplified) => SegmentPush::TurnOverOrMeetInside,
        };
        let (mut pushed_any, mut met_inside) = (false, false);
        for (collider, place) in places {
            let pushed = match collider.core(place).filter(|_| collider.meets_segment) {
                Some(core @ (a, b, r)) => {
                    let pushed = touch_segment(origin, bone.position, radii, core, segment_push);
                    met_inside |= pushed != bone.position && bone.position.distance(closest_on_segments(bone.position, bone.position, a, b).1) < r;
                    pushed
                }
                None => collide(bone.position, q.radius * scale, collider, place),
            };
            let out = match (collider.meets_segment, self.version) {
                (false, _) => 0.0,
                (true, PhysBoneVersion::V1_0) => 1.0,
                (true, PhysBoneVersion::V1_1) => PUSH_OUT_OF_VELOCITY,
            };
            momentum += (pushed - bone.position) * (1.0 - out);
            pushed_any |= pushed != bone.position;
            bone.position = keep(pushed);
        }
        for touch in touches {
            let pushed = touch_segment(origin, bone.position, radii, (touch.a, touch.b, touch.radius), SegmentPush::Plain);
            momentum += (pushed - bone.position) * (1.0 - PUSH_OUT_OF_VELOCITY);
            pushed_any |= pushed != bone.position;
            bone.position = keep(pushed);
        }
        // The SDK stops a bone a collider pushes at its limit: the push comes after the limit above.
        if pushed_any {
            let pushed = bone.position - origin;
            bone.position = origin + limit(pushed.normalize_or(direction), local.translation, lag * parent.rotation, q.limit) * pushed.length();
        }
        if matches!(self.integration, IntegrationType::Simplified) {
            bone.previous = bone.position - momentum;
        }
        bone.shown = from + (bone.position + drawn - from).normalize_or(direction) * bone.position.distance(origin);
        met_inside
    }

    /// Turns `bone` toward the children that drive it, which it returns.
    fn aim(&mut self, bone: usize, frame: &Frame) -> Vec<usize> {
        let driving = self.driving(bone);
        if driving.is_empty() {
            return driving;
        }
        let w = self.world[bone];
        let mut sum = Vec4::ZERO;
        for &c in &driving {
            let rest = w.rotation * (w.scale * self.local(frame, c).translation);
            let turn = Vec4::from(Quat::from_rotation_arc(rest.normalize_or_zero(), (self.bones[c].shown - w.translation).normalize_or_zero()));
            sum += if sum.dot(turn) < 0.0 { -turn } else { turn };
        }
        let turn = Quat::from_vec4(sum).normalize();
        self.world[bone].rotation = (turn * w.rotation).normalize();
        driving
    }

    /// The children that turn `bone` toward them.
    fn driving(&self, bone: usize) -> Vec<usize> {
        let children = &self.bones[bone].children;
        match (self.multi_child, children.len()) {
            (_, 0) | (MultiChildType::Ignore, 2..) => vec![],
            (_, 1) | (MultiChildType::Average, _) => children.clone(),
            (MultiChildType::First, _) => children[..1].to_vec(),
        }
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

    /// Each bone's drawn place: for a chain drawn behind its step, blended in the world rather than by local rotation, so
    /// that a root aimed at its child that turns over in one step is not drawn where neither step held it.
    fn drawn(&self) -> Vec<Transform> {
        let mut drawn = self.world.clone();
        if !self.drawn_behind {
            return drawn;
        }
        for i in 0..drawn.len() {
            let Some(p) = self.bones[i].parent else { continue };
            let newest = self.world[i].translation - self.world[p].translation;
            let toward = self.stepped_from[i].translation.lerp(self.world[i].translation, DRAWN_SHARE) - drawn[p].translation;
            drawn[i].translation = drawn[p].translation + toward.normalize_or(newest.normalize_or_zero()) * newest.length();
        }
        let turn = |i: usize, c: usize| {
            let from = (self.world[c].translation - self.world[i].translation).normalize_or_zero();
            let to = (drawn[c].translation - drawn[i].translation).normalize_or_zero();
            Vec4::from(Quat::from_rotation_arc(from, to))
        };
        let turns: Vec<Vec4> = (0..drawn.len())
            .map(|i| {
                let driving = self.driving(i);
                let mut sum = Vec4::ZERO;
                for t in driving.iter().map(|&c| turn(i, c)).chain(self.bones[i].parent.filter(|_| driving.is_empty()).map(|p| turn(p, i))) {
                    sum += if sum.dot(t) < 0.0 { -t } else { t };
                }
                sum
            })
            .collect();
        for (i, sum) in turns.into_iter().enumerate() {
            if sum != Vec4::ZERO {
                drawn[i].rotation = (Quat::from_vec4(sum).normalize() * self.world[i].rotation).normalize();
            }
        }
        drawn
    }

    fn locals(&self, frame: &Frame) -> Vec<Transform> {
        let root_parent = self.parent_world(frame);
        let world = self.drawn();
        self.bones
            .iter()
            .enumerate()
            .filter(|(_, b)| b.node.is_some())
            .map(|(i, b)| {
                let parent = b.parent.map_or(root_parent, |p| world[p]);
                let inverse = parent.rotation.inverse();
                let unscale = Vec3::select(parent.scale.cmpeq(Vec3::ZERO), Vec3::ZERO, parent.scale.recip());
                Transform {
                    translation: inverse * (world[i].translation - parent.translation) * unscale,
                    rotation: (inverse * world[i].rotation).normalize(),
                    scale: world[i].scale * unscale,
                }
            })
            .collect()
    }
}

/// Advanced integration carries a bone along with its parent's movement from the avatar's own motion by
/// `CARRY_PER_STIFFNESS` times stiffness at low stiffness, nearing all of it as stiffness nears 1; measured against the
/// reference traces at stiffness 0.1 to 0.8.
fn advanced_carry(stiffness: f32) -> f32 {
    (stiffness * CARRY_PER_STIFFNESS).min(1.0 - (1.0 - stiffness).powf(CARRY_EXPONENT))
}
const CARRY_PER_STIFFNESS: f32 = 1.45;
/// The smallest step of its own, over the bone's length, that frees a bone held since its chain's reset: fitted to the chains
/// resting under a tilted parent, where 0.0012 leaves a chain the SDK holds bending and 0.0017 holds one the SDK bends.
const STEP_FROM_REST: f32 = 0.0015;

/// The steps after an avatar loads that no hand collider takes part in: the SDK's step at the file's pose and its
/// first frame's, which come before the hand colliders a recording adds on that frame join.
const UNTOUCHED_STEPS: u32 = 2;

/// The share of a step more that the SDK's chains take at the file's pose: Unity's first frame after load lasts its fixed
/// timestep, 0.02 s, 1.2 steps. On the probe scene's first recorded frame, a step after that, its gravity chains and its
/// chains whose root starts inside a collider are where 2.2 steps take them: with the load frame 1.1, 1.15, 1.25 or 1.3
/// steps long the gravity chains are 6, 4, 2 and 5 times as far off as at 1.2. A clock that stays 0.2 steps ahead after
/// it misses the jump cases.
const LOAD_CARRY: f32 = 0.2;

/// How fast advanced integration's carry nears all of the parent's movement as stiffness rises.
const CARRY_EXPONENT: f32 = 1.6;
/// Advanced integration carries a bone with what all-motion immobile leaves of a change of its parent's pose, apart from
/// the avatar's own motion, by stiffness times `POSE_CARRY_IN_PLACE` in place and times `POSE_CARRY_IN_VELOCITY` more,
/// which also stays in its next step's velocity, never more than all of that between them. Fitted to the stiffness jump
/// cases at stiffness 0.1 and 0.3, with and without immobile: all of it in place leaves the short bones behind a
/// swinging bone too still, and all of it in velocity throws 2 cm bones under the 20 cm jump most of their length.
const POSE_CARRY_IN_PLACE: f32 = 0.725;
const POSE_CARRY_IN_VELOCITY: f32 = 0.5;
/// The share of its pull an advanced bone below another simulated bone keeps: with stiffness the SDK's second and third
/// bones of a chain returning from a turn of its parent trail their parent's return further than the full pull leaves
/// them. Fitted to the turn rows at stiffness 0.133 and 0.3 (spring 0.727, pull 0.053), and held near 1 by the jump cases
/// at pull 0.1 and the probes at pull 0.2, which the full pull fits. Without it the turn rows' third bones miss by up to
/// 1.7% over the return.
fn child_pull_share(q: Params, child: bool) -> f32 {
    if !child || q.stiffness <= 0.0 {
        return 1.0;
    }
    let hump = q.stiffness * (1.0 - q.stiffness).powi(CHILD_PULL_STIFFNESS_EXPONENT);
    1.0 - CHILD_PULL_LOSS * q.spring * hump * (1.0 - q.pull / CHILD_PULL_FULL_AT).max(0.0)
}
const CHILD_PULL_LOSS: f32 = 2.6;
const CHILD_PULL_STIFFNESS_EXPONENT: i32 = 6;
const CHILD_PULL_FULL_AT: f32 = 0.15;

/// The share of a bone's momentum advanced integration keeps after its pull. Without stiffness the SDK moves the bone
/// by its momentum and then toward its target by pull, so pull takes its share of the momentum too and a chain at
/// spring 1 settles. With stiffness the carry and stretch share above, fitted without this damping, already stand in
/// for it: the full damping misses the probes at stiffness 0.2 to 0.8 by up to three times their bounds, and a refit of
/// those constants with it stays above them. `PULL_DAMPING_STIFFNESS_EXPONENT` fades it out, fitted to the jump rows at
/// stiffness 0, 0.1, 0.15 and 0.3 and the probes at 0 to 0.8.
fn advanced_damping(q: Params) -> f32 {
    1.0 - q.pull * (1.0 - q.stiffness).powi(PULL_DAMPING_STIFFNESS_EXPONENT)
}
const PULL_DAMPING_STIFFNESS_EXPONENT: i32 = 15;
/// The share of what remains of a pinned bone's change of place that the simulation takes each step: the one that leaves
/// it a fifth of a step behind a steady change of pose. It fits the jump cases at pull 0 and spring 0, where only the
/// projection onto a bone's length moves a bone, and at spring 0.9 a jump that leaves a bone's old tip near its new root
/// turns that bone's momentum through so wide an angle that the share must be this one, not a rounding of it. Every chain
/// those cases hold is version 1.1; a version 1.0 chain takes the change at once, as VRM's springs do.
const FOLLOW: f32 = 5.0 / 6.0;
/// How far into its newest step the SDK draws a chain that a sphere or capsule met at rest with a bone's end inside, from
/// then on: the probe's chains a collider is brought onto at rest, or whose first bone starts wholly inside one, lie this
/// share of the way from the core's previous step to its newest, while chains swung against a collider, met at load only
/// along their segments, or under a jumping parent lie at their newest.
const DRAWN_SHARE: f32 = 0.2;
/// The share of stiffness that advanced integration takes from stretch motion's share, measured against the reference
/// traces at stiffness 0.3 and 0.7.
const STRETCH_STIFFNESS_SHARE: f32 = 0.8;
/// The share of a hand's push, and of a version 1.1 chain's push from a collider that meets its segment, that simplified
/// integration leaves out of the bone's next velocity, measured against the hand probe's chains at pull 0.05 to 0.6 and
/// spring 0.3 and 0.9; advanced integration keeps all of it in. A version 1.0 chain keeps none of such a collider's push:
/// the bent chain whose root starts inside a sphere rests several times further from the SDK's at this share, where its
/// version 1.1 twin rests 0.4 mm from it at this share and 10 mm keeping none.
const PUSH_OUT_OF_VELOCITY: f32 = 0.8;

/// How much of gravity's falloff applies: the cosine between `bone`'s and `reset`'s tilts from horizontal, 1 while
/// the bone keeps its tilt at reset whichever way it turns about the vertical. Past a quarter turn it is 0, so falloff
/// never adds gravity; the traces reach no such tilt.
fn tilt_cosine(bone: Vec3, reset: Vec3) -> f32 {
    let level = |d: Vec3| (d.x * d.x + d.z * d.z).sqrt();
    (level(bone) * level(reset) + bone.y * reset.y).max(0.0)
}

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
            let angle = libm::atan2f(d.z, d.y).clamp(-max, max);
            Vec3::new(0.0, libm::cosf(angle), libm::sinf(angle))
        }
        LimitAt::Polar { max_x, max_z, .. } => {
            let pitch = libm::atan2f(d.z, d.y).clamp(-max_x, max_x);
            let yaw = libm::atan2f(d.x, libm::hypotf(d.y, d.z)).clamp(-max_z, max_z);
            Vec3::new(libm::sinf(yaw), libm::cosf(yaw) * libm::cosf(pitch), libm::cosf(yaw) * libm::sinf(pitch))
        }
    };
    rotation * limited
}

/// Pushes a bone's end at `position` out of the collider, or keeps it inside an inside-bounds one; the caller then
/// puts it back at a length its stretch allows, as VRM's spring bones do. A bone collides with a plane, or with a sphere or
/// capsule set to treat bones as spheres, as a sphere of its radius at its end, and one of radius 0 passes through spheres
/// and capsules: as the reference traces show. Other spheres and capsules meet the bone's whole segment ([`touch_segment`]).
fn collide(position: Vec3, radius: f32, collider: &ColliderAt, place: &Transform) -> Vec3 {
    let Some((a, b, r)) = collider.core(place) else {
        let axis = (place.rotation * collider.rotation) * Vec3::Y;
        let d = (position - place.point(collider.position)).dot(axis) - radius;
        return if d < 0.0 { position - axis * d } else { position };
    };
    let (_, core) = closest_on_segments(position, position, a, b);
    let d = position - core;
    if matches!(collider.shape, ColliderShape::Sphere { inside_bounds: true, .. } | ColliderShape::Capsule { inside_bounds: true, .. }) {
        let limit = (r - radius).max(0.0);
        return if d.length() > limit { core + d.normalize_or_zero() * limit } else { position };
    }
    let reach = r + radius;
    if radius <= 0.0 || d.length() >= reach {
        return position;
    }
    core + d.normalize_or((b - a).normalize_or(Vec3::Y).any_orthonormal_vector()) * reach
}

/// How a collider's push on a bone's segment differs by the bone's chain; hands push every chain [`SegmentPush::Plain`].
#[derive(Clone, Copy)]
enum SegmentPush {
    Plain,
    TurnOver,
    TurnOverOrMeetInside,
}

/// Under [`SegmentPush::TurnOverOrMeetInside`], where a bone whose `origin` is inside the collider is met, as a share
/// of its radius at `origin` from `origin` ([`INSIDE_CONTACT`]), and the share of the push it takes there
/// ([`INSIDE_PUSH`]). The probe row of the bent chain whose root starts 3.5 cm inside a sphere, alone and with each of
/// the fixture tail's fields, rests 2 mm above the SDK's under gravity 0.4 and 3.4 mm below it under 0.2 when met at
/// the radius with all of the push; met nearer `origin` the SDK's rest moves more with gravity, and these two shares
/// together take the row's tips from 1.56% of chain length (RMS) to 1.21%. Version 1.0 chains under advanced
/// integration whose root starts 4 mm and 3.5 cm inside a sphere or capsule are within 0.5% of the SDK's met at the
/// radius with all of the push, and up to 4.2% off at the first recorded frame met so. Keeping a share of the push in
/// velocity instead moves the whole row alike, under either gravity. Only chains whose root starts inside were
/// measured.
const INSIDE_CONTACT: f32 = 0.6;
const INSIDE_PUSH: f32 = 0.78;

/// Pushes a bone's end at `position` by all of the push that takes one point of its segment from `origin` out of the
/// capsule `(a, b, r)`, a hand's or a collider's (a sphere's has `a == b`): the point nearest the capsule's point
/// nearest the end, but no nearer `origin` than the bone's radius at `origin`. The hand probe's one-bone chains, which
/// move only by the push, stop where that point leaves the reach, at every height and angle around the capsule, and
/// along its axis past either end; the probe scene's chains whose root starts inside a capsule or sphere are where the
/// SDK's are on the first recorded frame and settle where they do. Under the version 1.0 rules, a bone whose `origin`
/// is outside the capsule and whose pushed point is not held at the radius's distance from it is pushed as far, but
/// along the line from the capsule's point nearest the end through the end: a version 1.0 bone swung across a capsule
/// then turns over it to the far side once its end has passed the capsule's axis, as the SDK's does, where otherwise it
/// stays on the near side and the swung row's version 1.0 simplified capsule chains miss by 10–11% of chain length.
/// Version 1.1 chains pushed so miss by up to 6%, the bent version 1.0 chain whose root starts inside a sphere, pushed
/// so from there, by 23% where it otherwise misses by 4%, and version 1.0 chains resting against a capsule with a
/// rising radius curve move off their bounds when the held point is pushed so too.
fn touch_segment(origin: Vec3, position: Vec3, (at_origin, at_end): (f32, f32), (a, b, r): (Vec3, Vec3, f32), rule: SegmentPush) -> Vec3 {
    let segment = position - origin;
    let length = segment.length();
    if length <= 0.0 {
        return position;
    }
    let inside = origin.distance(closest_on_segments(origin, origin, a, b).1) < r;
    let deep = matches!(rule, SegmentPush::TurnOverOrMeetInside) && inside;
    let (_, core) = closest_on_segments(position, position, a, b);
    let nearest = (core - origin).dot(segment) / (length * length);
    let lowest = (at_origin * if deep { INSIDE_CONTACT } else { 1.0 } / length).min(1.0);
    let along = nearest.clamp(lowest, 1.0);
    let radius = at_origin + (at_end - at_origin) * along;
    if radius <= 0.0 {
        return position;
    }
    let d = origin + segment * along - core;
    let reach = r + radius;
    if d.length() >= reach {
        return position;
    }
    let away = d.normalize_or((b - a).normalize_or(Vec3::Y).any_orthonormal_vector());
    let away = if !matches!(rule, SegmentPush::Plain) && nearest > lowest && !inside { (position - core).normalize_or(away) } else { away };
    position + away * (reach - d.length()) * if deep { INSIDE_PUSH } else { 1.0 }
}

/// How far immobile's `share` of the turn in `moved` carries `x` about `origin`.
fn immobile_turn(moved: Transform, origin: Vec3, x: Vec3, share: f32) -> Vec3 {
    (moved.rotation * (x - origin) - (x - origin)) * share
}
