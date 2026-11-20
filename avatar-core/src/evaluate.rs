//! One avatar's frame, evaluated in VRChat's order: the animator, the constraints that read nothing a PhysBone
//! moves, PhysBones, the constraints that do, then contacts. Receiver and PhysBone parameters reach the next frame's
//! animator. A runtime drives an [`Evaluator`] and applies its [`Output`]; it composes none of these itself.

use glam::Quat;

use crate::animator::motion::Target;
use crate::animator::Animator;
use crate::constraints::{Constraints, Scene};
use crate::contacts::{Contacts, Sender};
use crate::model::Axis;
use crate::physbones::{Error, Frame, Nodes, PhysBones, Transform};
use crate::Avatar;

/// What the host gives each frame.
pub struct Input<'a> {
    /// Seconds since the last frame. The animator does not advance on a frame with none.
    pub dt: f32,
    /// The avatar root in the world. Its scale is not read: the root node's local scale is the avatar's.
    pub root: Transform,
    /// Poses the host sets after animation and before dynamics, in the avatar's root space: tracked or IK-solved
    /// bones, parents before children.
    pub poses: &'a [(usize, Transform)],
    /// Poses that replace what PhysBones leave and hold through the constraints solved after them, in the avatar's
    /// root space, parents before children: a pose the host already has from the avatar's own dynamics.
    pub held: &'a [(usize, Transform)],
    /// Contact senders outside the avatar: other avatars' and the hands'.
    pub senders: &'a [Sender],
    /// Whether the avatar is the local player's.
    pub local: bool,
}

/// What animation leaves, given by a host that does not run the core's animator on this avatar.
pub struct Animated<'a> {
    /// Every node's local pose.
    pub locals: &'a [Transform],
    /// Every node's own active flag.
    pub active: &'a [bool],
    /// Every PhysBone's enabled flag.
    pub enabled: &'a [bool],
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
    pub enabled: Vec<bool>,
    /// (renderer node, blendshape, weight from 0 to 100).
    pub blendshapes: Vec<(u32, String, f32)>,
    /// (renderer node, property with its component, value).
    pub materials: Vec<(u32, String, f32)>,
    /// (renderer node, slot, material index).
    pub slots: Vec<(u32, u32, u32)>,
}

#[derive(Default)]
struct TransformTargets {
    translation: [Option<usize>; 3],
    rotation: [Option<usize>; 4],
    scale: [Option<usize>; 3],
}

enum Field {
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
    /// The parameter each PhysBone and receiver drives.
    physbone_parameters: Vec<Option<String>>,
    authored_active: Vec<bool>,
    authored_enabled: Vec<bool>,
    authored_renderers: Vec<bool>,
    renderer_nodes: Vec<u32>,
    enabled: Vec<bool>,
    output: Output,
}

