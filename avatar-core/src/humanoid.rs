//! Unity's humanoid Avatar: the human stream's muscles and body become the humanoid bones' local rotations and the
//! hips' position, as Unity poses them from the frames its Avatar exports (`Humanoid::muscles`).

use glam::{Mat3, Quat, Vec3};

use crate::model::{HumanBodyBone as B, Humanoid, MuscleFrame};
use crate::physbones::Transform;

/// Unity's muscle curves of each bone, by degree of freedom: twist about x, then y and z.
pub const MUSCLES: [(B, [Option<&str>; 3]); 54] = {
    const N: Option<&str> = None;
    [
        (B::LeftUpperLeg, [Some("Left Upper Leg Twist In-Out"), Some("Left Upper Leg In-Out"), Some("Left Upper Leg Front-Back")]),
        (B::RightUpperLeg, [Some("Right Upper Leg Twist In-Out"), Some("Right Upper Leg In-Out"), Some("Right Upper Leg Front-Back")]),
        (B::LeftLowerLeg, [Some("Left Lower Leg Twist In-Out"), N, Some("Left Lower Leg Stretch")]),
        (B::RightLowerLeg, [Some("Right Lower Leg Twist In-Out"), N, Some("Right Lower Leg Stretch")]),
        (B::LeftFoot, [N, Some("Left Foot Twist In-Out"), Some("Left Foot Up-Down")]),
        (B::RightFoot, [N, Some("Right Foot Twist In-Out"), Some("Right Foot Up-Down")]),
        (B::Spine, [Some("Spine Twist Left-Right"), Some("Spine Left-Right"), Some("Spine Front-Back")]),
        (B::Chest, [Some("Chest Twist Left-Right"), Some("Chest Left-Right"), Some("Chest Front-Back")]),
        (B::UpperChest, [Some("UpperChest Twist Left-Right"), Some("UpperChest Left-Right"), Some("UpperChest Front-Back")]),
        (B::Neck, [Some("Neck Turn Left-Right"), Some("Neck Tilt Left-Right"), Some("Neck Nod Down-Up")]),
        (B::Head, [Some("Head Turn Left-Right"), Some("Head Tilt Left-Right"), Some("Head Nod Down-Up")]),
        (B::LeftShoulder, [N, Some("Left Shoulder Front-Back"), Some("Left Shoulder Down-Up")]),
        (B::RightShoulder, [N, Some("Right Shoulder Front-Back"), Some("Right Shoulder Down-Up")]),
        (B::LeftUpperArm, [Some("Left Arm Twist In-Out"), Some("Left Arm Front-Back"), Some("Left Arm Down-Up")]),
        (B::RightUpperArm, [Some("Right Arm Twist In-Out"), Some("Right Arm Front-Back"), Some("Right Arm Down-Up")]),
        (B::LeftLowerArm, [Some("Left Forearm Twist In-Out"), N, Some("Left Forearm Stretch")]),
        (B::RightLowerArm, [Some("Right Forearm Twist In-Out"), N, Some("Right Forearm Stretch")]),
        (B::LeftHand, [N, Some("Left Hand In-Out"), Some("Left Hand Down-Up")]),
        (B::RightHand, [N, Some("Right Hand In-Out"), Some("Right Hand Down-Up")]),
        (B::LeftToes, [N, N, Some("Left Toes Up-Down")]),
        (B::RightToes, [N, N, Some("Right Toes Up-Down")]),
        (B::LeftEye, [N, Some("Left Eye In-Out"), Some("Left Eye Down-Up")]),
        (B::RightEye, [N, Some("Right Eye In-Out"), Some("Right Eye Down-Up")]),
        (B::Jaw, [N, Some("Jaw Left-Right"), Some("Jaw Close")]),
        (B::LeftThumbProximal, [N, Some("LeftHand.Thumb.Spread"), Some("LeftHand.Thumb.1 Stretched")]),
        (B::LeftThumbIntermediate, [N, N, Some("LeftHand.Thumb.2 Stretched")]),
        (B::LeftThumbDistal, [N, N, Some("LeftHand.Thumb.3 Stretched")]),
        (B::LeftIndexProximal, [N, Some("LeftHand.Index.Spread"), Some("LeftHand.Index.1 Stretched")]),
        (B::LeftIndexIntermediate, [N, N, Some("LeftHand.Index.2 Stretched")]),
        (B::LeftIndexDistal, [N, N, Some("LeftHand.Index.3 Stretched")]),
        (B::LeftMiddleProximal, [N, Some("LeftHand.Middle.Spread"), Some("LeftHand.Middle.1 Stretched")]),
        (B::LeftMiddleIntermediate, [N, N, Some("LeftHand.Middle.2 Stretched")]),
        (B::LeftMiddleDistal, [N, N, Some("LeftHand.Middle.3 Stretched")]),
        (B::LeftRingProximal, [N, Some("LeftHand.Ring.Spread"), Some("LeftHand.Ring.1 Stretched")]),
        (B::LeftRingIntermediate, [N, N, Some("LeftHand.Ring.2 Stretched")]),
        (B::LeftRingDistal, [N, N, Some("LeftHand.Ring.3 Stretched")]),
        (B::LeftLittleProximal, [N, Some("LeftHand.Little.Spread"), Some("LeftHand.Little.1 Stretched")]),
        (B::LeftLittleIntermediate, [N, N, Some("LeftHand.Little.2 Stretched")]),
        (B::LeftLittleDistal, [N, N, Some("LeftHand.Little.3 Stretched")]),
        (B::RightThumbProximal, [N, Some("RightHand.Thumb.Spread"), Some("RightHand.Thumb.1 Stretched")]),
        (B::RightThumbIntermediate, [N, N, Some("RightHand.Thumb.2 Stretched")]),
        (B::RightThumbDistal, [N, N, Some("RightHand.Thumb.3 Stretched")]),
        (B::RightIndexProximal, [N, Some("RightHand.Index.Spread"), Some("RightHand.Index.1 Stretched")]),
        (B::RightIndexIntermediate, [N, N, Some("RightHand.Index.2 Stretched")]),
        (B::RightIndexDistal, [N, N, Some("RightHand.Index.3 Stretched")]),
        (B::RightMiddleProximal, [N, Some("RightHand.Middle.Spread"), Some("RightHand.Middle.1 Stretched")]),
        (B::RightMiddleIntermediate, [N, N, Some("RightHand.Middle.2 Stretched")]),
        (B::RightMiddleDistal, [N, N, Some("RightHand.Middle.3 Stretched")]),
        (B::RightRingProximal, [N, Some("RightHand.Ring.Spread"), Some("RightHand.Ring.1 Stretched")]),
        (B::RightRingIntermediate, [N, N, Some("RightHand.Ring.2 Stretched")]),
        (B::RightRingDistal, [N, N, Some("RightHand.Ring.3 Stretched")]),
        (B::RightLittleProximal, [N, Some("RightHand.Little.Spread"), Some("RightHand.Little.1 Stretched")]),
        (B::RightLittleIntermediate, [N, N, Some("RightHand.Little.2 Stretched")]),
        (B::RightLittleDistal, [N, N, Some("RightHand.Little.3 Stretched")]),
    ]
};

