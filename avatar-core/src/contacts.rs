use glam::{Quat, Vec3};

use crate::model::{self, ContactShape, Descriptor, DescriptorCollider, DescriptorColliderState, HumanBodyBone, Humanoid, ReceiverType};
use crate::physbones::{Error, Nodes, Toucher, Transform};
use crate::Avatar;

/// A contact sender outside the avatar: another avatar's part or a hand the host tracks.
#[derive(Clone, Debug, PartialEq)]
pub struct Sender {
    /// Names the sender across frames, for whether it was inside a receiver and how fast it moves.
    pub id: u64,
    /// The pose of the transform the sender sits on, in the world.
    pub at: Transform,
    pub shape: ContactShape,
    pub position: Vec3,
    pub rotation: Quat,
    pub tags: Vec<String>,
    /// The wearer's own or another player's; the wearer's body parts are already the avatar's standard senders.
    pub owner: Toucher,
}

/// A segment (a point for a sphere) and a radius.
#[derive(Clone, Copy, Debug)]
struct Placed {
    a: Vec3,
    b: Vec3,
    radius: f32,
}

fn place(at: &Transform, shape: &ContactShape, position: Vec3, rotation: Quat) -> Placed {
    let center = at.point(position);
    let scale = at.scale.x.abs();
    match *shape {
        ContactShape::Sphere { radius } => Placed { a: center, b: center, radius: radius * scale },
        ContactShape::Capsule { radius, height } => {
            let half = ((height * 0.5 - radius) * scale).max(0.0);
            let axis = (at.rotation * rotation) * Vec3::Y * half;
            Placed { a: center - axis, b: center + axis, radius: radius * scale }
        }
    }
}

fn closest(p: &Placed, q: &Placed) -> (Vec3, Vec3) {
    let (d1, d2, r) = (p.b - p.a, q.b - q.a, p.a - q.a);
    let (a, e, f) = (d1.dot(d1), d2.dot(d2), d2.dot(r));
    let (s, t) = if a <= f32::EPSILON && e <= f32::EPSILON {
        (0.0, 0.0)
    } else if a <= f32::EPSILON {
        (0.0, (f / e).clamp(0.0, 1.0))
    } else {
        let c = d1.dot(r);
        if e <= f32::EPSILON {
            ((-c / a).clamp(0.0, 1.0), 0.0)
        } else {
            let b = d1.dot(d2);
            let denom = a * e - b * b;
            let mut s = if denom > 0.0 { ((b * f - c * e) / denom).clamp(0.0, 1.0) } else { 0.0 };
            let mut t = (b * s + f) / e;
            if t < 0.0 {
                t = 0.0;
                s = (-c / a).clamp(0.0, 1.0);
            } else if t > 1.0 {
                t = 1.0;
                s = ((b - c) / a).clamp(0.0, 1.0);
            }
            (s, t)
        }
    };
    (p.a + d1 * s, q.a + d2 * t)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    Own(usize),
    Outside(u64),
}

/// VRChat contacts for one avatar: its senders and receivers, and senders outside it the host supplies.
#[derive(Clone, Debug)]
pub struct Contacts {
    senders: Vec<model::ContactSender>,
    receivers: Vec<model::ContactReceiver>,
    parent: Vec<Option<usize>>,
    /// The senders inside each receiver last frame.
    inside: Vec<Vec<Key>>,
    running: Vec<bool>,
    values: Vec<Option<f32>>,
    /// Each sender's centre in the previous frame, for its speed.
    centers: std::collections::BTreeMap<Key, Vec3>,
}

