use glam::{Quat, Vec3};

use crate::animator::motion::from_euler_degrees;

use crate::model::{self, ConstraintKind, SourceTarget, WorldUp};
use crate::physbones::{Error, Nodes, PhysBones, Transform};
use crate::Avatar;

/// VRChat constraints for one avatar, solved each frame between animation and the avatar's other dynamics: those
/// that read no node a PhysBone moves before PhysBones, the rest after them.
#[derive(Clone, Debug)]
pub struct Constraints {
    model: Vec<model::Constraint>,
    parent: Vec<Option<usize>>,
    order: Vec<usize>,
    after: Vec<bool>,
    /// Each constraint's target pose in the world while frozen.
    frozen: Vec<Option<Transform>>,
    /// Whether each constraint ran in the previous frame: one enabled this frame first runs in the next.
    running: Vec<bool>,
}

/// Each node's local pose and own active flag as animation left them, and the avatar root in the world, of whose
/// root node's local pose only the scale is read; the constraints write their targets' local poses into it.
pub struct Scene<'a> {
    pub root: Transform,
    pub locals: &'a mut [Transform],
    pub active: &'a [bool],
}

impl Scene<'_> {
    fn world(&self, node: usize, parent: &[Option<usize>]) -> Transform {
        let mut chain = vec![node];
        while let Some(p) = parent[*chain.last().unwrap()] {
            chain.push(p);
        }
        let root = chain.pop().unwrap();
        let mut t = Transform { scale: self.locals[root].scale, ..self.root };
        for n in chain.into_iter().rev() {
            t = t.mul(&self.locals[n]);
        }
        t
    }
}

fn relative(parent: &Transform, child: &Transform) -> Transform {
    parent.relative(child)
}

fn euler(e: Vec3) -> Quat {
    from_euler_degrees(e.to_array())
}

/// Past this sine of the X angle VRChat takes the rotation as gimbal-locked: Z is zero and Y alone turns, so a
/// rotation a hair past 90° about X comes back short of it (measured; the threshold lies between 1 - 1.9e-6 and
/// 1 - 2e-6).
const GIMBAL: f32 = 0.999998;

/// A rotation to Euler degrees as VRChat masks its axes (Unity's Z, X, Y order, mirrored on X): unlike the
/// animator's conversion, it gives up the flip at the gimbal lock.
fn to_euler(q: Quat) -> Vec3 {
    let m = glam::Mat3::from_quat(q);
    let (m00, m02, m10, m11, m12, m20, m22) = (m.x_axis.x, m.z_axis.x, m.x_axis.y, m.y_axis.y, m.z_axis.y, m.x_axis.z, m.z_axis.z);
    let x = libm::asinf((-m12).clamp(-1.0, 1.0));
    let (y, z) = if m12.abs() < GIMBAL { (libm::atan2f(m02, m22), libm::atan2f(m10, m11)) } else { (libm::atan2f(-m20, m00), 0.0) };
    Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees())
}

/// Unity's `Quaternion.LookRotation` in the model's mirrored axes.
fn look_rotation(forward: Vec3, up: Vec3) -> Option<Quat> {
    let z = forward.try_normalize()?;
    let x = z.cross(up).try_normalize()?;
    let y = x.cross(z);
    Some(Quat::from_mat3(&glam::Mat3::from_cols(-x, y, z)).normalize())
}

fn mask(value: Vec3, fallback: Vec3, affects: [bool; 3]) -> Vec3 {
    Vec3::new(
        if affects[0] { value.x } else { fallback.x },
        if affects[1] { value.y } else { fallback.y },
        if affects[2] { value.z } else { fallback.z },
    )
}

fn nlerp(from: Quat, to: Quat, s: f32) -> Quat {
    let to = if from.dot(to) < 0.0 { -to } else { to };
    from.lerp(to, s).normalize()
}

fn blend_rotations(items: &[(Quat, f32)]) -> Quat {
    let mut sum = glam::Vec4::ZERO;
    let first = items.first().map_or(Quat::IDENTITY, |i| i.0);
    for (q, w) in items {
        let q = if first.dot(*q) < 0.0 { -*q } else { *q };
        sum += glam::Vec4::from(q) * *w;
    }
    if sum.length_squared() > 0.0 { Quat::from_vec4(sum).normalize() } else { Quat::IDENTITY }
}

