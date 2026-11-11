use std::collections::HashMap;

use glam::{EulerRot, Quat, Vec2};

use crate::model::{
    AnimationCurve, Axis, BlendAlgorithm, Binding, Clip, Motion, NodeIndex, TransformProperty,
};

/// A property the animator writes. Rotations are always quaternion components: Euler curves are
/// converted when sampled.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Target {
    NodeActive(u32),
    RendererEnabled(u32),
    BlendShape(u32, String),
    MaterialProperty(u32, String),
    Translation(u32, Axis),
    Rotation(u32, Axis),
    Scale(u32, Axis),
    Muscle(String),
    AnimatorParameter(String),
    PhysBone(u32, String),
    Constraint(u32, String),
    ContactSender(u32, String),
    ContactReceiver(u32, String),
    /// The material index filling a renderer's slot.
    Slot(u32, u32),
}

impl Target {
    /// Whether the part of the frame no layer covers takes the rest value (transform and muscle values), or
    /// keeps the value it had.
    pub fn rests_at_default(&self) -> bool {
        matches!(self, Target::Translation(..) | Target::Rotation(..) | Target::Scale(..) | Target::Muscle(_))
    }

    /// The value adding nothing in an additive layer.
    pub fn identity(&self) -> f32 {
        match self {
            Target::Scale(..) | Target::Rotation(_, Axis::W) => 1.0,
            _ => 0.0,
        }
    }

    /// Discrete properties take the value of the heaviest contributor instead of a weighted mean.
    pub fn discrete(&self) -> bool {
        matches!(self, Target::Slot(..))
    }
}

#[derive(Default)]
pub struct Targets {
    pub list: Vec<Target>,
    index: HashMap<Target, u32>,
}

impl Targets {
    pub fn id(&mut self, target: Target) -> u32 {
        if let Some(&i) = self.index.get(&target) {
            return i;
        }
        let i = self.list.len() as u32;
        self.index.insert(target.clone(), i);
        self.list.push(target);
        i
    }

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn index_of(&self, target: &Target) -> Option<usize> {
        self.index.get(target).map(|&i| i as usize)
    }
}

struct EulerGroup {
    node: u32,
    axes: [Option<AnimationCurve>; 3],
    targets: [u32; 4],
}

struct Stepped {
    target: u32,
    keys: Vec<(f32, f32)>,
}

pub struct CompiledClip {
    pub length: f32,
    pub looping: bool,
    curves: Vec<(u32, AnimationCurve)>,
    /// Components of an animated transform property no curve drives: Unity writes them at their rest value.
    pub fill: Vec<u32>,
    euler: Vec<EulerGroup>,
    stepped: Vec<Stepped>,
    /// Every target this clip writes, sorted.
    pub writes: Vec<u32>,
    pub rotations: Vec<[u32; 4]>,
    /// The clip played mirrored, when it has humanoid curves: Unity mirrors only those.
    pub mirrored: Option<Box<CompiledClip>>,
    /// Muscles a humanoid clip has no curve for: Unity evaluates its whole human pose, these at rest.
    rest_muscles: Vec<u32>,
}

/// The rest value of a node's local rotation as Unity's Euler angles (ZXY order), in degrees, in glTF axes.
pub fn euler_degrees(q: Quat) -> [f32; 3] {
    let (y, x, z) = q.to_euler(EulerRot::YXZ);
    [x.to_degrees(), y.to_degrees(), z.to_degrees()]
}

fn from_euler_degrees(e: [f32; 3]) -> Quat {
    Quat::from_euler(EulerRot::YXZ, e[1].to_radians(), e[0].to_radians(), e[2].to_radians())
}

pub fn axis_index(a: Axis) -> usize {
    match a {
        Axis::X => 0,
        Axis::Y => 1,
        Axis::Z => 2,
        Axis::W => 3,
    }
}

const AXES: [Axis; 4] = [Axis::X, Axis::Y, Axis::Z, Axis::W];