/// The standard senders on the descriptor's enabled colliders, placed and tagged as the VRChat client places and tags
/// them; a finger's also carries `Finger` and `Finger` with its side. On a humanoid avatar an automatic collider is
/// the one the client computes from the skeleton in the humanoid's collider pose, whatever the descriptor stores.
pub fn standard_senders(descriptor: &Descriptor, humanoid: &Humanoid, nodes: &Nodes) -> Vec<model::ContactSender> {
    use HumanBodyBone as H;
    let c = &descriptor.colliders;
    let all: [(&DescriptorCollider, Part, &str, &str); 14] = [
        (&c.head, Part::Head, "Head", ""),
        (&c.torso, Part::Torso, "Torso", ""),
        (&c.hand_left, Part::Hand(Side::Left), "Hand", "L"),
        (&c.hand_right, Part::Hand(Side::Right), "Hand", "R"),
        (&c.foot_left, Part::Foot(Side::Left), "Foot", "L"),
        (&c.foot_right, Part::Foot(Side::Right), "Foot", "R"),
        (&c.finger_index_left, Part::Finger([H::LeftIndexProximal, H::LeftIndexIntermediate, H::LeftIndexDistal]), "FingerIndex", "L"),
        (&c.finger_middle_left, Part::Finger([H::LeftMiddleProximal, H::LeftMiddleIntermediate, H::LeftMiddleDistal]), "FingerMiddle", "L"),
        (&c.finger_ring_left, Part::Finger([H::LeftRingProximal, H::LeftRingIntermediate, H::LeftRingDistal]), "FingerRing", "L"),
        (&c.finger_little_left, Part::Finger([H::LeftLittleProximal, H::LeftLittleIntermediate, H::LeftLittleDistal]), "FingerLittle", "L"),
        (&c.finger_index_right, Part::Finger([H::RightIndexProximal, H::RightIndexIntermediate, H::RightIndexDistal]), "FingerIndex", "R"),
        (&c.finger_middle_right, Part::Finger([H::RightMiddleProximal, H::RightMiddleIntermediate, H::RightMiddleDistal]), "FingerMiddle", "R"),
        (&c.finger_ring_right, Part::Finger([H::RightRingProximal, H::RightRingIntermediate, H::RightRingDistal]), "FingerRing", "R"),
        (&c.finger_little_right, Part::Finger([H::RightLittleProximal, H::RightLittleIntermediate, H::RightLittleDistal]), "FingerLittle", "R"),
    ];
    let skeleton = Skeleton { humanoid, nodes, world: std::cell::OnceCell::new() };
    let mut out = vec![];
    for (stored, part, tag, side) in all {
        if stored.state == DescriptorColliderState::Disabled {
            continue;
        }
        let human = !humanoid.bones.is_empty();
        let collider = match stored.state {
            DescriptorColliderState::Automatic if human => skeleton.automatic(part, descriptor.view_position),
            _ => stored.clone(),
        };
        let Some(bone) = collider.bone else { continue };
        let bone = bone.0 as usize;
        let finger = matches!(part, Part::Finger(..));
        let (node, shape, position, rotation) = match (finger, nodes.parent[bone]) {
            (true, Some(parent)) => {
                let (shape, position, rotation) = finger_capsule(&collider, &nodes.rest[bone]);
                (parent, shape, position, rotation)
            }
            (true, None) => continue,
            (false, _) => (
                bone,
                ContactShape::Capsule { radius: collider.radius, height: collider.height },
                collider.position,
                collider.rotation,
            ),
        };
        let mut collision_tags = vec![tag.to_string()];
        if !side.is_empty() {
            collision_tags.push(format!("{tag}{side}"));
        }
        if finger {
            collision_tags.extend(["Finger".to_string(), format!("Finger{side}")]);
        }
        out.push(model::ContactSender {
            node: model::NodeIndex(node as u32),
            enabled: true,
            shape,
            position,
            rotation,
            collision_tags,
        });
    }
    out
}

#[derive(Clone, Copy, PartialEq)]
enum Side {
    Left,
    Right,
}

#[derive(Clone, Copy)]
enum Part {
    Head,
    Torso,
    Hand(Side),
    Foot(Side),
    /// Proximal, intermediate and distal.
    Finger([HumanBodyBone; 3]),
}

struct Skeleton<'a> {
    humanoid: &'a Humanoid,
    nodes: &'a Nodes,
    world: std::cell::OnceCell<Vec<Transform>>,
}