/// Unity's limits of an UpperChest, which an avatar without one adds to its Chest.
const UPPER_CHEST_DEGREES: f32 = 20.0;

/// A body position (`RootT`) and rotation (`RootQ`) as the human stream holds them, in Unity's axes.
#[derive(Clone, Copy, Debug)]
pub struct Body {
    pub position: Vec3,
    pub rotation: Quat,
}

struct Bone {
    node: usize,
    pre: Quat,
    post: Quat,
    sign: Vec3,
    min: Vec3,
    max: Vec3,
    /// Each degree of freedom's muscle, by index into `MUSCLES`' flattened names.
    muscles: [Option<usize>; 3],
    /// An absent UpperChest's muscles, which a Chest takes.
    extra: [Option<usize>; 3],
}

/// The humanoid bones of one avatar, posed from muscle values named by `MUSCLES`.
pub struct Human {
    hips: usize,
    /// Each humanoid bone but the hips at its position in the human description's skeleton, where Unity's
    /// humanoid stream puts it whatever the node's own.
    positions: Vec<(usize, Vec3)>,
    bones: Vec<Bone>,
    twists: Vec<(usize, usize, f32)>,
    /// Each bone's mass and its centre: a weighted mean of bone positions.
    centres: Vec<(f32, Vec<(usize, f32)>)>,
    frame: [usize; 4],
    scale: f32,
    t_pose_body: Quat,
}

/// Every muscle name of `MUSCLES`, in order: a muscle's index in the values `Human::pose` reads.
pub fn muscle_names() -> Vec<&'static str> {
    MUSCLES.iter().flat_map(|(_, m)| m.iter().flatten().copied()).collect()
}

/// The model's axes to Unity's: x mirrored.
fn unity_point(p: Vec3) -> Vec3 {
    Vec3::new(-p.x, p.y, p.z)
}

fn unity_rotation(q: Quat) -> Quat {
    Quat::from_xyzw(q.x, -q.y, -q.z, q.w)
}

