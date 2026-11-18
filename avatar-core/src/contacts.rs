use glam::{Quat, Vec3};

use crate::model::{self, ContactShape, ReceiverType};
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
    /// The wearer's own (a hand the host tracks for the wearer) or another player's.
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

impl Contacts {
    pub fn new(avatar: &Avatar) -> Result<Contacts, Error> {
        let nodes = Nodes::read(avatar)?;
        let m = &avatar.model.contacts;
        let count = m.receivers.len();
        Ok(Contacts {
            senders: m.senders.clone(),
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