impl Skeleton<'_> {
    fn bone(&self, b: HumanBodyBone) -> Option<usize> {
        self.humanoid.bones.get(&b).map(|h| h.node.0 as usize)
    }

    fn root(&self, mut node: usize) -> usize {
        while let Some(p) = self.nodes.parent[node] {
            node = p;
        }
        node
    }

    fn at(&self, node: usize) -> Transform {
        self.world.get_or_init(|| {
            let mut locals = self.nodes.rest.clone();
            for b in self.humanoid.bones.values() {
                if let Some(pose) = &b.collider_pose {
                    let local = &mut locals[b.node.0 as usize];
                    local.translation = Vec3::from(pose.position);
                    local.rotation = Quat::from_array(pose.rotation).normalize();
                }
            }
            self.nodes.world(&locals)
        })[node]
    }

    fn pos(&self, b: HumanBodyBone) -> Option<Vec3> {
        self.bone(b).map(|n| self.at(n).translation)
    }

    /// The collider the client computes for an automatic one, as the SDK's descriptor does from the skeleton;
    /// none with a bone when the humanoid lacks the bones it needs.
    fn automatic(&self, part: Part, view: model::Vec3) -> DescriptorCollider {
        use HumanBodyBone as H;
        let none = DescriptorCollider {
            state: DescriptorColliderState::Automatic,
            bone: None,
            radius: 0.0,
            height: 0.0,
            position: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
        };
        let on = |node: usize, radius: f32, height: f32, position: Vec3, rotation: Quat| DescriptorCollider {
            bone: Some(model::NodeIndex(node as u32)),
            radius,
            height,
            position: position.to_array(),
            rotation: rotation.to_array(),
            ..none.clone()
        };
        let scale = |node: usize| self.at(node).scale.abs().max_element();
        let local = |node: usize, p: Vec3| {
            let t = self.at(node);
            (t.rotation.inverse() * (p - t.translation)) / t.scale
        };
        let along = |node: usize, d: Vec3| match (self.at(node).rotation.inverse() * d).try_normalize() {
            Some(d) => Quat::from_rotation_arc(Vec3::Y, d),
            None => Quat::IDENTITY,
        };
        let shoulders = || Some(self.pos(H::LeftUpperArm)?.distance(self.pos(H::RightUpperArm)?));
        match part {
            Part::Head => {
                let Some(head) = self.bone(H::Head) else { return none };
                let at = self.at(head).translation;
                let eyes: Vec<f32> = [H::LeftEye, H::RightEye].into_iter().filter_map(|e| self.pos(e)).map(|p| p.y).collect();
                let root = self.at(self.root(head));
                let y = match eyes.len() {
                    0 => view[1] * root.scale.abs().max_element() + root.translation.y,
                    n => eyes.iter().sum::<f32>() / n as f32,
                };
                let radius = ((y - at.y).abs() * 0.8).max(shoulders().unwrap_or(0.0) * 0.33) / scale(head);
                on(head, radius, 0.0, local(head, Vec3::new(at.x, y, at.z)), Quat::IDENTITY)
            }
            Part::Torso => {
                let (Some(hips), Some(chest), Some(neck)) = (self.pos(H::Hips), self.bone(H::Chest).or(self.bone(H::Spine)), self.pos(H::Neck)) else {
                    return none;
                };
                let height = hips.distance(neck) / scale(chest);
                let radius = match shoulders() {
                    Some(w) => height.min(w / scale(chest)) * 0.5,
                    None => height * 0.3,
                };
                on(chest, radius, height, local(chest, (hips + neck) * 0.5), along(chest, neck - hips))
            }
            Part::Hand(side) => {
                let (hand, fingers, lower) = match side {
                    Side::Left => (H::LeftHand, [H::LeftIndexProximal, H::LeftMiddleProximal, H::LeftRingProximal, H::LeftLittleProximal], H::LeftLowerArm),
                    Side::Right => (H::RightHand, [H::RightIndexProximal, H::RightMiddleProximal, H::RightRingProximal, H::RightLittleProximal], H::RightLowerArm),
                };
                let Some(node) = self.bone(hand) else { return none };
                let at = self.at(node).translation;
                let roots: Vec<Vec3> = fingers.into_iter().filter_map(|f| self.pos(f)).collect();
                if roots.is_empty() {
                    let Some(lower) = self.pos(lower) else { return none };
                    let length = at.distance(lower);
                    return on(node, length * 0.2 / scale(node), 0.0, local(node, at + (at - lower).normalize_or_zero() * length * 0.2), Quat::IDENTITY);
                }
                let mid = roots.iter().sum::<Vec3>() / roots.len() as f32;
                let length = at.distance(mid);
                let across = (mid - at).normalize_or_zero().cross(Vec3::Y).normalize_or_zero();
                on(node, length * 0.25 / scale(node), length / scale(node), local(node, mid) * 0.85, along(node, across))
            }
            Part::Foot(side) => {
                let (upper, lower, foot, toes) = match side {
                    Side::Left => (H::LeftUpperLeg, H::LeftLowerLeg, H::LeftFoot, H::LeftToes),
                    Side::Right => (H::RightUpperLeg, H::RightLowerLeg, H::RightFoot, H::RightToes),
                };
                let (Some(upper), Some(lower), Some(foot)) = (self.pos(upper), self.pos(lower), self.bone(foot)) else { return none };
                let child = |name: &str| {
                    self.nodes.children[foot].iter().copied().find(|&c| self.nodes.names.get(c).is_some_and(|n| n.to_lowercase().contains(name)))
                };
                let node = self.bone(toes).or_else(|| child("toe")).or_else(|| child("_end")).unwrap_or(foot);
                let leg = upper.distance(lower) + lower.distance(self.at(foot).translation);
                on(node, leg * 0.1 / scale(node), 0.0, Vec3::ZERO, Quat::IDENTITY)
            }
            Part::Finger([proximal, intermediate, distal]) => {
                let (Some(p), Some(i)) = (self.bone(proximal), self.bone(intermediate)) else { return none };
                let (from, to) = match self.bone(distal) {
                    Some(d) => (i, d),
                    None => (p, i),
                };
                let length = self.at(from).translation.distance(self.at(to).translation);
                on(to, length * 0.4 / scale(to), length * 2.0 / scale(to), Vec3::ZERO, Quat::IDENTITY)
            }
        }
    }
}