pub fn target_of(binding: &Binding) -> Option<Target> {
    Some(match binding {
        Binding::NodeActive { node } => Target::NodeActive(node.0),
        Binding::RendererEnabled { node } => Target::RendererEnabled(node.0),
        Binding::BlendShape { node, name } => Target::BlendShape(node.0, name.clone()),
        Binding::MaterialProperty { node, property } => Target::MaterialProperty(node.0, property.clone()),
        Binding::Transform { node, property, component } => match property {
            TransformProperty::Translation => Target::Translation(node.0, *component),
            TransformProperty::Rotation => Target::Rotation(node.0, *component),
            TransformProperty::Scale => Target::Scale(node.0, *component),
            TransformProperty::EulerRotation => return None,
        },
        Binding::Muscle { name } => Target::Muscle(name.clone()),
        Binding::AnimatorParameter { name } => Target::AnimatorParameter(name.clone()),
        Binding::PhysBone { index, field } => Target::PhysBone(index.0, field.clone()),
        Binding::Constraint { index, field } => Target::Constraint(index.0, field.clone()),
        Binding::ContactSender { index, field } => Target::ContactSender(index.0, field.clone()),
        Binding::ContactReceiver { index, field } => Target::ContactReceiver(index.0, field.clone()),
    })
}

impl CompiledClip {
    pub fn compile(clip: &Clip, targets: &mut Targets) -> CompiledClip {
        let mut c = Self::compile_as_is(clip, targets);
        c.mirrored = mirror_clip(clip).map(|m| Box::new(Self::compile_as_is(&m, targets)));
        c
    }

    fn compile_as_is(clip: &Clip, targets: &mut Targets) -> CompiledClip {
        let mut curves = Vec::new();
        let mut euler: Vec<EulerGroup> = Vec::new();
        for c in &clip.curves {
            if let Binding::Transform { node, property: TransformProperty::EulerRotation, component } = &c.binding {
                let NodeIndex(n) = *node;
                let at = match euler.iter().position(|g| g.node == n) {
                    Some(i) => i,
                    None => {
                        let ids = AXES.map(|a| targets.id(Target::Rotation(n, a)));
                        euler.push(EulerGroup { node: n, axes: [None, None, None], targets: ids });
                        euler.len() - 1
                    }
                };
                let i = axis_index(*component);
                if i < 3 {
                    euler[at].axes[i] = Some(c.curve.clone());
                }
                continue;
            }
            if let Some(t) = target_of(&c.binding) {
                curves.push((targets.id(t), c.curve.clone()));
            }
        }
        let stepped = clip
            .slot_curves
            .iter()
            .map(|s| Stepped {
                target: targets.id(Target::Slot(s.renderer.0, s.slot)),
                keys: s.keys.iter().map(|k| (k.time, k.material.0 as f32)).collect(),
            })
            .collect::<Vec<_>>();
        let mut fill = Vec::new();
        let mut groups: Vec<(u32, u8)> = Vec::new();
        for c in &clip.curves {
            if let Binding::Transform { node, property, .. } = &c.binding {
                let k = match property {
                    TransformProperty::Translation => 0,
                    TransformProperty::Scale => 1,
                    TransformProperty::Rotation => 2,
                    TransformProperty::EulerRotation => continue,
                };
                if !groups.contains(&(node.0, k)) {
                    groups.push((node.0, k));
                }
            }
        }
        for (n, k) in groups {
            let axes: &[Axis] = if k == 2 { &AXES } else { &AXES[..3] };
            for &a in axes {
                let t = targets.id(match k {
                    0 => Target::Translation(n, a),
                    1 => Target::Scale(n, a),
                    _ => Target::Rotation(n, a),
                });
                if !curves.iter().any(|c| c.0 == t) {
                    fill.push(t);
                }
            }
        }
        let mut writes: Vec<u32> = curves.iter().map(|c| c.0).collect();
        writes.extend(fill.iter().copied());
        writes.extend(euler.iter().flat_map(|g| g.targets));
        writes.extend(stepped.iter().map(|s| s.target));
        writes.sort_unstable();
        writes.dedup();
        let mut rotations: Vec<[u32; 4]> = euler.iter().map(|g| g.targets).collect();
        for c in &clip.curves {
            if let Binding::Transform { node, property: TransformProperty::Rotation, .. } = &c.binding {
                let ids = AXES.map(|a| targets.id(Target::Rotation(node.0, a)));
                if !rotations.contains(&ids) {
                    rotations.push(ids);
                }
            }
        }
        CompiledClip { length: clip.length, looping: clip.looping, curves, fill, euler, stepped, writes, rotations, mirrored: None, rest_muscles: Vec::new() }
    }

    /// Gives a humanoid clip, and its mirror, every muscle of `muscles` it has no curve for at rest.
    pub fn complete_human_pose(&mut self, muscles: &[u32], targets: &Targets) {
        let human = |c: &CompiledClip| c.curves.iter().any(|(t, _)| matches!(targets.list[*t as usize], Target::Muscle(_)));
        if !human(self) {
            return;
        }
        self.rest_unbound_muscles(muscles);
        if let Some(m) = self.mirrored.as_deref_mut() {
            m.rest_unbound_muscles(muscles);
        }
    }