impl Human {
    /// None when the model holds no muscle frames or lacks a bone the body frame needs.
    pub fn new(h: &Humanoid) -> Option<Human> {
        let m = h.muscles.as_ref()?;
        let node = |b: B| h.bones.get(&b).map(|x| x.node.0 as usize);
        let frame = |b: B| m.bones.get(&b);
        let q = |a: [f32; 4]| Quat::from_array(a).normalize();
        let names = muscle_names();
        let index = |n: &str| names.iter().position(|x| *x == n);
        let mut bones = vec![];
        for (b, ms) in MUSCLES {
            let (Some(n), Some(f)) = (node(b), frame(b)) else { continue };
            let MuscleFrame { pre, post, sign, min, max, .. } = f;
            let mut extra = [None; 3];
            if b == B::Chest && node(B::UpperChest).is_none() {
                let up = MUSCLES.iter().find(|(x, _)| *x == B::UpperChest).unwrap().1;
                extra = up.map(|n| n.and_then(index));
            }
            bones.push(Bone {
                node: n,
                pre: q(*pre),
                post: q(*post),
                sign: Vec3::from_array(*sign),
                min: Vec3::from_array(*min).map(f32::to_radians),
                max: Vec3::from_array(*max).map(f32::to_radians),
                muscles: ms.map(|n| n.and_then(index)),
                extra,
            });
        }
        let mut twists = vec![];
        for (arm, leg) in [([B::LeftUpperArm, B::LeftLowerArm, B::LeftHand], [B::LeftUpperLeg, B::LeftLowerLeg, B::LeftFoot]), ([B::RightUpperArm, B::RightLowerArm, B::RightHand], [B::RightUpperLeg, B::RightLowerLeg, B::RightFoot])] {
            for (p, c, k) in [(arm[0], arm[1], m.arm_twist), (arm[1], arm[2], m.forearm_twist), (leg[0], leg[1], m.leg_twist), (leg[1], leg[2], m.leg_twist)] {
                let at = |b: B| node(b).and_then(|n| bones.iter().position(|x| x.node == n));
                if let (Some(p), Some(c)) = (at(p), at(c)) {
                    twists.push((p, c, k));
                }
            }
        }
        let mut centres = vec![];
        for (b, f) in &m.bones {
            if f.mass == 0.0 {
                continue;
            }
            let own = node(*b)?;
            let chest_child = if node(B::UpperChest).is_some() { B::UpperChest } else { B::Neck };
            let rule: Vec<(B, f32)> = match b {
                B::Hips => vec![(B::LeftUpperLeg, 1.0), (B::RightUpperLeg, 1.0), (B::Spine, 1.0)],
                B::LeftUpperLeg => vec![(B::LeftUpperLeg, 1.0), (B::LeftLowerLeg, 1.0)],
                B::RightUpperLeg => vec![(B::RightUpperLeg, 1.0), (B::RightLowerLeg, 1.0)],
                B::LeftLowerLeg => vec![(B::LeftLowerLeg, 1.0), (B::LeftFoot, 1.0)],
                B::RightLowerLeg => vec![(B::RightLowerLeg, 1.0), (B::RightFoot, 1.0)],
                B::Spine => vec![(B::Spine, 1.0), (B::Chest, 1.0)],
                B::Chest => vec![(B::Chest, 1.0), (chest_child, 1.0), (B::LeftShoulder, 1.0), (B::RightShoulder, 1.0)],
                B::Neck => vec![(B::Neck, 1.0), (B::Head, 1.0)],
                B::LeftShoulder => vec![(B::LeftShoulder, 1.0), (B::LeftUpperArm, 1.0)],
                B::RightShoulder => vec![(B::RightShoulder, 1.0), (B::RightUpperArm, 1.0)],
                B::LeftUpperArm => vec![(B::LeftUpperArm, 7.0), (B::LeftLowerArm, 1.0)],
                B::RightUpperArm => vec![(B::RightUpperArm, 1.0), (B::RightLowerArm, 1.0)],
                B::LeftLowerArm => vec![(B::LeftLowerArm, 1.0), (B::LeftHand, 1.0)],
                B::RightLowerArm => vec![(B::RightLowerArm, 1.0), (B::RightHand, 1.0)],
                _ => vec![(*b, 1.0)],
            };
            let spec: Vec<(usize, f32)> = rule.iter().filter_map(|(b, w)| Some((node(*b)?, *w))).collect();
            let spec = if spec.is_empty() { vec![(own, 1.0)] } else { spec };
            let total: f32 = spec.iter().map(|s| s.1).sum();
            centres.push((f.mass, spec.into_iter().map(|(n, w)| (n, w / total)).collect()));
        }
        let hips = node(B::Hips)?;
        let mapped: std::collections::BTreeSet<usize> = h.bones.values().map(|b| b.node.0 as usize).collect();
        let positions = h.skeleton.iter().map(|s| (s.node.0 as usize, Vec3::from_array(s.position))).filter(|(n, _)| *n != hips && mapped.contains(n)).collect();
        Some(Human {
            hips,
            positions,
            bones,
            twists,
            centres,
            frame: [node(B::LeftUpperLeg)?, node(B::RightUpperLeg)?, node(B::LeftUpperArm)?, node(B::RightUpperArm)?],
            scale: m.scale,
            t_pose_body: unity_rotation(q(m.t_pose_body)),
        })
    }

