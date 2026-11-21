//! One avatar's frame, evaluated in VRChat's order: the animator, the constraints that read nothing a PhysBone
//! moves, PhysBones, the constraints that do, then contacts. Receiver and PhysBone parameters reach the next frame's
//! animator. A runtime drives an [`Evaluator`] and applies its [`Output`]; it composes none of these itself.

use glam::{Quat, Vec3};

use crate::animator::motion::{axis_index, unit, Target};
use crate::animator::Animator;
use crate::constraints::{Constraints, Scene};
use crate::contacts::{Contacts, Sender};
use crate::humanoid::{muscle_names, Body, Human};
use crate::physbones::{Error, Frame, HandCollider, Nodes, PhysBones, Toucher, Transform};
use crate::Avatar;

/// What the host gives each frame.
pub struct Input<'a> {
    /// Seconds since the last frame. The animator does not advance on a frame with none.
    pub dt: f32,
    /// The avatar root in the world. Its scale is not read: the root node's local scale is the avatar's.
    pub root: Transform,
    /// Poses the host sets after animation and before dynamics, in the avatar's root space: tracked or IK-solved
    /// bones.
    pub poses: &'a [(usize, Transform)],
    /// Poses that replace what PhysBones leave and hold through the constraints solved after them, in the avatar's
    /// root space: a replay of recorded dynamics. A runtime passes none.
    pub held: &'a [(usize, Transform)],
    /// Contact senders outside the avatar: other avatars' and the hands'.
    pub senders: &'a [Sender],
    /// Other avatars' hand and finger colliders, each from its own avatar's [`Evaluator::hands`]; never this one's own,
    /// which it already holds.
    pub hands: &'a [HandCollider],
}

/// What animation leaves, given by a host that does not run the core's animator on this avatar.
pub struct Animated<'a> {
    /// Every node's local pose.
    pub locals: &'a [Transform],
    /// Every node's own active flag.
    pub active: &'a [bool],
    /// Every PhysBone's enabled flag.
    pub physbones: &'a [bool],
    /// The constraint and contact fields animation sets.
    pub fields: &'a [(Field, f32)],
}

/// One frame's result. Blendshapes, material properties and slots are those the animator binds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Output {
    /// Every node's local pose.
    pub locals: Vec<Transform>,
    /// Every node's pose in the avatar's root space, the root node's local scale included.
    pub space: Vec<Transform>,
    /// Every node's own active flag.
    pub active: Vec<bool>,
    /// Each renderer's enabled flag, in the model's renderer order.
    pub renderers: Vec<bool>,
    /// (renderer node, blendshape, weight from 0 to 100).
    pub blendshapes: Vec<(u32, String, f32)>,
    /// (renderer node, property with its component, value).
    pub materials: Vec<(u32, String, f32)>,
    /// (renderer node, slot, material index).
    pub slots: Vec<(u32, u32, u32)>,
    /// (particle system node, serialized property, value).
    pub particles: Vec<(u32, String, f32)>,
}

#[derive(Default)]
struct TransformTargets {
    translation: [Option<usize>; 3],
    rotation: [Option<usize>; 4],
    scale: [Option<usize>; 3],
}

/// A constraint or contact field an animation curve binds, by component index and the name the curve binds it with.
#[derive(Clone, Debug, PartialEq)]
pub enum Field {
    Constraint(usize, String),
    Sender(usize, String),
    Receiver(usize, String),
}

pub struct Evaluator {
    nodes: Nodes,
    animator: Animator,
    constraints: Constraints,
    physbones: PhysBones,
    contacts: Contacts,
    transforms: Vec<(usize, TransformTargets)>,
    node_active: Vec<(usize, usize)>,
    physbone_enabled: Vec<(usize, usize)>,
    renderer_enabled: Vec<(usize, usize)>,
    fields: Vec<(usize, Field)>,
    blendshapes: Vec<(usize, u32, String)>,
    materials: Vec<(usize, u32, String)>,
    slots: Vec<(usize, u32, u32)>,
    particles: Vec<(usize, u32, String)>,
    physbone_parameters: Vec<Option<[String; 5]>>,
    authored_active: Vec<bool>,
    authored_enabled: Vec<bool>,
    authored_renderers: Vec<bool>,
    renderer_nodes: Vec<u32>,
    human: Option<HumanTargets>,
    root: Transform,
    output: Output,
}