    fn rest_unbound_muscles(&mut self, muscles: &[u32]) {
        self.rest_muscles = muscles.iter().copied().filter(|t| !self.curves.iter().any(|x| x.0 == *t)).collect();
        self.writes.extend(self.rest_muscles.iter().copied());
        self.writes.sort_unstable();
        self.writes.dedup();
    }

    pub fn variant(&self, mirror: bool) -> &CompiledClip {
        match (&self.mirrored, mirror) {
            (Some(m), true) => m,
            _ => self,
        }
    }

    pub fn bindings(&self) -> impl Iterator<Item = u32> + '_ {
        self.writes.iter().chain(self.mirrored.iter().flat_map(|m| m.writes.iter())).copied()
    }

    /// The clip time for a normalized state time.
    pub fn time(&self, normalized: f32) -> f32 {
        if self.looping {
            normalized.rem_euclid(1.0) * self.length
        } else {
            normalized.clamp(0.0, 1.0) * self.length
        }
    }

    pub fn sample(&self, normalized: f32, defaults: &[f32], out: &mut impl FnMut(u32, f32)) {
        let t = self.time(normalized);
        for (target, curve) in &self.curves {
            out(*target, curve.evaluate(t));
        }
        for &t in self.fill.iter().chain(&self.rest_muscles) {
            out(t, defaults[t as usize]);
        }
        for g in &self.euler {
            let rest = Quat::from_xyzw(
                defaults[g.targets[0] as usize],
                defaults[g.targets[1] as usize],
                defaults[g.targets[2] as usize],
                defaults[g.targets[3] as usize],
            );
            let mut e = euler_degrees(rest);
            for (i, c) in g.axes.iter().enumerate() {
                if let Some(c) = c {
                    e[i] = c.evaluate(t);
                }
            }
            let q = from_euler_degrees(e);
            for (i, v) in [q.x, q.y, q.z, q.w].into_iter().enumerate() {
                out(g.targets[i], v);
            }
        }
        for s in self.stepped.iter().filter(|s| !s.keys.is_empty()) {
            let i = s.keys.partition_point(|k| k.0 <= t).max(1);
            out(s.target, s.keys[i - 1].1);
        }
    }
}

/// A clip with its humanoid curves mirrored, or None when it has none.
fn mirror_clip(clip: &Clip) -> Option<Clip> {
    if !clip.curves.iter().any(|c| matches!(c.binding, Binding::Muscle { .. })) {
        return None;
    }
    let mut m = clip.clone();
    for c in &mut m.curves {
        let Binding::Muscle { name } = &c.binding else { continue };
        let (other, negate) = super::human::mirrored(name);
        c.binding = Binding::Muscle { name: other };
        if negate {
            for k in &mut c.curve.keys {
                k.value = -k.value;
                k.in_tangent = -k.in_tangent;
                k.out_tangent = -k.out_tangent;
            }
        }
    }
    Some(m)
}

/// A leaf of a motion's blend: a clip, its weight, and the time scale and cycle offset its tree path applies.
#[derive(Clone, Copy, Debug)]
pub struct Leaf {
    pub clip: u32,
    pub weight: f32,
    pub time_scale: f32,
    pub cycle_offset: f32,
    /// Under a direct blend tree, whose weights need not sum to one: a target only some leaves write gets no
    /// share of its default from the others.
    pub direct: bool,
    /// Which leaf of the motion this is, counting every leaf whether or not it plays this frame.
    pub id: u32,
    /// Whether the state and the tree path above it mirror the clip, each mirror undoing the one above.
    pub mirror: bool,
}

/// Blend tree weights for each algorithm, as Unity computes them. A 1D tree evaluates both ends of the segment
/// its parameter lies in, an end at weight zero included, so what that end binds and the other leaves out takes
/// its default; children it does not evaluate are None.
pub fn weights_1d(thresholds: &[f32], x: f32) -> Vec<Option<f32>> {
    let n = thresholds.len();
    let mut w = vec![None; n];
    if n == 0 {
        return w;
    }
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| thresholds[a].total_cmp(&thresholds[b]));
    if x < thresholds[order[0]] || n == 1 {
        w[order[0]] = Some(1.0);
        return w;
    }
    if x > thresholds[order[n - 1]] {
        w[order[n - 1]] = Some(1.0);
        return w;
    }
    for k in 0..n - 1 {
        let (a, b) = (order[k], order[k + 1]);
        let (ta, tb) = (thresholds[a], thresholds[b]);
        if x >= ta && x <= tb {
            let s = if tb > ta { (x - ta) / (tb - ta) } else { 0.0 };
            w[a] = Some(1.0 - s);
            w[b] = Some(s);
            return w;
        }
    }
    w
}

