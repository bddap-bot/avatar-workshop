use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::ser::{self, Serialize};
use serde_json::{Map, Value};

use crate::model::{AwAvatar, LayerContent};

/// A model that parsed but does not hold together: `at` is a JSON pointer into `AW_avatar`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invalid {
    pub at: String,
    pub reason: String,
}

fn invalid(at: impl Into<String>, reason: impl Into<String>) -> Invalid {
    Invalid {
        at: at.into(),
        reason: reason.into(),
    }
}

/// An index the model holds, by the name of its type, at a JSON pointer into `AW_avatar`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reference {
    pub pointer: String,
    pub kind: &'static str,
    pub index: u32,
}

/// Every index-typed value in the model, found from the types themselves, so a field added
/// later is checked without a validator change.
pub fn references(model: &AwAvatar) -> Vec<Reference> {
    let mut collector = Collector::default();
    model
        .serialize(&mut collector)
        .expect("the collector accepts every model");
    collector.found
}

/// Whether the model refers only to what exists: in the glTF document beside it and in itself.
/// It says nothing about whether an avatar is affordable; that is the host's quality policy.
pub fn validate(gltf: &Map<String, Value>, model: &AwAvatar) -> Result<(), Invalid> {
    let gltf_array = |key: &str| {
        gltf.get(key)
            .and_then(Value::as_array)
            .map_or(&[][..], Vec::as_slice)
    };
    let gltf_nodes = gltf_array("nodes");
    for (section, entries, gltf_count) in [
        ("nodes", model.nodes.len(), gltf_nodes.len()),
        (
            "materials",
            model.materials.len(),
            gltf_array("materials").len(),
        ),
    ] {
        if entries != gltf_count {
            return Err(invalid(
                format!("/{section}"),
                format!("{entries} entries for {gltf_count} glTF {section}"),
            ));
        }
    }

    let references = references(model);
    let counts: BTreeMap<&str, (&str, usize)> = [
        ("NodeIndex", ("node", model.nodes.len())),
        ("MaterialIndex", ("material", model.materials.len())),
        (
            "TextureIndex",
            ("glTF texture", gltf_array("textures").len()),
        ),
        ("ClipIndex", ("clip", model.clips.len())),
        ("BehaviourIndex", ("behaviour", model.behaviours.len())),
        ("ColliderIndex", ("collider", model.colliders.len())),
        ("PhysBoneIndex", ("physbone", model.physbones.len())),
        ("ConstraintIndex", ("constraint", model.constraints.len())),
        (
            "ContactSenderIndex",
            ("contact sender", model.contacts.senders.len()),
        ),
        (
            "ContactReceiverIndex",
            ("contact receiver", model.contacts.receivers.len()),
        ),
    ]
    .into();
    for r in &references {
        if let Some(&(noun, count)) = counts.get(r.kind) {
            if r.index as usize >= count {
                return Err(invalid(
                    &r.pointer,
                    format!("{noun} {}, but there are {count}", r.index),
                ));
            }
        }
    }

    let gltf_meshes = gltf_array("meshes");
    let mut slots = BTreeMap::new();
    for (i, renderer) in model.renderers.iter().enumerate() {
        let at = format!("/renderers/{i}/node");
        let node = renderer.node.0;
        if slots.insert(node, renderer.materials.len()).is_some() {
            return Err(invalid(at, format!("node {node} has a second renderer")));
        }
        let primitives = gltf_nodes[node as usize]
            .get("mesh")
            .and_then(Value::as_u64)
            .and_then(|mesh| gltf_meshes.get(mesh as usize))
            .and_then(|mesh| mesh.get("primitives"))
            .and_then(Value::as_array)
            .ok_or_else(|| invalid(&at, format!("glTF node {node} has no mesh")))?;
        if primitives.len() != renderer.materials.len() {
            return Err(invalid(
                format!("/renderers/{i}/materials"),
                format!(
                    "{} slots for a mesh of {} primitives",
                    renderer.materials.len(),
                    primitives.len()
                ),
            ));
        }
    }
    for (c, clip) in model.clips.iter().enumerate() {
        for (s, curve) in clip.slot_curves.iter().enumerate() {
            let at = format!("/clips/{c}/slotCurves/{s}");
            let Some(&count) = slots.get(&curve.renderer.0) else {
                return Err(invalid(
                    format!("{at}/renderer"),
                    format!("node {} has no renderer", curve.renderer.0),
                ));
            };
            if curve.slot as usize >= count {
                return Err(invalid(
                    format!("{at}/slot"),
                    format!("slot {}, but the renderer has {count}", curve.slot),
                ));
            }
        }
    }

    let mut names = BTreeSet::new();
    for (i, parameter) in model.parameters.iter().enumerate() {
        if !names.insert(&parameter.name) {
            return Err(invalid(
                format!("/parameters/{i}/name"),
                format!("a second parameter named {:?}", parameter.name),
            ));
        }
    }

    validate_controllers(model, &references)
}