impl Constraints {
    pub fn new(avatar: &Avatar) -> Result<Constraints, Error> {
        let nodes = Nodes::read(avatar)?;
        let model = avatar.model.constraints.clone();
        let reads = |c: &model::Constraint| -> Vec<usize> {
            let mut out: Vec<usize> = c.sources.iter().filter_map(|s| if let SourceTarget::Node(n) = s.source { Some(n.0 as usize) } else { None }).collect();
            match &c.kind {
                ConstraintKind::Aim { world_up: WorldUp::ObjectUp { node } | WorldUp::ObjectRotationUp { node, .. }, .. } => out.push(node.0 as usize),
                ConstraintKind::LookAt { world_up_node: Some(node), .. } => out.push(node.0 as usize),
                _ => {}
            }
            out.extend(nodes.parent[c.target.0 as usize]);
            out
        };
        let under = |node: usize, ancestor: usize| {
            let mut at = Some(node);
            while let Some(n) = at {
                if n == ancestor {
                    return true;
                }
                at = nodes.parent[n];
            }
            false
        };
        let count = model.len();
        let mut depth = vec![0usize; count];
        for _ in 0..count {
            let mut changed = false;
            for a in 0..count {
                for b in 0..count {
                    if a != b && model[a].target != model[b].target && reads(&model[a]).iter().any(|&n| under(n, model[b].target.0 as usize)) && depth[a] <= depth[b] {
                        depth[a] = depth[b] + 1;
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        let mut order: Vec<usize> = (0..count).collect();
        order.sort_by_key(|&i| (depth[i], i));
        let physbones = PhysBones::new(avatar)?;
        let mut moved = vec![false; nodes.parent.len()];
        for p in 0..avatar.model.physbones.len() {
            for n in physbones.bones(p) {
                moved[n] = true;
            }
        }
        let mut after = vec![false; count];
        loop {
            let mut changed = false;
            for a in 0..count {
                if after[a] {
                    continue;
                }
                let hit = reads(&model[a]).into_iter().any(|n| {
                    let mut at = Some(n);
                    while let Some(x) = at {
                        if moved[x] || (0..count).any(|b| b != a && after[b] && model[b].target.0 as usize == x) {
                            return true;
                        }
                        at = nodes.parent[x];
                    }
                    false
                });
                if hit {
                    after[a] = true;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        Ok(Constraints { model, parent: nodes.parent, order, after, frozen: vec![None; count], running: vec![true; count] })
    }

    pub fn model(&self) -> &[model::Constraint] {
        &self.model
    }

    /// Sets a field by the name an animation curve binds it with, signed as the model holds it; an unknown field is
    /// ignored, as Unity ignores a curve bound to a property the component lacks.
    pub fn set(&mut self, index: usize, field: &str, value: f32) {
        let Some(c) = self.model.get_mut(index) else { return };
        let (name, axis) = match field.rsplit_once('.') {
            Some((n, a)) if a.len() == 1 && "xyz".contains(a) => (n, Some("xyz".find(a).unwrap())),
            _ => (field, None),
        };
        let on = value != 0.0;
        if let Some(rest) = name.strip_prefix("Sources.source") {
            let Some((k, f)) = rest.split_once('.') else { return };
            let Some(src) = k.parse::<usize>().ok().and_then(|k| c.sources.get_mut(k)) else { return };
            match (f, axis) {
                ("Weight", None) => src.weight = value,
                ("ParentPositionOffset", Some(a)) => src.position_offset[a] = value,
                ("ParentRotationOffset", Some(a)) => src.rotation_offset[a] = value,
                _ => {}
            }
            return;
        }
        match (name, axis) {
            ("m_Enabled", None) => c.enabled = on,
            ("IsActive", None) => c.is_active = on,
            ("GlobalWeight", None) => c.weight = value,
            ("FreezeToWorld", None) => c.freeze_to_world = on,
            ("SolveInLocalSpace", None) => c.solve_in_local_space = on,
            ("RebakeOffsetsWhenUnfrozen", None) => c.rebake_offsets_when_unfrozen = on,
            ("Roll", None) => {
                if let ConstraintKind::LookAt { roll, .. } = &mut c.kind {
                    *roll = value
                }
            }
            (n, Some(a)) => {
                use ConstraintKind::*;
                let (which, part) = ["Offset", "AtRest"].iter().find_map(|p| n.strip_suffix(p).map(|w| (w, *p))).unwrap_or((n, ""));
                let channel = match (&mut c.kind, which) {
                    (Parent { position, .. } | Position { position }, "Position") => Some(position),
                    (Parent { rotation, .. } | Rotation { rotation } | Aim { rotation, .. } | LookAt { rotation, .. }, "Rotation") => Some(rotation),
                    (Scale { scale }, "Scale") => Some(scale),
                    _ => None,
                };
                match (channel, part) {
                    (Some(ch), "Offset") => ch.offset[a] = value,
                    (Some(ch), "AtRest") => ch.at_rest[a] = value,
                    _ => {}
                }
            }
            (n, None) => {
                use ConstraintKind::*;
                let Some(rest) = n.strip_prefix("Affects") else { return };
                let (which, axis) = rest.split_at(rest.len().saturating_sub(1));
                let Some(a) = "XYZ".find(axis) else { return };
                let channel = match (&mut c.kind, which) {
                    (Parent { position, .. } | Position { position }, "Position") => Some(position),
                    (Parent { rotation, .. } | Rotation { rotation } | Aim { rotation, .. }, "Rotation") => Some(rotation),
                    (Scale { scale }, "Scale") => Some(scale),
                    _ => None,
                };
                if let Some(ch) = channel {
                    ch.affects[a] = on;
                }
            }
        }
    }

    /// Whether a constraint runs after PhysBones: it reads a node a PhysBone moves, or one under it, directly or
    /// through another constraint's target.
    pub fn after_physbones(&self, index: usize) -> bool {
        self.after[index]
    }

    /// Solves the constraints that run before PhysBones, in dependency order.
    pub fn solve_before_physbones(&mut self, scene: &mut Scene) {
        self.solve(scene, false)
    }

    /// Solves the constraints that read where PhysBones put a bone, in dependency order, once the PhysBones'
    /// output is in `scene`.
    pub fn solve_after_physbones(&mut self, scene: &mut Scene) {
        self.solve(scene, true)
    }

    fn solve(&mut self, scene: &mut Scene, after: bool) {
        for k in 0..self.order.len() {
            let i = self.order[k];
            if self.after[i] == after {
                self.solve_one(i, scene);
            }
        }
    }

    fn source_world(&self, s: &model::ConstraintSource, scene: &Scene) -> Transform {
        match &s.source {
            SourceTarget::Node(n) => scene.world(n.0 as usize, &self.parent),
            SourceTarget::World(m) => {
                let (scale, rotation, translation) = glam::Mat4::from_cols_array(m).to_scale_rotation_translation();
                Transform { translation, rotation, scale }
            }
        }
    }

    /// Recomputes a constraint's offsets so that, unfrozen, it holds its target where it stands.
    fn rebake(&mut self, i: usize, scene: &Scene, parent: &Transform, held: &Transform) {
        let worlds: Vec<Transform> = self.model[i].sources.iter().map(|s| self.source_world(s, scene)).collect();
        let c = &mut self.model[i];
        let frozen = parent.mul(held);
        let weight = c.weight.clamp(0.0, 1.0);
        match &mut c.kind {
            ConstraintKind::Position { position } if weight > 0.0 && !c.solve_in_local_space => {
                let total: f32 = c.sources.iter().map(|s| s.weight).sum();
                let sum: Vec3 = worlds.iter().zip(&c.sources).map(|(t, s)| t.translation * s.weight).sum::<Vec3>() / total.max(1.0);
                let solved = relative(parent, &Transform { translation: sum, ..Transform::IDENTITY }).translation;
                let rest = Vec3::from(position.at_rest);
                position.offset = ((held.translation - rest) / weight + rest - solved).to_array();
            }
            ConstraintKind::Parent { .. } if !c.solve_in_local_space => {
                for (t, s) in worlds.iter().zip(c.sources.iter_mut()) {
                    let r = t.rotation.inverse();
                    s.position_offset = ((r * (frozen.translation - t.translation)) / t.scale).to_array();
                    s.rotation_offset = to_euler(r * frozen.rotation).to_array();
                }
            }
            _ => {}
        }
    }

    fn solve_one(&mut self, i: usize, scene: &mut Scene) {
        let c = &self.model[i];
        let target = c.target.0 as usize;
        let on = c.enabled && crate::active_in_hierarchy(scene.active, &self.parent, c.node.0 as usize);
        let was = std::mem::replace(&mut self.running[i], on);
        if !on {
            self.frozen[i] = None;
        }
        if !on || !was || !c.is_active {
            return;
        }
        let parent = self.parent[target].map_or(Transform { scale: Vec3::ONE, ..scene.root }, |p| scene.world(p, &self.parent));
        let current = scene.locals[target];
        if c.freeze_to_world {
            let held = relative(&parent, self.frozen[i].get_or_insert(parent.mul(&current)));
            let mut out = current;
            match &c.kind {
                ConstraintKind::Position { position } => out.translation = mask(held.translation, current.translation, position.affects),
                ConstraintKind::Parent { position, rotation } => {
                    out.translation = mask(held.translation, current.translation, position.affects);
                    out.rotation = euler(mask(to_euler(held.rotation), to_euler(current.rotation), rotation.affects));
                }
                ConstraintKind::Rotation { rotation } | ConstraintKind::Aim { rotation, .. } => {
                    out.rotation = euler(mask(to_euler(held.rotation), to_euler(current.rotation), rotation.affects))
                }
                ConstraintKind::LookAt { .. } => out.rotation = held.rotation,
                ConstraintKind::Scale { scale } => out.scale = mask(held.scale, current.scale, scale.affects),
            }
            scene.locals[target] = out;
            return;
        }
        if self.frozen[i].take().is_some() && c.rebake_offsets_when_unfrozen {
            self.rebake(i, scene, &parent, &current);
        }
        let c = &self.model[i];
        let local_space = c.solve_in_local_space;
        let sources: Vec<(Transform, f32, &model::ConstraintSource)> = c
            .sources
            .iter()
            .map(|s| {
                let at = match (&s.source, local_space) {
                    (SourceTarget::Node(n), true) => scene.locals[n.0 as usize],
                    _ => self.source_world(s, scene),
                };
                (at, s.weight, s)
            })
            .collect();
        let total: f32 = sources.iter().map(|s| s.1).sum();
        let to_local = |w: &Transform| if local_space { *w } else { relative(&parent, w) };
        let weight = c.weight.clamp(0.0, 1.0);
        let mut out = current;
        let world_sum = |sources: &[(Transform, f32, &model::ConstraintSource)]| -> Vec3 { sources.iter().map(|(t, w, _)| t.translation * *w).sum::<Vec3>() / total.max(1.0) };
        let point_local = |p: Vec3| to_local(&Transform { translation: p, ..Transform::IDENTITY }).translation;
        let rotation_of = |items: &[(Quat, f32)]| {
            let q = blend_rotations(items);
            if local_space { q } else { parent.rotation.inverse() * q }
        };
        let towards = |rest: Quat, q: Quat| nlerp(rest, q, weight);
        let rotate = |ch: &model::Channel, q: Quat| euler(mask(to_euler(q), to_euler(current.rotation), ch.affects));
        match &c.kind {
            ConstraintKind::Position { position } => {
                let rest = Vec3::from(position.at_rest);
                let p = point_local(world_sum(&sources)) + Vec3::from(position.offset);
                let p = if total > 0.0 { rest.lerp(p, weight) } else { rest };
                out.translation = mask(p, current.translation, position.affects);
            }
            ConstraintKind::Rotation { rotation } => {
                let rest = euler(Vec3::from(rotation.at_rest));
                let items: Vec<(Quat, f32)> = sources.iter().map(|(t, w, _)| (t.rotation, *w)).collect();
                let q = if total > 0.0 { towards(rest, rotation_of(&items) * euler(Vec3::from(rotation.offset))) } else { rest };
                out.rotation = rotate(rotation, q);
            }
            ConstraintKind::Scale { scale } => {
                let rest = Vec3::from(scale.at_rest);
                let s = if total > 0.0 {
                    let log: Vec3 = sources.iter().map(|(t, w, _)| t.scale.abs().max(Vec3::splat(f32::MIN_POSITIVE)).map(libm::logf) * *w).sum();
                    let s = (log / total).map(libm::expf);
                    let s = (if local_space { s } else { s / parent.scale }) * Vec3::from(scale.offset);
                    rest.lerp(s, weight)
                } else {
                    rest
                };
                out.scale = mask(s, current.scale, scale.affects);
            }
            ConstraintKind::Parent { position, rotation } => {
                let rest_p = Vec3::from(position.at_rest);
                let rest_q = euler(Vec3::from(rotation.at_rest));
                let placed: Vec<(Transform, f32, &model::ConstraintSource)> = sources
                    .iter()
                    .map(|(t, w, s)| {
                        let offset = Transform { translation: Vec3::from(s.position_offset), rotation: euler(Vec3::from(s.rotation_offset)), scale: Vec3::ONE };
                        (t.mul(&offset), *w, *s)
                    })
                    .collect();
                let items: Vec<(Quat, f32)> = placed.iter().map(|(t, w, _)| (t.rotation, *w)).collect();
                let (p, q) = if total > 0.0 { (rest_p.lerp(point_local(world_sum(&placed)), weight), towards(rest_q, rotation_of(&items))) } else { (rest_p, rest_q) };
                out.translation = mask(p, current.translation, position.affects);
                out.rotation = rotate(rotation, q);
            }
            ConstraintKind::Aim { rotation, aim_axis, up_axis, world_up } => {
                let rest = euler(Vec3::from(rotation.at_rest));
                let here = if local_space { current.translation } else { parent.point(current.translation) };
                let goal = world_sum(&sources);
                let up = match world_up {
                    WorldUp::SceneUp {} => Some(Vec3::Y),
                    WorldUp::Vector { vector } => Some(Vec3::from(*vector)),
                    WorldUp::ObjectUp { node } => Some(scene.world(node.0 as usize, &self.parent).translation - here),
                    WorldUp::ObjectRotationUp { node, vector } => Some(scene.world(node.0 as usize, &self.parent).rotation * Vec3::from(*vector)),
                    WorldUp::None {} => None,
                };
                let aim = Vec3::from(*aim_axis);
                let upa = Vec3::from(*up_axis);
                let world = match (up.and_then(|u| look_rotation(goal - here, u)), aim.try_normalize(), (goal - here).try_normalize()) {
                    (Some(look), ..) => Some(look * look_rotation(aim, upa).unwrap_or(Quat::IDENTITY).inverse()),
                    (None, Some(from), Some(to)) => Some(Quat::from_rotation_arc(from, to)),
                    _ => None,
                };
                if let Some(world) = world.filter(|_| total > 0.0) {
                    out.rotation = rotate(rotation, towards(rest, rotation_of(&[(world, 1.0)]) * euler(Vec3::from(rotation.offset))));
                } else if total <= 0.0 {
                    out.rotation = rotate(rotation, rest);
                }
            }
            ConstraintKind::LookAt { rotation, roll, world_up_node } => {
                let rest = euler(Vec3::from(rotation.at_rest));
                let here = if local_space { current.translation } else { parent.point(current.translation) };
                let goal = world_sum(&sources);
                let up = world_up_node.map_or(Vec3::Y, |n| scene.world(n.0 as usize, &self.parent).translation - here);
                if total <= 0.0 {
                    out.rotation = rest;
                } else if let Some(look) = look_rotation(goal - here, up) {
                    let world = look * Quat::from_rotation_z(-roll.to_radians());
                    out.rotation = towards(rest, rotation_of(&[(world, 1.0)]) * euler(Vec3::from(rotation.offset)));
                }
            }
        }
        scene.locals[target] = out;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A rotation a hair past 90° about X comes back short of it, as VRChat's masks return it; further past, whole.
    #[test]
    fn a_rotation_at_the_gimbal_lock_loses_its_flip() {
        let back = |degrees: f32| euler(to_euler(Quat::from_rotation_x(degrees.to_radians())));
        assert!(back(90.1).angle_between(Quat::from_rotation_x(89.9_f32.to_radians())) < 1e-3);
        assert!(back(90.3).angle_between(Quat::from_rotation_x(90.3_f32.to_radians())) < 1e-3);
        assert!(back(-90.1).angle_between(Quat::from_rotation_x((-89.9_f32).to_radians())) < 1e-3);
    }
}