pub fn weights_cartesian(points: &[Vec2], p: Vec2) -> Vec<f32> {
    let n = points.len();
    let mut w = vec![0.0f32; n];
    for i in 0..n {
        let mut m = 1.0f32;
        for j in 0..n {
            if i == j {
                continue;
            }
            let ij = points[j] - points[i];
            let d = ij.length_squared();
            if d == 0.0 {
                continue;
            }
            let ip = p - points[i];
            m = m.min(1.0 - ip.dot(ij) / d);
        }
        w[i] = m.max(0.0);
    }
    normalize(&mut w);
    w
}

fn signed_angle(a: Vec2, b: Vec2) -> f32 {
    (a.x * b.y - a.y * b.x).atan2(a.dot(b))
}

/// Gradient band interpolation in polar space, as Unity's freeform directional blend computes it: a pair with a
/// motion at the centre measures the angle between the other motion and the parameter instead.
pub fn weights_directional(points: &[Vec2], p: Vec2) -> Vec<f32> {
    let unsigned = |a: Vec2, b: Vec2| if a == Vec2::ZERO || b == Vec2::ZERO { 0.0 } else { signed_angle(a, b).abs() };
    let n = points.len();
    let mut w = vec![0.0f32; n];
    let pm = p.length();
    for i in 0..n {
        let mut m = 1.0f32;
        let pi = points[i];
        let mi = pi.length();
        for j in 0..n {
            if i == j {
                continue;
            }
            let pj = points[j];
            let mj = pj.length();
            let mean = (mi + mj) * 0.5;
            if mean == 0.0 {
                continue;
            }
            let (vij, vip) = if mi == 0.0 {
                (Vec2::new(mj / mean, unsigned(pj, p)), Vec2::new(pm / mean, 0.0))
            } else if mj == 0.0 {
                let a = unsigned(pi, p);
                (Vec2::new(-mi / mean, a), Vec2::new((pm - mi) / mean, a))
            } else {
                let aip = if pm == 0.0 { 0.0 } else { signed_angle(pi, p) };
                (Vec2::new((mj - mi) / mean, signed_angle(pi, pj)), Vec2::new((pm - mi) / mean, aip))
            };
            let d = vij.length_squared();
            if d == 0.0 {
                continue;
            }
            m = m.min(1.0 - vip.dot(vij) / d);
        }
        w[i] = m.max(0.0);
    }
    normalize(&mut w);
    w
}

pub fn weights_simple_directional(points: &[Vec2], p: Vec2) -> Vec<f32> {
    let n = points.len();
    let mut w = vec![0.0f32; n];
    if n == 0 {
        return w;
    }
    let center = points.iter().position(|q| q.length_squared() < 1e-12);
    if p.length_squared() < 1e-12 {
        match center {
            Some(c) => w[c] = 1.0,
            None => w.iter_mut().for_each(|x| *x = 1.0 / n as f32),
        }
        return w;
    }
    let dirs: Vec<usize> = (0..n).filter(|&i| Some(i) != center).collect();
    if dirs.is_empty() {
        if let Some(c) = center {
            w[c] = 1.0;
        }
        return w;
    }
    let angle = |q: Vec2| q.y.atan2(q.x);
    let pa = angle(p);
    let mut sorted = dirs.clone();
    sorted.sort_by(|&a, &b| angle(points[a]).total_cmp(&angle(points[b])));
    let k = sorted.len();
    let mut pair = (sorted[k - 1], sorted[0]);
    for s in 0..k {
        let (a, b) = (sorted[s], sorted[(s + 1) % k]);
        let (aa, mut ab) = (angle(points[a]), angle(points[b]));
        if ab <= aa {
            ab += std::f32::consts::TAU;
        }
        let mut x = pa;
        if x < aa {
            x += std::f32::consts::TAU;
        }
        if x >= aa && x <= ab {
            pair = (a, b);
            break;
        }
    }
    let (a, b) = pair;
    let (pa_, pb_) = (points[a], points[b]);
    let det = pa_.x * pb_.y - pa_.y * pb_.x;
    if a == b || det.abs() < 1e-9 {
        let s = (p.dot(pa_) / pa_.length_squared()).clamp(0.0, 1.0);
        w[a] = s;
        if let Some(c) = center {
            w[c] += 1.0 - s;
        } else {
            w[a] = 1.0;
        }
        return w;
    }
    let mut u = (p.x * pb_.y - p.y * pb_.x) / det;
    let mut v = (pa_.x * p.y - pa_.y * p.x) / det;
    u = u.max(0.0);
    v = v.max(0.0);
    let sum = u + v;
    if sum <= 0.0 && center.is_none() {
        w[a] = 0.5;
        w[b] = 0.5;
    } else if center.is_none() || sum > 1.0 {
        w[a] = u / sum;
        w[b] = v / sum;
    } else {
        w[a] = u;
        w[b] = v;
        w[center.unwrap()] = 1.0 - sum;
    }
    w
}