/// State and state-machine ids are scoped to their controller; a `.../id` field declares one
/// unless it sits in a transition's destination.
fn validate_controllers(model: &AwAvatar, references: &[Reference]) -> Result<(), Invalid> {
    type Ids<'a> = BTreeMap<(&'a str, &'static str), BTreeSet<u32>>;
    let controller_of = |pointer: &str| {
        let mut segments = pointer.splitn(4, '/');
        segments.next();
        match (segments.next(), segments.next()) {
            (Some("layers"), Some(layer)) => layer.len() + "/layers/".len(),
            _ => 0,
        }
    };
    let scoped = references
        .iter()
        .filter(|r| matches!(r.kind, "StateId" | "StateMachineId"));
    let mut declared = Ids::new();
    let mut used = Vec::new();
    for r in scoped {
        let controller = &r.pointer[..controller_of(&r.pointer)];
        if r.pointer.ends_with("/id") && !r.pointer.ends_with("/destination/id") {
            if !declared
                .entry((controller, r.kind))
                .or_default()
                .insert(r.index)
            {
                return Err(invalid(
                    &r.pointer,
                    format!("a second {} {} in this controller", noun(r.kind), r.index),
                ));
            }
        } else {
            used.push((controller, r));
        }
    }
    for (controller, r) in used {
        if !declared
            .get(&(controller, r.kind))
            .is_some_and(|ids| ids.contains(&r.index))
        {
            return Err(invalid(
                &r.pointer,
                format!("no {} {} in this controller", noun(r.kind), r.index),
            ));
        }
    }

    for (kind, playable) in &model.layers {
        let Some(controller) = &playable.controller else {
            continue;
        };
        let key = serde_json::to_value(kind).expect("a layer type serializes");
        let key = key.as_str().expect("a layer type is a string");
        for (i, layer) in controller.layers.iter().enumerate() {
            let LayerContent::Synced { layer: source, .. } = layer.content else {
                continue;
            };
            let at = format!("/layers/{key}/controller/layers/{i}/content/layer");
            let target = controller.layers.get(source as usize).map(|l| &l.content);
            if source as usize == i || !matches!(target, Some(LayerContent::StateMachine { .. })) {
                return Err(invalid(
                    at,
                    format!("layer {source} is not another layer with a state machine"),
                ));
            }
        }
    }
    Ok(())
}

fn noun(kind: &str) -> &str {
    match kind {
        "StateId" => "state",
        _ => "state machine",
    }
}

#[derive(Default)]
struct Collector {
    path: Vec<String>,
    found: Vec<Reference>,
}

impl Collector {
    fn pointer(&self) -> String {
        self.path
            .iter()
            .map(|s| format!("/{}", s.replace('~', "~0").replace('/', "~1")))
            .collect()
    }

    fn within<T: Serialize + ?Sized>(&mut self, segment: String, value: &T) -> Result<(), Never> {
        self.path.push(segment);
        let result = value.serialize(&mut *self);
        self.path.pop();
        result
    }
}

#[derive(Debug)]
struct Never;

impl fmt::Display for Never {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("unreachable")
    }
}

impl std::error::Error for Never {}

impl ser::Error for Never {
    fn custom<T: fmt::Display>(_: T) -> Self {
        Never
    }
}

struct Compound<'a> {
    collector: &'a mut Collector,
    next: usize,
    key: String,
    variant: bool,
}

impl<'a> Compound<'a> {
    fn new(collector: &'a mut Collector, variant: Option<&str>) -> Self {
        if let Some(variant) = variant {
            collector.path.push(variant.to_owned());
        }
        Compound {
            collector,
            next: 0,
            key: String::new(),
            variant: variant.is_some(),
        }
    }

    fn element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Never> {
        self.next += 1;
        self.collector.within((self.next - 1).to_string(), value)
    }

    fn end(self) -> Result<(), Never> {
        if self.variant {
            self.collector.path.pop();
        }
        Ok(())
    }
}

impl ser::SerializeSeq for Compound<'_> {
    type Ok = ();
    type Error = Never;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Never> {
        self.element(value)
    }
    fn end(self) -> Result<(), Never> {
        Compound::end(self)
    }
}

impl ser::SerializeTuple for Compound<'_> {
    type Ok = ();
    type Error = Never;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Never> {
        self.element(value)
    }
    fn end(self) -> Result<(), Never> {
        Compound::end(self)
    }
}

impl ser::SerializeTupleStruct for Compound<'_> {
    type Ok = ();
    type Error = Never;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Never> {
        self.element(value)
    }
    fn end(self) -> Result<(), Never> {
        Compound::end(self)
    }
}