/// The humanoid bones' poser and where the animator holds what it reads.
struct HumanTargets {
    human: Human,
    muscles: Vec<Option<usize>>,
    body: [Option<usize>; 7],
    rest: [f32; 7],
}

impl Evaluator {
    pub fn new(avatar: &Avatar) -> Result<Evaluator, Error> {
        let nodes = Nodes::read(avatar)?;
        let animator = Animator::new(avatar);
        let m = &avatar.model;
        let count = nodes.parent.len();
        let mut transforms: Vec<Option<TransformTargets>> = (0..count).map(|_| None).collect();
        let mut e = Evaluator {
            constraints: Constraints::new(avatar)?,
            physbones: PhysBones::new(avatar)?,
            contacts: Contacts::new(avatar)?,
            transforms: vec![],
            node_active: vec![],
            physbone_enabled: vec![],
            renderer_enabled: vec![],
            fields: vec![],
            blendshapes: vec![],
            materials: vec![],
            slots: vec![],
            particles: vec![],
            physbone_parameters: m
                .physbones
                .iter()
                .map(|p| p.parameter.as_ref().map(|n| ["IsGrabbed", "IsPosed", "Angle", "Stretch", "Squish"].map(|s| format!("{n}_{s}"))))
                .collect(),
            authored_active: (0..count).map(|n| m.nodes.get(n).is_none_or(|x| x.active)).collect(),
            authored_enabled: m.physbones.iter().map(|p| p.enabled).collect(),
            authored_renderers: m.renderers.iter().map(|r| r.enabled).collect(),
            renderer_nodes: m.renderers.iter().map(|r| r.node.0).collect(),
            human: None,
            root: Transform::IDENTITY,
            output: Output {
                locals: nodes.rest.clone(),
                space: vec![Transform::IDENTITY; count],
                active: vec![true; count],
                ..Output::default()
            },
            nodes,
            animator,
        };
        for (i, t) in e.animator.targets().iter().enumerate() {
            let slot = match t {
                Target::Translation(n, _) | Target::Rotation(n, _) | Target::Scale(n, _) => transforms.get_mut(*n as usize).map(|t| t.get_or_insert_with(TransformTargets::default)),
                _ => None,
            };
            match t {
                Target::Translation(_, a) => slot.into_iter().for_each(|t| t.translation[axis_index(*a)] = Some(i)),
                Target::Rotation(_, a) => slot.into_iter().for_each(|t| t.rotation[axis_index(*a)] = Some(i)),
                Target::Scale(_, a) => slot.into_iter().for_each(|t| t.scale[axis_index(*a)] = Some(i)),
                Target::NodeActive(n) if (*n as usize) < count => e.node_active.push((i, *n as usize)),
                Target::RendererEnabled(n) => {
                    if let Some(r) = e.renderer_nodes.iter().position(|x| x == n) {
                        e.renderer_enabled.push((i, r));
                    }
                }
                Target::PhysBone(p, f) if f == "m_Enabled" && (*p as usize) < e.authored_enabled.len() => e.physbone_enabled.push((i, *p as usize)),
                Target::Constraint(c, f) => e.fields.push((i, Field::Constraint(*c as usize, f.clone()))),
                Target::ContactSender(c, f) => e.fields.push((i, Field::Sender(*c as usize, f.clone()))),
                Target::ContactReceiver(c, f) => e.fields.push((i, Field::Receiver(*c as usize, f.clone()))),
                Target::BlendShape(n, s) => e.blendshapes.push((i, *n, s.clone())),
                Target::MaterialProperty(n, p) => e.materials.push((i, *n, p.clone())),
                Target::Slot(n, s) => e.slots.push((i, *n, *s)),
                Target::ParticleSystem(n, p) => e.particles.push((i, *n, p.clone())),
                _ => {}
            }
        }
        e.human = Human::new(&m.humanoid).map(|human| {
            let find = |name: &str| e.animator.targets().iter().position(|t| matches!(t, Target::Muscle(n) if n == name));
            let rest = m.humanoid.rest.as_ref().map_or([0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0], |r| {
                let (p, q) = (r.body.position, r.body.rotation);
                [p[0], p[1], p[2], q[0], q[1], q[2], q[3]]
            });
            HumanTargets {
                human,
                muscles: muscle_names().into_iter().map(find).collect(),
                body: ["RootT.x", "RootT.y", "RootT.z", "RootQ.x", "RootQ.y", "RootQ.z", "RootQ.w"].map(find),
                rest,
            }
        });
        e.transforms = transforms.into_iter().enumerate().filter_map(|(n, t)| t.map(|t| (n, t))).collect();
        e.output.active.clone_from(&e.authored_active);
        e.output.renderers.clone_from(&e.authored_renderers);
        e.output.space = space(&e.nodes.parent, &e.output.locals, &[]);
        e.write_bound();
        Ok(e)
    }

