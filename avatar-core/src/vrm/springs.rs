//! VRM spring bones as PhysBones.
//!
//! A VRM spring joint moves its tail each step by its velocity `v` times `1 - dragForce`, by `stiffness · dt`
//! metres along `r`, its rest axis turned by its parent's current rotation, and by `gravityPower · dt` metres along
//! `gravityDir`, then puts it back at its bone's length `L`. A version 1.0 PhysBone with advanced integration and
//! no stiffness moves a bone end `p`, taken from its parent's position, to `p + spring · v + pull · (L·r - p)` and
//! then `gravity · L / 2` down before the same projection, which is `(1 - pull)` times
//! `p + spring / (1 - pull) · v + pull / (1 - pull) · L·r`. With `a = stiffness · dt / L` the two steps are equal
//! before collision for `pull = a / (1 + a)`, `spring = (1 - dragForce)(1 - pull)` and
//! `gravity = 2 · gravityPower · dt · (1 - pull) / L`, `dt` being the PhysBone step and `gravityPower` scaled by how
//! far `gravityDir` points down. A chain's lengths vary, so these ride on curves over its depth; a VRM 0.x tree
//! takes the mean length at each depth, and gravity past the PhysBone range of ±1 is clamped, so there the match is
//! approximate.

use glam::{Quat, Vec3};
use serde_json::Value;

use super::{floats, malformed, number, Doc, Error, Version};
use crate::model::{
    AnimationCurve, Collider, ColliderIndex, ColliderShape, Curved, ImmobileType, IntegrationType, Keyframe, Limit,
    MultiChildType, NodeIndex, Permission, PhysBone, PhysBoneVersion, WeightedMode,
};
use crate::physbones::STEP;

/// VRM 0.x gives a leaf joint a tail this far along its own offset from its parent.
const LEAF_TAIL: f32 = 0.07;

/// A joint's VRM settings, for the bone from it to its tail.
#[derive(Clone, Copy)]
struct Joint {
    stiffness: f32,
    drag: f32,
    gravity: f32,
    radius: f32,
}

fn curved(values: &[f32]) -> Curved {
    let peak = values.iter().copied().fold(0.0f32, |a, v| a.max(v.abs()));
    if values.windows(2).all(|w| (w[0] - w[1]).abs() <= 1e-6 * peak.max(1.0)) || peak == 0.0 {
        return Curved { value: values.first().copied().unwrap_or(0.0), curve: None };
    }
    let last = (values.len() - 1).max(1) as f32;
    let mut keys = values
        .iter()
        .enumerate()
        .map(|(i, v)| Keyframe { time: i as f32 / last, value: v / peak, in_tangent: 0.0, out_tangent: 0.0, in_weight: 1.0 / 3.0, out_weight: 1.0 / 3.0, weighted_mode: WeightedMode::None })
        .collect::<Vec<_>>();
    for i in 0..keys.len() {
        let slope = |a: &Keyframe, b: &Keyframe| (b.value - a.value) / (b.time - a.time);
        if i > 0 {
            keys[i].in_tangent = slope(&keys[i - 1], &keys[i]);
        }
        if i + 1 < keys.len() {
            keys[i].out_tangent = slope(&keys[i], &keys[i + 1]);
        }
    }
    Curved { value: peak, curve: Some(AnimationCurve { keys }) }
}

fn constant(value: f32) -> Curved {
    Curved { value, curve: None }
}

