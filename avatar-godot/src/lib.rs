//! The evaluation core for Godot: `AvatarModel` reads a model file or a VRM, `AvatarEvaluator` runs its frames and
//! applies their output to the scene Godot's glTF importer built from `AvatarModel.glb()`.

use std::collections::HashMap;

use avatar_core::evaluate::{Evaluator, Input};
use glam::{Quat, Vec3};
use avatar_core::physbones::{HandCollider, Toucher, Transform};
use avatar_core::Avatar;
use godot::classes::{Material, MeshInstance3D, Node3D, RenderingServer, Skeleton3D};
use godot::prelude::*;

struct AvatarCore;

#[gdextension]
unsafe impl ExtensionLibrary for AvatarCore {}

/// A model, from a model file or a VRM through the core's importer.
#[derive(GodotClass)]
#[class(no_init, base = RefCounted)]
pub struct AvatarModel {
    avatar: Avatar,
    glb: PackedByteArray,
}

#[godot_api]
impl AvatarModel {
    /// The model a `.glb` with `AW_avatar` or a VRM 0.x/1.0 holds, or why it holds none.
    #[func]
    fn read(bytes: PackedByteArray) -> Variant {
        let b = bytes.as_slice();
        let read = match avatar_core::read(b) {
            Ok(a) => Ok((a, bytes.clone())),
            Err(avatar_core::Error::MissingExtension) => avatar_core::vrm::import(b)
                .map_err(|e| e.to_string())
                .and_then(|a| avatar_core::write(&a).map(|g| (a, PackedByteArray::from(g.as_slice()))).map_err(|e| e.to_string())),
            Err(e) => Err(e.to_string()),
        };
        match read {
            Ok((avatar, glb)) => Gd::from_object(AvatarModel { avatar, glb }).to_variant(),
            Err(e) => GString::from(e.as_str()).to_variant(),
        }
    }

    /// The glTF binary whose nodes, meshes, skins and materials the model's indices name.
    #[func]
    fn glb(&self) -> PackedByteArray {
        self.glb.clone()
    }

    /// The `AW_avatar` extension as JSON.
    #[func]
    fn model(&self) -> GString {
        GString::from(serde_json::to_string(&self.avatar.model).expect("a model serializes").as_str())
    }
}

/// One avatar's hand and finger colliders after a frame, in the world, for the avatars around it.
#[derive(GodotClass)]
#[class(no_init, base = RefCounted)]
pub struct AvatarHands {
    hands: Vec<HandCollider>,
}

/// One avatar's frames through [`Evaluator`].
#[derive(GodotClass)]
#[class(no_init, base = RefCounted)]
pub struct AvatarEvaluator {
    evaluator: Evaluator,
    slots: Vec<Vec<u32>>,
    physbones: usize,
    poses: Vec<(usize, Transform)>,
    scene: Option<Scene>,
}

struct Scene {
    bones: Vec<Option<(Gd<Skeleton3D>, i32)>>,
    nodes: Vec<Option<Gd<Node3D>>>,
    renderers: Vec<Option<Gd<MeshInstance3D>>>,
    materials: Vec<Gd<Material>>,
    shapes: HashMap<(u32, String), i32>,
    renderer_of: HashMap<u32, usize>,
}

fn from_godot(t: Transform3D) -> Transform {
    let mut b = t.basis;
    let mirror = if b.determinant() < 0.0 { -1.0 } else { 1.0 };
    b.set_col_a(b.col_a() * mirror);
    let (mut s, q) = (b.get_scale(), b.get_quaternion());
    s.x *= mirror;
    Transform { translation: Vec3::new(t.origin.x, t.origin.y, t.origin.z), rotation: Quat::from_xyzw(q.x, q.y, q.z, q.w).normalize(), scale: Vec3::new(s.x, s.y, s.z) }
}

fn to_godot(t: &Transform) -> Transform3D {
    let (p, q, s) = (t.translation, t.rotation, t.scale);
    let basis = Basis::from_quaternion(Quaternion::new(q.x, q.y, q.z, q.w)) * Basis::from_scale(Vector3::new(s.x, s.y, s.z));
    Transform3D::new(basis, Vector3::new(p.x, p.y, p.z))
}

fn who(other: bool) -> Toucher {
    if other {
        Toucher::Other
    } else {
        Toucher::Wearer
    }
}

#[godot_api]
impl AvatarEvaluator {
    /// An evaluator for `model`, or why the core refuses it.
    #[func]
    fn create(model: Gd<AvatarModel>) -> Variant {
        let m = model.bind();
        match Evaluator::new(&m.avatar) {
            Ok(evaluator) => Gd::from_object(AvatarEvaluator {
                evaluator,
                slots: m.avatar.model.renderers.iter().map(|r| r.materials.iter().map(|x| x.0).collect()).collect(),
                physbones: m.avatar.model.physbones.len(),
                poses: vec![],
                scene: None,
            })
            .to_variant(),
            Err(e) => GString::from(e.to_string().as_str()).to_variant(),
        }
    }