/// A finger's capsule in its bone's parent's space, from the parent's origin through the bone, run on in the bone's own
/// space to the collider's height; the collider's offsets go unused.
fn finger_capsule(collider: &DescriptorCollider, bone: &Transform) -> (ContactShape, model::Vec3, model::Quat) {
    let length = bone.translation.length();
    let reach = bone.translation.normalize_or_zero() * (collider.height - length).max(0.0);
    let end = bone.point(reach);
    let radius = collider.radius * bone.scale.abs().max_element();
    let rotation = Quat::from_rotation_arc(Vec3::Y, end.normalize_or(Vec3::Y));
    (ContactShape::Capsule { radius, height: end.length() + 2.0 * radius }, (end * 0.5).to_array(), rotation.to_array())
}

impl Contacts {
    /// The avatar's own senders are its model's, then the standard ones on its descriptor's colliders.
    pub fn new(avatar: &Avatar) -> Result<Contacts, Error> {
        let nodes = Nodes::read(avatar)?;
        let m = &avatar.model.contacts;
        let count = m.receivers.len();
        let mut senders = m.senders.clone();
        senders.extend(standard_senders(&avatar.model.descriptor, &avatar.model.humanoid, &nodes));
        Ok(Contacts {
            senders,
            receivers: m.receivers.clone(),
            parent: nodes.parent,
            inside: vec![vec![]; count],
            running: vec![false; count],
            values: vec![None; count],
            centers: Default::default(),
        })
    }

    pub fn receivers(&self) -> &[model::ContactReceiver] {
        &self.receivers
    }

    /// Each receiver's parameter value after the last [`Contacts::advance`], none for a receiver that is not running
    /// and so leaves its parameter to the animator.
    pub fn values(&self) -> &[Option<f32>] {
        &self.values
    }