/// The PhysBone for one VRM chain or tree, whose bones at each depth below its root take `per_depth`'s settings
/// and lengths.
fn physbone(root: NodeIndex, ignore: Vec<NodeIndex>, endpoint: Vec3, per_depth: &[(Joint, f32)], colliders: Vec<ColliderIndex>) -> PhysBone {
    let by = |f: &dyn Fn(&(Joint, f32)) -> f32| curved(&per_depth.iter().map(f).collect::<Vec<_>>());
    let per_length = |x: f32, length: f32| if length > 1e-6 { x.max(0.0) * STEP / length } else { 0.0 };
    let pull = |s: &Joint, l: f32| {
        let a = per_length(s.stiffness, l);
        a / (1.0 + a)
    };
    let no = Permission { allow_self: false, allow_others: false };
    PhysBone {
        node: root,
        enabled: true,
        version: PhysBoneVersion::V1_0,
        root,
        ignore_transforms: ignore,
        ignore_other_phys_bones: false,
        endpoint_position: endpoint.to_array(),
        multi_child_type: MultiChildType::First,
        integration_type: IntegrationType::Advanced,
        pull: by(&|(s, l)| pull(s, *l)),
        spring: by(&|(s, l)| (1.0 - s.drag).clamp(0.0, 1.0) * (1.0 - pull(s, *l))),
        stiffness: constant(0.0),
        gravity: by(&|(s, l)| 2.0 * per_length(s.gravity, *l) * (1.0 - pull(s, *l))),
        gravity_falloff: constant(0.0),
        immobile_type: ImmobileType::AllMotion,
        immobile: constant(0.0),
        limit: Limit::None {},
        radius: by(&|(s, _)| s.radius),
        max_stretch: constant(0.0),
        stretch_motion: constant(0.0),
        max_squish: constant(0.0),
        colliders,
        collision: Permission { allow_self: true, allow_others: false },
        grabbing: no,
        posing: no,
        grab_movement: 0.5,
        snap_to_hand: false,
        parameter: None,
        is_animated: false,
        reset_when_disabled: false,
    }
}

impl Doc<'_> {
    fn length(&self, from: usize, to: usize) -> f32 {
        (self.world[to].translation - self.world[from].translation).length()
    }

    /// A joint's downward gravity; PhysBones fall only along -Y.
    fn gravity(&mut self, power: f32, direction: Vec3) -> f32 {
        let direction = direction.normalize_or(Vec3::NEG_Y);
        if power != 0.0 && direction.dot(Vec3::NEG_Y) < 1.0 - 1e-4 {
            self.report.add("vrm spring gravity not straight down");
        }
        power * direction.dot(Vec3::NEG_Y).max(0.0)
    }
}

pub(super) fn springs(doc: &mut Doc, exts: &Value) -> Result<(Vec<PhysBone>, Vec<Collider>), Error> {
    match doc.version {
        Version::V1 => v1(doc, exts.get("VRMC_springBone")),
        Version::V0 => v0(doc),
    }
}