impl ser::SerializeTupleVariant for Compound<'_> {
    type Ok = ();
    type Error = Never;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Never> {
        self.element(value)
    }
    fn end(self) -> Result<(), Never> {
        Compound::end(self)
    }
}

impl Compound<'_> {
    fn field<T: Serialize + ?Sized>(&mut self, key: &str, value: &T) -> Result<(), Never> {
        self.collector.within(key.to_owned(), value)
    }
}

impl ser::SerializeStruct for Compound<'_> {
    type Ok = ();
    type Error = Never;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Never> {
        self.field(key, value)
    }
    fn end(self) -> Result<(), Never> {
        Compound::end(self)
    }
}

impl ser::SerializeStructVariant for Compound<'_> {
    type Ok = ();
    type Error = Never;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Never> {
        self.field(key, value)
    }
    fn end(self) -> Result<(), Never> {
        Compound::end(self)
    }
}

impl ser::SerializeMap for Compound<'_> {
    type Ok = ();
    type Error = Never;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), Never> {
        self.key = match serde_json::to_value(key).map_err(|_| Never)? {
            Value::String(key) => key,
            other => other.to_string(),
        };
        Ok(())
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Never> {
        let key = std::mem::take(&mut self.key);
        self.collector.within(key, value)
    }
    fn end(self) -> Result<(), Never> {
        Compound::end(self)
    }
}

impl<'a> ser::Serializer for &'a mut Collector {
    type Ok = ();
    type Error = Never;
    type SerializeSeq = Compound<'a>;
    type SerializeTuple = Compound<'a>;
    type SerializeTupleStruct = Compound<'a>;
    type SerializeTupleVariant = Compound<'a>;
    type SerializeMap = Compound<'a>;
    type SerializeStruct = Compound<'a>;
    type SerializeStructVariant = Compound<'a>;

    fn serialize_bool(self, _: bool) -> Result<(), Never> {
        Ok(())
    }
    fn serialize_i8(self, _: i8) -> Result<(), Never> {
        Ok(())
    }
    fn serialize_i16(self, _: i16) -> Result<(), Never> {
        Ok(())
    }
    fn serialize_i32(self, _: i32) -> Result<(), Never> {
        Ok(())
    }
    fn serialize_i64(self, _: i64) -> Result<(), Never> {
        Ok(())
    }
    fn serialize_u8(self, _: u8) -> Result<(), Never> {
        Ok(())
    }
    fn serialize_u16(self, _: u16) -> Result<(), Never> {
        Ok(())
    }
    fn serialize_u32(self, _: u32) -> Result<(), Never> {
        Ok(())
    }
    fn serialize_u64(self, _: u64) -> Result<(), Never> {
        Ok(())
    }
    fn serialize_f32(self, _: f32) -> Result<(), Never> {
        Ok(())
    }
    fn serialize_f64(self, _: f64) -> Result<(), Never> {
        Ok(())
    }
    fn serialize_char(self, _: char) -> Result<(), Never> {
        Ok(())
    }
    fn serialize_str(self, _: &str) -> Result<(), Never> {
        Ok(())
    }
    fn serialize_bytes(self, _: &[u8]) -> Result<(), Never> {
        Ok(())
    }
    fn serialize_none(self) -> Result<(), Never> {
        Ok(())
    }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<(), Never> {
        value.serialize(self)
    }
    fn serialize_unit(self) -> Result<(), Never> {
        Ok(())
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<(), Never> {
        Ok(())
    }
    fn serialize_unit_variant(self, _: &'static str, _: u32, _: &'static str) -> Result<(), Never> {
        Ok(())
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<(), Never> {
        if let Some(index) = serde_json::to_value(value)
            .ok()
            .as_ref()
            .and_then(Value::as_u64)
            .and_then(|i| u32::try_from(i).ok())
        {
            self.found.push(Reference {
                pointer: self.pointer(),
                kind: name,
                index,
            });
        }
        Ok(())
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<(), Never> {
        self.within(variant.to_owned(), value)
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Compound<'a>, Never> {
        Ok(Compound::new(self, None))
    }
    fn serialize_tuple(self, _: usize) -> Result<Compound<'a>, Never> {
        Ok(Compound::new(self, None))
    }
    fn serialize_tuple_struct(self, _: &'static str, _: usize) -> Result<Compound<'a>, Never> {
        Ok(Compound::new(self, None))
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Compound<'a>, Never> {
        Ok(Compound::new(self, Some(variant)))
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Compound<'a>, Never> {
        Ok(Compound::new(self, None))
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Compound<'a>, Never> {
        Ok(Compound::new(self, None))
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Compound<'a>, Never> {
        Ok(Compound::new(self, Some(variant)))
    }
}