    /// Sets a sender's field by the name an animation curve binds it with; an unknown field is ignored.
    pub fn set_sender(&mut self, index: usize, field: &str, value: f32) {
        if let (Some(s), "m_Enabled") = (self.senders.get_mut(index), field) {
            s.enabled = value != 0.0;
        }
    }

    /// Sets a receiver's field by the name an animation curve binds it with; an unknown field is ignored.
    pub fn set_receiver(&mut self, index: usize, field: &str, value: f32) {
        let Some(r) = self.receivers.get_mut(index) else { return };
        let on = value != 0.0;
        match field {
            "m_Enabled" => r.enabled = on,
            "allowSelf" => r.allow_self = on,
            "allowOthers" => r.allow_others = on,
            "localOnly" => r.local_only = on,
            "minVelocity" => {
                if let ReceiverType::OnEnter { min_velocity } = &mut r.receiver_type {
                    *min_velocity = value
                }
            }
            _ => {}
        }
    }

    /// Advances by `dt` seconds with every node's world pose and own active flag, and the senders outside the
    /// avatar, updating each receiver's value. `local` is whether the avatar is the local player's.
    pub fn advance(&mut self, dt: f32, world: &[Transform], active: &[bool], outside: &[Sender], local: bool) {
        let on = |node: usize| {
            let mut at = Some(node);
            while let Some(n) = at {
                if !active[n] {
                    return false;
                }
                at = self.parent[n];
            }
            true
        };
        let mut senders: Vec<(Placed, &[String], Toucher)> = vec![];
        let mut keys = vec![];
        for (k, s) in self.senders.iter().enumerate() {
            let n = s.node.0 as usize;
            if s.enabled && on(n) {
                senders.push((place(&world[n], &s.shape, Vec3::from(s.position), Quat::from_array(s.rotation)), &s.collision_tags, Toucher::Wearer));
                keys.push(Key::Own(k));
            }
        }
        for s in outside {
            senders.push((place(&s.at, &s.shape, s.position, s.rotation), &s.tags, s.owner));
            keys.push(Key::Outside(s.id));
        }
        for (i, r) in self.receivers.iter().enumerate() {
            let n = r.node.0 as usize;
            let running = r.enabled && on(n) && (local || !r.local_only);
            let was = std::mem::replace(&mut self.running[i], running);
            if !running {
                self.values[i] = None;
                self.inside[i].clear();
                continue;
            }
            if !was {
                self.values[i] = None;
                continue;
            }
            let me = place(&world[n], &r.shape, Vec3::from(r.position), Quat::from_array(r.rotation));
            let mut inside = vec![];
            let mut proximity = 0.0f32;
            let mut entered = false;
            for ((s, tags, owner), key) in senders.iter().zip(&keys) {
                let allowed = match owner {
                    Toucher::Wearer => r.allow_self,
                    Toucher::Other => r.allow_others,
                };
                if !allowed || !tags.iter().any(|t| r.collision_tags.contains(t)) {
                    continue;
                }
                let (x, y) = closest(&me, s);
                let d = (x - y).length();
                let reach = me.radius + s.radius;
                if d >= reach {
                    continue;
                }
                let stayed = self.inside[i].contains(key);
                if !stayed && dt <= 0.0 {
                    continue;
                }
                if stayed {
                    proximity = proximity.max(if me.radius > 0.0 { ((reach - d) / me.radius).clamp(0.0, 1.0) } else { 1.0 });
                }
                if let (&ReceiverType::OnEnter { min_velocity }, false) = (&r.receiver_type, stayed) {
                    let speed = self.centers.get(key).map_or(0.0, |b| ((s.a + s.b) * 0.5 - *b).length() / dt);
                    entered |= speed >= min_velocity;
                }
                inside.push(*key);
            }
            self.values[i] = Some(match r.receiver_type {
                ReceiverType::Constant {} => (!inside.is_empty()) as u8 as f32,
                ReceiverType::OnEnter { .. } => entered as u8 as f32,
                ReceiverType::Proximity {} => proximity,
            });
            self.inside[i] = inside;
        }
        self.centers = senders.iter().zip(&keys).map(|((s, ..), k)| (*k, (s.a + s.b) * 0.5)).collect();
    }
}