fn v1(doc: &mut Doc, ext: Option<&Value>) -> Result<(Vec<PhysBone>, Vec<Collider>), Error> {
    let Some(ext) = ext else { return Ok((vec![], vec![])) };
    let mut colliders = vec![];
    for (i, c) in ext.get("colliders").and_then(Value::as_array).into_iter().flatten().enumerate() {
        let node = doc.node(c.get("node"), &format!("spring collider {i}"))?;
        let extended = c.pointer("/extensions/VRMC_springBone_extended_collider/shape");
        let shape = extended.or(c.get("shape")).ok_or_else(|| malformed(format!("spring collider {i} has no shape")))?;
        colliders.push(collider(node, shape)?);
    }
    let groups: Vec<Vec<ColliderIndex>> = ext
        .get("colliderGroups")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|g| {
            g.get("colliders")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|c| c.as_u64().filter(|&c| (c as usize) < colliders.len()).map(|c| ColliderIndex(c as u32)).ok_or_else(|| malformed("a spring collider group names a missing collider")))
                .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Result<_, _>>()?;
    let mut physbones = vec![];
    for (s, spring) in ext.get("springs").and_then(Value::as_array).into_iter().flatten().enumerate() {
        let mut joints = vec![];
        for j in spring.get("joints").and_then(Value::as_array).into_iter().flatten() {
            let node = doc.node(j.get("node"), &format!("spring {s} joint"))?.0 as usize;
            let settings = Joint {
                stiffness: number(j.get("stiffness"), 1.0),
                drag: number(j.get("dragForce"), 0.5),
                gravity: {
                    let dir = floats::<3>(j.get("gravityDir")).map_or(Vec3::NEG_Y, Vec3::from);
                    doc.gravity(number(j.get("gravityPower"), 0.0), dir)
                },
                radius: number(j.get("hitRadius"), 0.0),
            };
            joints.push((node, settings));
        }
        if joints.len() < 2 {
            doc.report.add("vrm spring with fewer than two joints");
            continue;
        }
        if joints.windows(2).any(|w| doc.nodes.parent[w[1].0] != Some(w[0].0)) {
            doc.report.add("vrm spring whose joints are not a chain");
            continue;
        }
        if spring.get("center").is_some() {
            doc.report.add("vrm spring center");
        }
        let in_chain = |n: usize| joints.iter().any(|j| j.0 == n);
        let mut ignore = vec![];
        for (n, _) in &joints {
            for &c in &doc.nodes.children[*n] {
                if !in_chain(c) {
                    ignore.push(NodeIndex(c as u32));
                }
            }
        }
        let per_depth: Vec<(Joint, f32)> = joints.windows(2).map(|w| (w[0].1, doc.length(w[0].0, w[1].0))).collect();
        let mut used = vec![];
        for g in spring.get("colliderGroups").and_then(Value::as_array).into_iter().flatten() {
            let group = g.as_u64().and_then(|g| groups.get(g as usize)).ok_or_else(|| malformed(format!("spring {s} names a missing collider group")))?;
            for c in group {
                if !used.contains(c) {
                    used.push(*c);
                }
            }
        }
        physbones.push(physbone(NodeIndex(joints[0].0 as u32), ignore, Vec3::ZERO, &per_depth, used));
    }
    Ok((physbones, colliders))
}

/// A VRM 1.0 collider shape, or a VRM 0.x sphere, whose offsets the caller has put in glTF axes.
fn collider(node: NodeIndex, shape: &Value) -> Result<Collider, Error> {
    let at = |s: &Value, k: &str| floats::<3>(s.get(k)).map_or(Vec3::ZERO, Vec3::from);
    let inside = |s: &Value| s.get("inside").and_then(Value::as_bool) == Some(true);
    if let Some(s) = shape.get("sphere") {
        return Ok(Collider {
            node,
            shape: ColliderShape::Sphere { radius: number(s.get("radius"), 0.0), inside_bounds: inside(s) },
            position: at(s, "offset").to_array(),
            rotation: Quat::IDENTITY.to_array(),
            bones_as_spheres: false,
        });
    }
    if let Some(s) = shape.get("capsule") {
        let (a, b, radius) = (at(s, "offset"), at(s, "tail"), number(s.get("radius"), 0.0));
        let axis = b - a;
        return Ok(Collider {
            node,
            shape: ColliderShape::Capsule { radius, height: axis.length() + 2.0 * radius, inside_bounds: inside(s) },
            position: ((a + b) / 2.0).to_array(),
            rotation: Quat::from_rotation_arc(Vec3::Y, axis.normalize_or(Vec3::Y)).to_array(),
            bones_as_spheres: false,
        });
    }
    if let Some(s) = shape.get("plane") {
        let normal = at(s, "normal").normalize_or(Vec3::Y);
        return Ok(Collider {
            node,
            shape: ColliderShape::Plane {},
            position: at(s, "offset").to_array(),
            rotation: Quat::from_rotation_arc(Vec3::Y, normal).to_array(),
            bones_as_spheres: false,
        });
    }
    Err(malformed(format!("spring collider on node {} has no sphere, capsule or plane", node.0)))
}