    /// Binds the scene `apply` writes, once, by glTF node: the skeleton holding it (null for none) and its bone there,
    /// or its own node; each model renderer's mesh instance; and the glTF materials by index. Each renderer takes its
    /// own material slots.
    #[func]
    fn bind(&mut self, skeletons: Array<Variant>, bones: PackedInt32Array, nodes: Array<Variant>, renderers: Array<Variant>, materials: Array<Gd<Material>>) {
        if self.scene.is_some() {
            godot_error!("avatar: a scene is already bound");
            return;
        }
        let count = self.evaluator.parents().len();
        let drawn = self.evaluator.renderer_nodes().len();
        let node = |v: Variant| v.try_to::<Gd<Node3D>>().ok();
        self.scene = Some(Scene {
            bones: (0..count)
                .map(|n| Some((skeletons.get(n)?.try_to::<Gd<Skeleton3D>>().ok()?, bones.get(n).filter(|b| *b >= 0)?)))
                .collect(),
            nodes: (0..count).map(|n| nodes.get(n).and_then(node)).collect(),
            renderers: (0..drawn).map(|r| renderers.get(r).and_then(|v| v.try_to::<Gd<MeshInstance3D>>().ok())).collect(),
            materials: materials.iter_shared().collect(),
            shapes: HashMap::new(),
            renderer_of: self.evaluator.renderer_nodes().iter().enumerate().map(|(r, n)| (*n, r)).collect(),
        });
        let s = self.scene.as_mut().expect("bound");
        for (r, slots) in self.slots.iter().enumerate() {
            let Some(m) = &mut s.renderers[r] else { continue };
            for (slot, material) in slots.iter().enumerate() {
                if let Some(mat) = s.materials.get(*material as usize).filter(|_| (slot as i32) < m.get_surface_override_material_count()) {
                    m.set_surface_override_material(slot as i32, mat);
                }
            }
        }
    }

    fn node(&self, node: i32) -> Option<usize> {
        let n = usize::try_from(node).ok().filter(|n| *n < self.evaluator.parents().len());
        if n.is_none() {
            godot_error!("avatar: no node {node}");
        }
        n
    }

    fn physbone(&self, physbone: i32) -> Option<usize> {
        let p = usize::try_from(physbone).ok().filter(|p| *p < self.physbones);
        if p.is_none() {
            godot_error!("avatar: no PhysBone {physbone}");
        }
        p
    }

    #[func]
    fn set_parameter(&mut self, name: GString, value: f32) {
        self.evaluator.set(&name.to_string(), value)
    }

    /// Whether `name` is a built-in the core knows.
    #[func]
    fn set_builtin(&mut self, name: GString, value: f32) -> bool {
        self.evaluator.set_builtin(&name.to_string(), value)
    }

    #[func]
    fn set_eyes(&mut self, blink: f32, look_y: f32) {
        self.evaluator.set_eyes(blink, look_y)
    }

    /// The head's animated pose in the avatar's root space.
    #[func]
    fn set_head(&mut self, head: Transform3D) {
        self.evaluator.set_head(from_godot(head))
    }

    /// Sets a node's pose in the avatar's root space for the next `evaluate`, after animation and before dynamics.
    #[func]
    fn pose(&mut self, node: i32, at: Transform3D) {
        let Some(n) = self.node(node) else { return };
        self.poses.push((n, from_godot(at)));
    }

    /// Evaluates a frame `dt` seconds after the last, the avatar root at `root` in the world, with the poses set
    /// since the last frame and the other avatars' `hands`.
    #[func]
    fn evaluate(&mut self, dt: f32, root: Transform3D, hands: VarArray) {
        let poses = std::mem::take(&mut self.poses);
        let others: Vec<HandCollider> = hands.iter_shared().filter_map(|h| h.try_to::<Gd<AvatarHands>>().ok()).flat_map(|h| h.bind().hands.clone()).collect();
        self.evaluator.evaluate(&Input { dt, root: from_godot(root), poses: &poses, held: &[], senders: &[], hands: &others });
    }

    /// This avatar's hand and finger colliders after the last frame, named `player` for the avatars around it.
    #[func]
    fn hands(&self, player: i64) -> Gd<AvatarHands> {
        Gd::from_object(AvatarHands { hands: self.evaluator.hands(player.cast_unsigned()) })
    }