fn axis(a: Axis) -> usize {
    crate::animator::motion::axis_index(a)
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
            physbone_parameters: m.physbones.iter().map(|p| p.parameter.clone()).collect(),
            authored_active: (0..count).map(|n| m.nodes.get(n).is_none_or(|x| x.active)).collect(),
            authored_enabled: m.physbones.iter().map(|p| p.enabled).collect(),
            authored_renderers: m.renderers.iter().map(|r| r.enabled).collect(),
            renderer_nodes: m.renderers.iter().map(|r| r.node.0).collect(),
            enabled: m.physbones.iter().map(|p| p.enabled).collect(),
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
                Target::Translation(_, a) => slot.into_iter().for_each(|t| t.translation[axis(*a)] = Some(i)),
                Target::Rotation(_, a) => slot.into_iter().for_each(|t| t.rotation[axis(*a)] = Some(i)),
                Target::Scale(_, a) => slot.into_iter().for_each(|t| t.scale[axis(*a)] = Some(i)),
                Target::NodeActive(n) if (*n as usize) < count => e.node_active.push((i, *n as usize)),
                Target::RendererEnabled(n) => {
                    if let Some(r) = e.renderer_nodes.iter().position(|x| x == n) {
                        e.renderer_enabled.push((i, r));
                    }
                }
                Target::PhysBone(p, f) if f == "m_Enabled" && (*p as usize) < e.enabled.len() => e.physbone_enabled.push((i, *p as usize)),
                Target::Constraint(c, f) => e.fields.push((i, Field::Constraint(*c as usize, f.clone()))),
                Target::ContactSender(c, f) => e.fields.push((i, Field::Sender(*c as usize, f.clone()))),
                Target::ContactReceiver(c, f) => e.fields.push((i, Field::Receiver(*c as usize, f.clone()))),
                Target::BlendShape(n, s) => e.blendshapes.push((i, *n, s.clone())),
                Target::MaterialProperty(n, p) => e.materials.push((i, *n, p.clone())),
                Target::Slot(n, s) => e.slots.push((i, *n, *s)),
                _ => {}
            }
        }
        e.transforms = transforms.into_iter().enumerate().filter_map(|(n, t)| t.map(|t| (n, t))).collect();
        e.output.active.clone_from(&e.authored_active);
        e.output.enabled.clone_from(&e.authored_renderers);
        e.output.space = space(&e.nodes.parent, &e.output.locals, &[]);
        e.write_bound();
        Ok(e)
    }

    pub fn animator(&self) -> &Animator {
        &self.animator
    }

    /// For the host's parameters: [`Animator::set`], [`Animator::set_builtin`], [`Animator::set_head`],
    /// [`Animator::set_eyes`].
    pub fn animator_mut(&mut self) -> &mut Animator {
        &mut self.animator
    }

    pub fn physbones(&self) -> &PhysBones {
        &self.physbones
    }

    /// For the host's grabs: [`PhysBones::grab`], [`PhysBones::hold`], [`PhysBones::release`].
    pub fn physbones_mut(&mut self) -> &mut PhysBones {
        &mut self.physbones
    }

    pub fn constraints(&self) -> &Constraints {
        &self.constraints
    }

    /// For a host that gives [`Animated`] and so sets animated constraint fields itself.
    pub fn constraints_mut(&mut self) -> &mut Constraints {
        &mut self.constraints
    }

    pub fn contacts(&self) -> &Contacts {
        &self.contacts
    }

    /// For a host that gives [`Animated`] and so sets animated contact fields itself.
    pub fn contacts_mut(&mut self) -> &mut Contacts {
        &mut self.contacts
    }

    pub fn output(&self) -> &Output {
        &self.output
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
                let q = Quat::from_array(q);
                r.rotation = if q.length_squared() > 0.0 { q.normalize() } else { Quat::IDENTITY };
            }
        }
        let mut active = self.authored_active.clone();
        for (i, n) in &self.node_active {
            active[*n] = values[*i] != 0.0;
        }
        self.enabled.clone_from(&self.authored_enabled);
        for (i, p) in &self.physbone_enabled {
            self.enabled[*p] = values[*i] != 0.0;
        }
        for (i, f) in &self.fields {
            let v = values[*i];
            match f {
                Field::Constraint(c, f) => self.constraints.set(*c, f, v),
                Field::Sender(c, f) => self.contacts.set_sender(*c, f, v),
                Field::Receiver(c, f) => self.contacts.set_receiver(*c, f, v),
            }
        }
        let enabled = std::mem::take(&mut self.enabled);
        self.dynamics(input, locals, active, &enabled);
        self.enabled = enabled;
        let values = self.animator.values();
        for (i, r) in &self.renderer_enabled {
            self.output.enabled[*r] = values[*i] != 0.0;
        }
        self.write_bound();
        &self.output
    }

    /// Evaluates a frame from what the host's animation left; the core's animator does not run, and constraint and
    /// contact fields keep what the host set.
    pub fn evaluate_animated(&mut self, input: &Input, animated: &Animated) -> &Output {
        self.dynamics(input, animated.locals.to_vec(), animated.active.to_vec(), animated.enabled);
        &self.output
    }

    fn dynamics(&mut self, input: &Input, mut locals: Vec<Transform>, active: Vec<bool>, enabled: &[bool]) {
        let parent = &self.nodes.parent;
        let mut posed = vec![None; locals.len()];
        for (n, t) in input.poses {
            locals[*n] = match parent[*n] {
                Some(p) => world(parent, &locals, &posed, p).relative(t),
                None => *t,
            };
            posed[*n] = Some(*t);
        }
        let rigid = Transform { scale: glam::Vec3::ONE, ..input.root };
        let before = locals.clone();
        self.constraints.solve_before_physbones(&mut Scene { root: rigid, locals: &mut locals, active: &active });
        let moved: Vec<bool> = before.iter().zip(&locals).map(|(a, b)| a != b).collect();
        let nodes = space(parent, &locals, &unmoved(parent, &posed, &moved));
        let frame = Frame { root: input.root, nodes, locals: locals.clone(), active: active.clone(), enabled: enabled.to_vec() };
        self.physbones.advance(input.dt, &frame);
        for c in 0..self.physbone_parameters.len() {
            if !self.physbones.running(c) {
                continue;
            }
            for (n, l) in self.physbones.bones(c).zip(self.physbones.local(c)) {
                locals[n] = l;
            }
        }
        let held = |locals: &mut Vec<Transform>| {
            for (n, t) in input.held {
                locals[*n] = match parent[*n] {
                    Some(p) => world(parent, locals, &[], p).relative(t),
                    None => *t,
                };
            }
        };
        held(&mut locals);
        self.constraints.solve_after_physbones(&mut Scene { root: rigid, locals: &mut locals, active: &active });
        held(&mut locals);
        let world: Vec<Transform> = worlds(parent, &rigid, &locals);
        self.contacts.advance(input.dt, &world, &active, input.senders, input.local);
        for (c, name) in self.physbone_parameters.iter().enumerate() {
            let Some(name) = name else { continue };
            let p = self.physbones.parameters(c);
            for (suffix, v) in [("IsGrabbed", p.is_grabbed as u8 as f32), ("IsPosed", p.is_posed as u8 as f32), ("Angle", p.angle), ("Stretch", p.stretch), ("Squish", p.squish)] {
                self.animator.set(&format!("{name}_{suffix}"), v);
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
        self.output.slots = self.slots.iter().filter(|(i, ..)| values[*i] >= 0.0).map(|(i, n, s)| (*n, *s, values[*i] as u32)).collect();
    }
}

/// A node's pose in the avatar's root space: a pose the host gave as given, the rest from the locals.
fn world(parent: &[Option<usize>], locals: &[Transform], posed: &[Option<Transform>], node: usize) -> Transform {
    if let Some(Some(t)) = posed.get(node) {
        return *t;
    }
    match parent[node] {
        Some(p) => world(parent, locals, posed, p).mul(&locals[node]),
        None => Transform { translation: glam::Vec3::ZERO, rotation: Quat::IDENTITY, scale: locals[node].scale },
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
        (None, None) => Transform { translation: glam::Vec3::ZERO, rotation: Quat::IDENTITY, scale: locals[n].scale },
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