fn normalize(w: &mut [f32]) {
    let s: f32 = w.iter().sum();
    if s > 0.0 {
        w.iter_mut().for_each(|x| *x /= s);
    }
}

/// The clips a motion plays and their weights, given a parameter lookup.
pub fn leaves(motion: &Motion, param: &dyn Fn(&str) -> f32, scale: Leaf, out: &mut Vec<Leaf>) {
    let mut next = 0;
    walk(motion, param, scale, &mut next, out);
}

fn leaf_count(motion: &Motion) -> u32 {
    let mut ids = Vec::new();
    super::machine::clip_ids(motion, &mut ids);
    ids.len() as u32
}

fn walk(motion: &Motion, param: &dyn Fn(&str) -> f32, scale: Leaf, next: &mut u32, out: &mut Vec<Leaf>) {
    match motion {
        Motion::Clip(c) => {
            out.push(Leaf { clip: c.0, id: *next, ..scale });
            *next += 1;
        }
        Motion::BlendTree(tree) => {
            let direct = scale.direct || matches!(tree.algorithm, BlendAlgorithm::Direct { .. });
            let mut push = |m: &Option<Motion>, w: Option<f32>, ts: f32, co: f32, mirror: bool, out: &mut Vec<Leaf>| {
                let Some(m) = m else { return };
                let Some(w) = w else {
                    *next += leaf_count(m);
                    return;
                };
                let child = Leaf {
                    clip: 0,
                    weight: scale.weight * w,
                    time_scale: scale.time_scale * ts,
                    cycle_offset: scale.cycle_offset + co,
                    direct,
                    id: 0,
                    mirror: scale.mirror ^ mirror,
                };
                walk(m, param, child, next, out);
            };
            match &tree.algorithm {
                BlendAlgorithm::Simple1D { blend_parameter, children } => {
                    let th: Vec<f32> = children.iter().map(|c| c.threshold).collect();
                    let w = weights_1d(&th, param(blend_parameter));
                    for (c, w) in children.iter().zip(w) {
                        push(&c.motion, w, c.time_scale, c.cycle_offset, c.mirror, out);
                    }
                }
                BlendAlgorithm::SimpleDirectional2D { blend_parameter, blend_parameter_y, children }
                | BlendAlgorithm::FreeformDirectional2D { blend_parameter, blend_parameter_y, children }
                | BlendAlgorithm::FreeformCartesian2D { blend_parameter, blend_parameter_y, children } => {
                    let pts: Vec<Vec2> = children.iter().map(|c| Vec2::new(c.position[0], c.position[1])).collect();
                    let p = Vec2::new(param(blend_parameter), param(blend_parameter_y));
                    let w = match &tree.algorithm {
                        BlendAlgorithm::SimpleDirectional2D { .. } => weights_simple_directional(&pts, p),
                        BlendAlgorithm::FreeformDirectional2D { .. } => weights_directional(&pts, p),
                        _ => weights_cartesian(&pts, p),
                    };
                    for (c, w) in children.iter().zip(w) {
                        push(&c.motion, (w != 0.0).then_some(w), c.time_scale, c.cycle_offset, c.mirror, out);
                    }
                }
                BlendAlgorithm::Direct { normalized_blend_values, children } => {
                    let mut w: Vec<f32> = children.iter().map(|c| param(&c.direct_blend_parameter).max(0.0)).collect();
                    if *normalized_blend_values {
                        normalize(&mut w);
                    }
                    for (c, w) in children.iter().zip(w) {
                        push(&c.motion, (w != 0.0).then_some(w), c.time_scale, c.cycle_offset, c.mirror, out);
                    }
                }
            }
        }
    }
}