    /// Writes the last frame's output to the bound scene.
    #[func]
    fn apply(&mut self) {
        let Some(s) = &mut self.scene else { return };
        let e = &self.evaluator;
        let out = e.output();
        for (n, t) in out.locals.iter().enumerate() {
            if let Some((sk, b)) = &mut s.bones[n] {
                let q = t.rotation;
                sk.set_bone_pose_position(*b, Vector3::new(t.translation.x, t.translation.y, t.translation.z));
                sk.set_bone_pose_rotation(*b, Quaternion::new(q.x, q.y, q.z, q.w));
                sk.set_bone_pose_scale(*b, Vector3::new(t.scale.x, t.scale.y, t.scale.z));
            } else if let Some(node) = &mut s.nodes[n] {
                node.set_transform(to_godot(t));
            }
        }
        let renderer = |node: &u32| s.renderer_of.get(node).copied();
        let mut server = RenderingServer::singleton();
        for (r, m) in s.renderers.iter().enumerate() {
            if let Some(m) = m {
                server.instance_set_visible(m.get_instance(), e.shown(r));
            }
        }
        for (node, shape, w) in &out.blendshapes {
            let Some(m) = renderer(node).and_then(|r| s.renderers[r].as_mut()) else { continue };
            let i = *s.shapes.entry((*node, shape.clone())).or_insert_with(|| m.find_blend_shape_by_name(shape.as_str()));
            if i >= 0 {
                m.set_blend_shape_value(i, w / 100.0);
            }
        }
        for (node, slot, material) in &out.slots {
            let Some(m) = renderer(node).and_then(|r| s.renderers[r].as_mut()) else { continue };
            if let Some(mat) = s.materials.get(*material as usize).filter(|_| (*slot as i32) < m.get_surface_override_material_count()) {
                m.set_surface_override_material(*slot as i32, mat);
            }
        }
    }

    /// Whether renderer `r`, in the model's order, draws after the last frame.
    #[func]
    fn shown(&self, r: i32) -> bool {
        usize::try_from(r).ok().filter(|r| *r < self.evaluator.renderer_nodes().len()).is_some_and(|r| self.evaluator.shown(r))
    }

    /// A node's pose in the avatar's root space after the last frame.
    #[func]
    fn space(&self, node: i32) -> Transform3D {
        self.node(node).map_or(Transform3D::IDENTITY, |n| to_godot(&self.evaluator.output().space[n]))
    }

    /// A node's local pose after the last frame.
    #[func]
    fn local(&self, node: i32) -> Transform3D {
        self.node(node).map_or(Transform3D::IDENTITY, |n| to_godot(&self.evaluator.output().locals[n]))
    }

    /// Every animator parameter's value.
    #[func]
    fn parameters(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        for p in self.evaluator.animator().parameters() {
            d.set(&GString::from(p.name.as_str()).to_variant(), &p.value.to_variant());
        }
        d
    }

    /// The saved expression parameters' values.
    #[func]
    fn saved(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        for (n, v) in self.evaluator.animator().saved() {
            d.set(&GString::from(n.as_str()).to_variant(), &v.to_variant());
        }
        d
    }

    /// Each layer's state as `playable:layer:name` → `current` or `current -> next`.
    #[func]
    fn states(&self) -> VarDictionary {
        let mut d = VarDictionary::new();
        for s in self.evaluator.animator().states() {
            let at = match &s.next {
                Some(n) => format!("{} -> {n}", s.current),
                None => s.current.clone(),
            };
            d.set(&GString::from(format!("{}:{}:{}", s.playable, s.layer, s.name).as_str()).to_variant(), &GString::from(at.as_str()).to_variant());
        }
        d
    }

    /// The material properties the animator binds after the last frame, each `[renderer node, property, value]`.
    #[func]
    fn materials(&self) -> VarArray {
        self.evaluator.output().materials.iter().map(|(n, p, v)| { let mut a = VarArray::new(); a.push(&(*n as i64).to_variant()); a.push(&GString::from(p.as_str()).to_variant()); a.push(&v.to_variant()); a }.to_variant()).collect()
    }

    /// The nodes each PhysBone moves, root first.
    #[func]
    fn physbone_bones(&self, physbone: i32) -> PackedInt32Array {
        self.physbone(physbone).map_or_else(PackedInt32Array::new, |p| self.evaluator.physbones().bones(p).map(|n| n as i32).collect())
    }

    /// Grabs `node` of `physbone` with a hand at `at` in the world; whether the grab filter lets it hold.
    #[func]
    fn grab(&mut self, physbone: i32, node: i32, at: Vector3, other: bool) -> bool {
        let (Some(p), Some(n)) = (self.physbone(physbone), self.node(node)) else { return false };
        self.evaluator.grab(p, n, Vec3::new(at.x, at.y, at.z), who(other))
    }

    #[func]
    fn hold(&mut self, physbone: i32, at: Vector3) {
        if let Some(p) = self.physbone(physbone) {
            self.evaluator.hold(p, Vec3::new(at.x, at.y, at.z))
        }
    }

    #[func]
    fn release(&mut self, physbone: i32, pose: bool, other: bool) {
        if let Some(p) = self.physbone(physbone) {
            self.evaluator.release(p, pose, who(other))
        }
    }
}