    pub fn animator(&self) -> &Animator {
        &self.animator
    }

    /// Sets an expression parameter; [`Animator::set`].
    pub fn set(&mut self, name: &str, value: f32) {
        self.animator.set(name, value)
    }

    /// Sets a built-in parameter from the host's input; [`Animator::set_builtin`]. `IsLocal` also decides which
    /// receivers run.
    pub fn set_builtin(&mut self, name: &str, value: f32) -> bool {
        self.animator.set_builtin(name, value)
    }

    /// [`Animator::set_eyes`].
    pub fn set_eyes(&mut self, blink: f32, look_y: f32) {
        self.animator.set_eyes(blink, look_y)
    }

    /// [`Animator::set_head`].
    pub fn set_head(&mut self, head: Transform) {
        self.animator.set_head(head)
    }

    /// [`Animator::bind_default`].
    pub fn bind_default(&mut self, target: &Target, value: f32) {
        self.animator.bind_default(target, value)
    }

    /// [`PhysBones::grab`].
    pub fn grab(&mut self, physbone: usize, node: usize, at: Vec3, who: Toucher) -> bool {
        self.physbones.grab(physbone, node, at, who)
    }

    /// [`PhysBones::hold`].
    pub fn hold(&mut self, physbone: usize, at: Vec3) {
        self.physbones.hold(physbone, at)
    }

    /// [`PhysBones::release`].
    pub fn release(&mut self, physbone: usize, pose: bool, who: Toucher) {
        self.physbones.release(physbone, pose, who)
    }

    pub fn physbones(&self) -> &PhysBones {
        &self.physbones
    }

    pub fn constraints(&self) -> &Constraints {
        &self.constraints
    }

    pub fn contacts(&self) -> &Contacts {
        &self.contacts
    }

    pub fn output(&self) -> &Output {
        &self.output
    }

    /// Each renderer's node, in the model's renderer order.
    pub fn renderer_nodes(&self) -> &[u32] {
        &self.renderer_nodes
    }

    /// Every node's parent.
    pub fn parents(&self) -> &[Option<usize>] {
        &self.nodes.parent
    }

    /// Whether renderer `r` draws after the last frame: enabled, on a node active with all its ancestors.
    pub fn shown(&self, r: usize) -> bool {
        let mut at = Some(self.renderer_nodes[r] as usize);
        while let Some(n) = at {
            if !self.output.active[n] {
                return false;
            }
            at = self.nodes.parent[n];
        }
        self.output.renderers[r]
    }

    /// This avatar's hand and finger colliders after the last frame, in the world, named by the host's `player` for it:
    /// what the host gives the avatars around it in [`Input::hands`].
    pub fn hands(&self, player: u64) -> Vec<HandCollider> {
        self.physbones.hands(player, &self.root, &self.output.space)
    }

    /// Evaluates a frame, the core's animator posing the avatar.
    pub fn evaluate(&mut self, input: &Input) -> &Output {
        if input.dt > 0.0 {
            self.animator.step(input.dt);
        }
        let values = self.animator.values();
        let mut locals = self.nodes.rest.clone();
        for (n, t) in &self.transforms {
            let r = &mut locals[*n];
            for a in 0..3 {
                if let Some(i) = t.translation[a] {
                    r.translation[a] = values[i];
                }
                if let Some(i) = t.scale[a] {
                    r.scale[a] = values[i];
                }
            }
            if t.rotation.iter().any(Option::is_some) {
                let mut q = r.rotation.to_array();
                for (a, i) in t.rotation.iter().enumerate() {
                    if let Some(i) = i {
                        q[a] = values[*i];
                    }
                }
                r.rotation = unit(Quat::from_array(q));
            }
        }
        if let Some(h) = &self.human {
            let muscles: Vec<f32> = h.muscles.iter().map(|i| i.map_or(0.0, |i| values[i])).collect();
            let b: Vec<f32> = h.body.iter().zip(h.rest).map(|(i, r)| i.map_or(r, |i| values[i])).collect();
            let body = Body { position: Vec3::new(b[0], b[1], b[2]), rotation: unit(Quat::from_xyzw(b[3], b[4], b[5], b[6])) };
            h.human.pose(&self.nodes.parent, &mut locals, &muscles, body);
        }
        let mut active = self.authored_active.clone();
        for (i, n) in &self.node_active {
            active[*n] = values[*i] != 0.0;
        }
        let mut physbones = self.authored_enabled.clone();
        for (i, p) in &self.physbone_enabled {
            physbones[*p] = values[*i] != 0.0;
        }
        for (i, f) in &self.fields {
            set_field(&mut self.constraints, &mut self.contacts, f, values[*i]);
        }
        self.dynamics(input, locals, active, &physbones);
        let values = self.animator.values();
        for (i, r) in &self.renderer_enabled {
            self.output.renderers[*r] = values[*i] != 0.0;
        }
        self.write_bound();
        &self.output
    }