    /// Poses the humanoid bones in `locals` from muscle values (indexed as `muscle_names`) and the body, in the
    /// root node's space without its scale; every other node keeps its local.
    pub fn pose(&self, parent: &[Option<usize>], locals: &mut [Transform], muscles: &[f32], body: Body) {
        let value = |i: Option<usize>| i.map_or(0.0, |i| muscles[i]);
        for &(n, p) in &self.positions {
            locals[n].translation = p;
        }
        let mut rolls = vec![0.0; self.bones.len()];
        for (k, b) in self.bones.iter().enumerate() {
            let dof = Vec3::from_array(b.muscles.map(value));
            let mut angle = Vec3::select(dof.cmplt(Vec3::ZERO), -dof * b.min, dof * b.max);
            if b.extra.iter().any(Option::is_some) {
                angle += Vec3::from_array(b.extra.map(value)) * UPPER_CHEST_DEGREES.to_radians();
            }
            let t = (0.5 * angle * b.sign).map(libm::tanf);
            let r = Quat::from_xyzw(t.x, t.y + t.x * t.z, t.z - t.x * t.y, 1.0).normalize();
            rolls[k] = angle.x * b.sign.x;
            locals[b.node].rotation = (b.pre * r * b.post.conjugate()).normalize();
        }
        for &(p, c, keep) in &self.twists {
            let post = self.bones[p].post;
            let moved = post * Quat::from_rotation_x((1.0 - keep) * rolls[p]) * post.conjugate();
            let (pn, cn) = (self.bones[p].node, self.bones[c].node);
            locals[pn].rotation = (locals[pn].rotation * moved.conjugate()).normalize();
            locals[cn].rotation = (moved * locals[cn].rotation).normalize();
        }
        let hips = self.hips;
        let above = parent[hips].map_or(Transform::IDENTITY, |p| root_space(parent, locals, p));
        let at_hips = Transform { scale: above.scale * locals[hips].scale, ..Transform::IDENTITY };
        let from_hips = |n: usize| -> Vec3 {
            let mut chain = vec![];
            let mut x = n;
            while x != hips {
                chain.push(x);
                x = parent[x].expect("a humanoid bone under the hips");
            }
            unity_point(chain.iter().rev().fold(at_hips, |t, &c| t.mul(&locals[c])).translation)
        };
        let p = self.frame.map(from_hips);
        let up = ((p[2] + p[3]) - (p[0] + p[1])).normalize();
        let right = (p[1] - p[0]) + (p[3] - p[2]);
        let forward = right.cross(up).normalize();
        let posed = Quat::from_mat3(&Mat3::from_cols(up.cross(forward), up, forward));
        let centre: Vec3 = self.centres.iter().map(|(m, spec)| *m * spec.iter().map(|(n, w)| *w * from_hips(*n)).sum::<Vec3>()).sum();
        let rotation = (body.rotation * self.t_pose_body * posed.conjugate()).normalize();
        let position = body.position * self.scale - rotation * centre;
        let world = Transform { translation: unity_point(position), rotation: unity_rotation(rotation), scale: at_hips.scale };
        let local = above.relative(&world);
        locals[hips].translation = local.translation;
        locals[hips].rotation = local.rotation;
    }
}

/// A node's pose in the root node's space without the root's own transform.
fn root_space(parent: &[Option<usize>], locals: &[Transform], n: usize) -> Transform {
    match parent[n] {
        Some(p) => root_space(parent, locals, p).mul(&locals[n]),
        None => Transform::IDENTITY,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique() {
        let n = muscle_names();
        assert_eq!(n.len(), 95);
        let mut s = n.clone();
        s.sort();
        s.dedup();
        assert_eq!(s.len(), 95);
    }
}