fn v0(doc: &mut Doc) -> Result<(Vec<PhysBone>, Vec<Collider>), Error> {
    let Some(secondary) = doc.vrm.get("secondaryAnimation").cloned() else { return Ok((vec![], vec![])) };
    let mut colliders = vec![];
    let mut groups = vec![];
    for (i, g) in secondary.get("colliderGroups").and_then(Value::as_array).into_iter().flatten().enumerate() {
        let node = doc.node(g.get("node"), &format!("collider group {i}"))?;
        let mut members = vec![];
        for c in g.get("colliders").and_then(Value::as_array).into_iter().flatten() {
            let o = super::xyz(c.get("offset")).unwrap_or_default();
            let sphere = serde_json::json!({ "sphere": { "offset": [o.x, o.y, -o.z], "radius": number(c.get("radius"), 0.0) } });
            members.push(ColliderIndex(colliders.len() as u32));
            colliders.push(collider(node, &sphere)?);
        }
        groups.push(members);
    }
    let mut physbones = vec![];
    let mut roots = vec![];
    for (i, g) in secondary.get("boneGroups").and_then(Value::as_array).into_iter().flatten().enumerate() {
        let dir = super::xyz(g.get("gravityDir")).unwrap_or(Vec3::NEG_Y);
        let settings = Joint {
            stiffness: number(g.get("stiffiness"), 1.0),
            drag: number(g.get("dragForce"), 0.4),
            gravity: doc.gravity(number(g.get("gravityPower"), 0.0), dir),
            radius: number(g.get("hitRadius"), 0.0),
        };
        if g.get("center").and_then(Value::as_i64).is_some_and(|c| c >= 0) {
            doc.report.add("vrm spring center");
        }
        let mut used = vec![];
        for k in g.get("colliderGroups").and_then(Value::as_array).into_iter().flatten() {
            let group = k.as_u64().and_then(|k| groups.get(k as usize)).ok_or_else(|| malformed(format!("bone group {i} names a missing collider group")))?;
            for c in group {
                if !used.contains(c) {
                    used.push(*c);
                }
            }
        }
        for b in g.get("bones").and_then(Value::as_array).into_iter().flatten() {
            let root = doc.node(Some(b), &format!("bone group {i}"))?;
            roots.push((root, settings, used.clone()));
        }
    }
    for (root, settings, used) in roots {
        let r = root.0 as usize;
        let mut walk = vec![(r, 0usize)];
        let mut order = vec![];
        while let Some((n, d)) = walk.pop() {
            order.push((n, d));
            walk.extend(doc.nodes.children[n].iter().map(|&c| (c, d + 1)));
        }
        let leaves: Vec<usize> = order.iter().filter(|(n, _)| doc.nodes.children[*n].is_empty()).map(|(n, _)| *n).collect();
        let tail = |n: usize| doc.nodes.rest[n].translation.normalize_or_zero() * LEAF_TAIL;
        let endpoint = leaves.first().map_or(Vec3::ZERO, |&l| tail(l));
        if leaves.iter().any(|&l| (tail(l) - endpoint).length() > 1e-4) {
            doc.report.add("vrm 0.x leaf tails in several directions");
        }
        let mut depths: Vec<Vec<f32>> = vec![];
        let mut at = |d: usize, length: f32| {
            if depths.len() <= d {
                depths.resize(d + 1, vec![]);
            }
            depths[d].push(length);
        };
        for &(n, d) in &order {
            for &c in &doc.nodes.children[n] {
                at(d, doc.length(n, c));
            }
            if doc.nodes.children[n].is_empty() && endpoint != Vec3::ZERO {
                at(d, LEAF_TAIL * doc.world[n].scale.x.abs());
            }
        }
        if depths.is_empty() {
            doc.report.add("vrm 0.x spring root with no bone to move");
            continue;
        }
        if depths.iter().any(|ls| ls.iter().any(|l| (l - ls[0]).abs() > 0.01 * ls[0])) {
            doc.report.add("vrm 0.x spring tree whose bones at one depth differ in length, given their mean");
        }
        let per_depth: Vec<(Joint, f32)> = depths.iter().map(|ls| (settings, ls.iter().sum::<f32>() / ls.len().max(1) as f32)).collect();
        physbones.push(physbone(root, vec![], endpoint, &per_depth, used));
    }
    Ok((physbones, colliders))
}