    /// Evaluates a frame from what the host's animation left instead of the core's animator, which does not run:
    /// the renderers and the animator-bound output stay as authored.
    pub fn evaluate_animated(&mut self, input: &Input, animated: &Animated) -> &Output {
        let count = self.nodes.parent.len();
        assert!(animated.locals.len() == count && animated.active.len() == count, "animation leaves {} locals and {} active flags for {count} nodes", animated.locals.len(), animated.active.len());
        assert_eq!(animated.physbones.len(), self.authored_enabled.len(), "a PhysBone flag for each PhysBone");
        for (f, v) in animated.fields {
            set_field(&mut self.constraints, &mut self.contacts, f, *v);
        }
        self.dynamics(input, animated.locals.to_vec(), animated.active.to_vec(), animated.physbones);
        &self.output
    }

    fn dynamics(&mut self, input: &Input, mut locals: Vec<Transform>, active: Vec<bool>, enabled: &[bool]) {
        let parent = &self.nodes.parent;
        let posed = place(parent, &mut locals, input.poses);
        let rigid = Transform { scale: Vec3::ONE, ..input.root };
        let before = locals.clone();
        self.constraints.solve_before_physbones(&mut Scene { root: rigid, locals: &mut locals, active: &active });
        let moved: Vec<bool> = before.iter().zip(&locals).map(|(a, b)| a != b).collect();
        let nodes = space(parent, &locals, &unmoved(parent, &posed, &moved));
        let frame = Frame { root: input.root, nodes, locals: locals.clone(), active: active.clone(), enabled: enabled.to_vec(), hands: input.hands.to_vec() };
        self.root = input.root;
        self.physbones.advance(input.dt, &frame);
        for c in 0..self.physbone_parameters.len() {
            if !self.physbones.running(c) {
                continue;
            }
            for (n, l) in self.physbones.bones(c).zip(self.physbones.local(c)) {
                locals[n] = l;
            }
        }
        place(parent, &mut locals, input.held);
        self.constraints.solve_after_physbones(&mut Scene { root: rigid, locals: &mut locals, active: &active });
        place(parent, &mut locals, input.held);
        let world: Vec<Transform> = worlds(parent, &rigid, &locals);
        let local = self.animator.builtin("IsLocal").is_none_or(|v| v != 0.0);
        self.contacts.advance(input.dt, &world, &active, input.senders, local);
        for (c, names) in self.physbone_parameters.iter().enumerate() {
            let Some(names) = names else { continue };
            let p = self.physbones.parameters(c);
            for (name, v) in names.iter().zip([p.is_grabbed as u8 as f32, p.is_posed as u8 as f32, p.angle, p.stretch, p.squish]) {
                self.animator.set(name, v);
            }
        }
        let mut received: Vec<(&str, f32)> = vec![];
        for (r, v) in self.contacts.receivers().iter().zip(self.contacts.values()) {
            let (false, Some(v)) = (r.parameter.is_empty(), v) else { continue };
            match received.iter_mut().find(|(n, _)| *n == r.parameter) {
                Some((_, w)) => *w = w.max(*v),
                None => received.push((&r.parameter, *v)),
            }
        }
        for (n, v) in received {
            self.animator.set(n, v);
        }
        self.output.space = space(parent, &locals, &[]);
        self.output.locals = locals;
        self.output.active = active;
    }

