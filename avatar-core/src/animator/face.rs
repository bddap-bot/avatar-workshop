//! The descriptor's lip sync and eyelids, which VRChat applies over the animator's output unless a
//! tracking-control behaviour has handed the mouth or the eyes to animation.

use glam::Quat;

use crate::model::{Descriptor, EyeRotations, Eyelids, LipSync, NodeIndex, Quat as ModelQuat};

use super::motion::{Target, Targets};
use crate::model::Axis;

enum Lips {
    None,
    /// One shape per viseme in VRChat's order; several visemes may share a shape.
    Visemes(Vec<Option<u32>>),
    JawShape(u32),
    JawBone { rotation: [u32; 4], closed: Quat, open: Quat },
}

struct Lid {
    rotation: [u32; 4],
    rest: Quat,
    closed: Quat,
    up: Quat,
    down: Quat,
}

enum Lids {
    None,
    Shapes { blink: Option<u32>, up: Option<u32>, down: Option<u32> },
    Bones(Vec<Lid>),
}

pub struct Face {
    lips: Lips,
    lids: Lids,
}

fn quat(q: ModelQuat) -> Quat {
    Quat::from_xyzw(q[0], q[1], q[2], q[3])
}

fn rotation(targets: &mut Targets, node: NodeIndex) -> [u32; 4] {
    [Axis::X, Axis::Y, Axis::Z, Axis::W].map(|a| targets.id(Target::Rotation(node.0, a)))
}

fn write(out: &mut [f32], ids: [u32; 4], q: Quat) {
    for (i, v) in ids.into_iter().zip([q.x, q.y, q.z, q.w]) {
        out[i as usize] = v;
    }
}

impl Face {
    pub fn new(d: &Descriptor, targets: &mut Targets) -> Face {
        let mut shape = |mesh: NodeIndex, name: &str| targets.id(Target::BlendShape(mesh.0, name.to_string()));
        let lips = match &d.lip_sync {
            LipSync::VisemeBlendShape { mesh, visemes: v } => Lips::Visemes(
                [&v.sil, &v.pp, &v.ff, &v.th, &v.dd, &v.kk, &v.ch, &v.ss, &v.nn, &v.rr, &v.aa, &v.e, &v.ih, &v.oh, &v.ou]
                    .into_iter()
                    .map(|n| (!n.is_empty()).then(|| shape(*mesh, n)))
                    .collect(),
            ),
            LipSync::JawFlapBlendShape { mesh, blend_shape } => Lips::JawShape(shape(*mesh, blend_shape)),
            LipSync::JawFlapBone { bone, closed, open } => {
                Lips::JawBone { rotation: rotation(targets, *bone), closed: quat(*closed), open: quat(*open) }
            }
            LipSync::Default {} | LipSync::VisemeParameterOnly {} => Lips::None,
        };
        let lids = match &d.eyes.eyelids {
            Eyelids::None {} => Lids::None,
            Eyelids::BlendShapes { mesh, blink, looking_up, looking_down } => {
                let mut named = |n: &Option<String>| n.as_ref().filter(|n| !n.is_empty()).map(|n| targets.id(Target::BlendShape(mesh.0, n.clone())));
                Lids::Shapes { blink: named(blink), up: named(looking_up), down: named(looking_down) }
            }
            Eyelids::Bones { left_upper, right_upper, left_lower, right_lower, default, closed, looking_up, looking_down } => {
                let side = |r: &EyeRotations, left: bool| quat(if left { r.left } else { r.right });
                let lids = [(left_upper, true, true), (right_upper, true, false), (left_lower, false, true), (right_lower, false, false)];
                Lids::Bones(
                    lids.into_iter()
                        .filter_map(|(node, upper, left)| {
                            let pick = |r: &crate::model::EyelidRotations| side(if upper { &r.upper } else { &r.lower }, left);
                            Some(Lid {
                                rotation: rotation(targets, (*node)?),
                                rest: pick(default),
                                closed: pick(closed),
                                up: pick(looking_up),
                                down: pick(looking_down),
                            })
                        })
                        .collect(),
                )
            }
        };
        Face { lips, lids }
    }

    /// Shapes the mouth for a viseme index, 0 being silence.
    pub fn speak(&self, viseme: usize, out: &mut [f32]) {
        match &self.lips {
            Lips::None => {}
            Lips::Visemes(ids) => {
                for &t in ids.iter().flatten() {
                    out[t as usize] = 0.0;
                }
                if let Some(Some(t)) = ids.get(viseme) {
                    out[*t as usize] = 100.0;
                }
            }
            Lips::JawShape(t) => out[*t as usize] = if viseme == 0 { 0.0 } else { 100.0 },
            Lips::JawBone { rotation, closed, open } => write(out, *rotation, if viseme == 0 { *closed } else { *open }),
        }
    }

    /// Sets the eyelids for a blink from 0 (open) to 1 (shut) and a vertical look from -1 (down) to 1 (up).
    pub fn look(&self, blink: f32, look_y: f32, out: &mut [f32]) {
        match &self.lids {
            Lids::None => {}
            Lids::Shapes { blink: b, up, down } => {
                for (t, v) in [(b, 100.0 * blink), (up, 100.0 * look_y.clamp(0.0, 1.0)), (down, -100.0 * look_y.clamp(-1.0, 0.0))] {
                    if let Some(t) = t {
                        out[*t as usize] = v;
                    }
                }
            }
            Lids::Bones(lids) => {
                for l in lids {
                    let toward = if look_y < 0.0 { l.down } else { l.up };
                    write(out, l.rotation, l.rest.slerp(toward, look_y.abs().min(1.0)).slerp(l.closed, blink.clamp(0.0, 1.0)));
                }
            }
        }
    }
}