    fn write_bound(&mut self) {
        let values = self.animator.values();
        self.output.blendshapes = self.blendshapes.iter().map(|(i, n, s)| (*n, s.clone(), values[*i])).collect();
        self.output.materials = self.materials.iter().map(|(i, n, p)| (*n, p.clone(), values[*i])).collect();
        self.output.particles = self.particles.iter().map(|(i, n, p)| (*n, p.clone(), values[*i])).collect();
        self.output.slots = self.slots.iter().filter(|(i, ..)| values[*i] >= 0.0).map(|(i, n, s)| (*n, *s, values[*i] as u32)).collect();
    }
}

fn set_field(constraints: &mut Constraints, contacts: &mut Contacts, f: &Field, v: f32) {
    match f {
        Field::Constraint(c, f) => constraints.set(*c, f, v),
        Field::Sender(c, f) => contacts.set_sender(*c, f, v),
        Field::Receiver(c, f) => contacts.set_receiver(*c, f, v),
    }
}

/// Places each node at its root-space pose by setting its local, whatever the order of `poses`, and returns the
/// poses by node.
fn place(parent: &[Option<usize>], locals: &mut [Transform], poses: &[(usize, Transform)]) -> Vec<Option<Transform>> {
    let mut posed = vec![None; locals.len()];
    for (n, t) in poses {
        posed[*n] = Some(*t);
    }
    for (n, t) in poses {
        locals[*n] = match parent[*n] {
            Some(p) => world(parent, locals, &posed, p).relative(t),
            None => *t,
        };
    }
    posed
}

/// A node's pose in the avatar's root space: a pose the host gave as given, the rest from the locals.
fn world(parent: &[Option<usize>], locals: &[Transform], posed: &[Option<Transform>], node: usize) -> Transform {
    if let Some(Some(t)) = posed.get(node) {
        return *t;
    }
    match parent[node] {
        Some(p) => world(parent, locals, posed, p).mul(&locals[node]),
        None => Transform { translation: Vec3::ZERO, rotation: Quat::IDENTITY, scale: locals[node].scale },
    }
}

/// The host's poses that no constraint moved, nor any node above them.
fn unmoved(parent: &[Option<usize>], posed: &[Option<Transform>], moved: &[bool]) -> Vec<Option<Transform>> {
    let above = |mut n: usize| loop {
        if moved[n] {
            return true;
        }
        match parent[n] {
            Some(p) => n = p,
            None => return false,
        }
    };
    posed.iter().enumerate().map(|(n, t)| t.filter(|_| !above(n))).collect()
}

/// Every node's pose in the avatar's root space.
fn space(parent: &[Option<usize>], locals: &[Transform], posed: &[Option<Transform>]) -> Vec<Transform> {
    let mut out: Vec<Option<Transform>> = vec![None; locals.len()];
    for n in 0..locals.len() {
        fill(parent, locals, posed, n, &mut out);
    }
    out.into_iter().map(Option::unwrap).collect()
}

fn fill(parent: &[Option<usize>], locals: &[Transform], posed: &[Option<Transform>], n: usize, out: &mut [Option<Transform>]) -> Transform {
    if let Some(t) = out[n] {
        return t;
    }
    let t = match (posed.get(n).copied().flatten(), parent[n]) {
        (Some(t), _) => t,
        (None, Some(p)) => fill(parent, locals, posed, p, out).mul(&locals[n]),
        (None, None) => Transform { translation: Vec3::ZERO, rotation: Quat::IDENTITY, scale: locals[n].scale },
    };
    out[n] = Some(t);
    t
}

/// Every node's pose in the world, from the avatar root's.
fn worlds(parent: &[Option<usize>], root: &Transform, locals: &[Transform]) -> Vec<Transform> {
    let mut out: Vec<Option<Transform>> = vec![None; locals.len()];
    fn at(parent: &[Option<usize>], root: &Transform, locals: &[Transform], n: usize, out: &mut [Option<Transform>]) -> Transform {
        if let Some(t) = out[n] {
            return t;
        }
        let t = match parent[n] {
            Some(p) => at(parent, root, locals, p, out).mul(&locals[n]),
            None => Transform { scale: locals[n].scale, ..*root },
        };
        out[n] = Some(t);
        t
    }
    for n in 0..locals.len() {
        at(parent, root, locals, n, &mut out);
    }
    out.into_iter().map(Option::unwrap).collect()
}
