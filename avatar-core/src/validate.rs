use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use serde::ser::{self, Serialize};
use serde_json::{Map, Value};

use crate::model::{
    AwAvatar, Behaviour, ControlledLayer, Destination, LayerContent, PlayableLayerType, StateId,
    StateMachine,
};

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

/// An index-typed value in the model, by the name of its type, at a JSON pointer into
/// `AW_avatar`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reference {
    pub pointer: String,
    pub kind: &'static str,
    pub index: u32,
}

/// Every index-typed value in the model, found from the types themselves.
pub fn references(model: &AwAvatar) -> Vec<Reference> {
    let mut found = Vec::new();
    visit_indices(model, &mut |kind, index, path| {
        found.push(Reference {
            pointer: pointer(path),
            kind,
            index,
        });
        Ok(())
    })
    .expect("collecting refuses nothing");
    found
}

fn visit_indices(model: &AwAvatar, visit: &mut Visit<'_>) -> Result<(), Invalid> {
    let mut collector = Collector {
        path: Vec::new(),
        visit,
        stop: None,
    };
    match model.serialize(&mut collector) {
        Ok(()) => Ok(()),
        Err(Stop) => Err(collector
            .stop
            .unwrap_or_else(|| invalid("", "the model does not serialize"))),
    }
}

fn pointer(path: &[String]) -> String {
    path.iter()
        .map(|s| format!("/{}", s.replace('~', "~0").replace('/', "~1")))
        .collect()
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

    let textures = gltf_array("textures").len();
    visit_indices(model, &mut |kind, index, path| {
        let (noun, count) = match kind {
            "NodeIndex" => ("node", model.nodes.len()),
            "MaterialIndex" => ("material", model.materials.len()),
            "TextureIndex" => ("glTF texture", textures),
            "ClipIndex" => ("clip", model.clips.len()),
            "BehaviourIndex" => ("behaviour", model.behaviours.len()),
            "ColliderIndex" => ("collider", model.colliders.len()),
            "PhysBoneIndex" => ("physbone", model.physbones.len()),
            "ConstraintIndex" => ("constraint", model.constraints.len()),
            "ContactSenderIndex" => ("contact sender", model.contacts.senders.len()),
            "ContactReceiverIndex" => ("contact receiver", model.contacts.receivers.len()),
            "StateId" | "StateMachineId" => return Ok(()),
            other => {
                return Err(invalid(
                    pointer(path),
                    format!("unchecked index type {other}"),
                ))
            }
        };
        if index as usize >= count {
            return Err(invalid(
                pointer(path),
                format!("{noun} {index}, but there are {count}"),
            ));
        }
        Ok(())
    })?;

    let gltf_meshes = gltf_array("meshes");
    let mut slots = BTreeMap::new();
    for (i, renderer) in model.renderers.iter().enumerate() {
        let at = format!("/renderers/{i}/node");
        let node = renderer.node.0;
        if slots.insert(node, renderer.materials.len()).is_some() {
            return Err(invalid(at, format!("node {node} has a second renderer")));
        }
        let primitives = gltf_nodes
            .get(node as usize)
            .and_then(|node| node.get("mesh"))
            .and_then(Value::as_u64)
            .and_then(|mesh| gltf_meshes.get(usize::try_from(mesh).ok()?))
            .and_then(|mesh| mesh.get("primitives"))
            .and_then(Value::as_array)
            .ok_or_else(|| invalid(&at, format!("glTF node {node} has no mesh with primitives")))?;
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

    for (i, behaviour) in model.behaviours.iter().enumerate() {
        let Behaviour::AnimatorLayerControl {
            playable, layer, ..
        } = behaviour
        else {
            continue;
        };
        let playable = match playable {
            ControlledLayer::Action => PlayableLayerType::Action,
            ControlledLayer::Fx => PlayableLayerType::Fx,
            ControlledLayer::Gesture => PlayableLayerType::Gesture,
            ControlledLayer::Additive => PlayableLayerType::Additive,
        };
        let Some(controller) = model
            .layers
            .get(&playable)
            .and_then(|l| l.controller.as_ref())
        else {
            continue;
        };
        if *layer as usize >= controller.layers.len() {
            return Err(invalid(
                format!("/behaviours/{i}/layer"),
                format!(
                    "layer {layer}, but that controller has {}",
                    controller.layers.len()
                ),
            ));
        }
    }

    validate_controllers(model)
}

/// The states and state machines one animator layer declares.
#[derive(Default)]
struct LayerIds {
    states: BTreeSet<u32>,
    machines: BTreeSet<u32>,
}

/// Ids are unique in their controller; a transition, default state or synced state reaches
/// only what its own animator layer declares.
fn validate_controllers(model: &AwAvatar) -> Result<(), Invalid> {
    for (kind, playable) in &model.layers {
        let Some(controller) = &playable.controller else {
            continue;
        };
        let key = serde_json::to_value(kind).map_err(|e| invalid("/layers", e.to_string()))?;
        let base = format!(
            "/layers/{}/controller/layers",
            key.as_str().unwrap_or_default()
        );
        let mut unique = LayerIds::default();
        let mut layers = Vec::new();
        for (i, layer) in controller.layers.iter().enumerate() {
            let mut ids = LayerIds::default();
            if let LayerContent::StateMachine { state_machine } = &layer.content {
                let at = format!("{base}/{i}/content/stateMachine");
                declare(state_machine, &at, &mut unique, &mut ids)?;
            }
            layers.push(ids);
        }
        for (i, layer) in controller.layers.iter().enumerate() {
            match &layer.content {
                LayerContent::StateMachine { state_machine } => {
                    let at = format!("{base}/{i}/content/stateMachine");
                    reach(state_machine, &at, &layers[i])?;
                }
                LayerContent::Synced {
                    layer: source,
                    states,
                    ..
                } => {
                    let at = format!("{base}/{i}/content");
                    let source_machine = controller
                        .layers
                        .get(*source as usize)
                        .filter(|_| *source as usize != i)
                        .is_some_and(|l| matches!(l.content, LayerContent::StateMachine { .. }));
                    if !source_machine {
                        return Err(invalid(
                            format!("{at}/layer"),
                            format!("layer {source} is not another layer with a state machine"),
                        ));
                    }
                    for (k, synced) in states.iter().enumerate() {
                        if !layers[*source as usize].states.contains(&synced.state.0) {
                            return Err(invalid(
                                format!("{at}/states/{k}/state"),
                                format!("no state {} in layer {source}", synced.state.0),
                            ));
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn declare(
    machine: &StateMachine,
    at: &str,
    unique: &mut LayerIds,
    ids: &mut LayerIds,
) -> Result<(), Invalid> {
    if !unique.machines.insert(machine.id.0) {
        return Err(invalid(
            format!("{at}/id"),
            format!("a second state machine {} in this controller", machine.id.0),
        ));
    }
    ids.machines.insert(machine.id.0);
    for (s, state) in machine.states.iter().enumerate() {
        if !unique.states.insert(state.id.0) {
            return Err(invalid(
                format!("{at}/states/{s}/id"),
                format!("a second state {} in this controller", state.id.0),
            ));
        }
        ids.states.insert(state.id.0);
    }
    for (k, sub) in machine.state_machines.iter().enumerate() {
        declare(sub, &format!("{at}/stateMachines/{k}"), unique, ids)?;
    }
    Ok(())
}

fn holds(machine: &StateMachine, state: StateId) -> bool {
    machine.states.iter().any(|s| s.id == state)
        || machine.state_machines.iter().any(|sub| holds(sub, state))
}

fn reach(machine: &StateMachine, at: &str, ids: &LayerIds) -> Result<(), Invalid> {
    if let Some(default) = machine.default_state {
        if !holds(machine, default) {
            return Err(invalid(
                format!("{at}/defaultState"),
                format!("state {} is not in this machine", default.0),
            ));
        }
    }
    let destination = |destination: &Destination, at: String| match destination {
        Destination::State { id } if !ids.states.contains(&id.0) => Err(invalid(
            format!("{at}/destination/id"),
            format!("no state {} in this layer", id.0),
        )),
        Destination::StateMachine { id } if !ids.machines.contains(&id.0) => Err(invalid(
            format!("{at}/destination/id"),
            format!("no state machine {} in this layer", id.0),
        )),
        _ => Ok(()),
    };
    for (s, state) in machine.states.iter().enumerate() {
        for (t, transition) in state.transitions.iter().enumerate() {
            destination(
                &transition.destination,
                format!("{at}/states/{s}/transitions/{t}"),
            )?;
        }
    }
    for (t, transition) in machine.any_state_transitions.iter().enumerate() {
        destination(
            &transition.destination,
            format!("{at}/anyStateTransitions/{t}"),
        )?;
    }
    for (t, transition) in machine.entry_transitions.iter().enumerate() {
        destination(
            &transition.destination,
            format!("{at}/entryTransitions/{t}"),
        )?;
    }
    for (t, transition) in machine.state_machine_transitions.iter().enumerate() {
        destination(
            &transition.destination,
            format!("{at}/stateMachineTransitions/{t}"),
        )?;
    }
    for (k, sub) in machine.state_machines.iter().enumerate() {
        reach(sub, &format!("{at}/stateMachines/{k}"), ids)?;
    }
    Ok(())
}

type Visit<'v> = dyn FnMut(&'static str, u32, &[String]) -> Result<(), Invalid> + 'v;

struct Collector<'v> {
    path: Vec<String>,
    visit: &'v mut Visit<'v>,
    stop: Option<Invalid>,
}

impl Collector<'_> {
    fn within<T: Serialize + ?Sized>(&mut self, segment: String, value: &T) -> Result<(), Stop> {
        self.path.push(segment);
        let result = value.serialize(&mut *self);
        self.path.pop();
        result
    }
}

#[derive(Debug)]
struct Stop;

impl fmt::Display for Stop {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("validation stopped")
    }
}

impl std::error::Error for Stop {}

impl ser::Error for Stop {
    fn custom<T: fmt::Display>(_: T) -> Self {
        Stop
    }
}

struct Compound<'a, 'v> {
    collector: &'a mut Collector<'v>,
    next: usize,
    key: String,
    variant: bool,
}

impl<'a, 'v> Compound<'a, 'v> {
    fn new(collector: &'a mut Collector<'v>, variant: Option<&str>) -> Self {
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

    fn element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Stop> {
        self.next += 1;
        self.collector.within((self.next - 1).to_string(), value)
    }

    fn end(self) -> Result<(), Stop> {
        if self.variant {
            self.collector.path.pop();
        }
        Ok(())
    }
}

impl ser::SerializeSeq for Compound<'_, '_> {
    type Ok = ();
    type Error = Stop;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Stop> {
        self.element(value)
    }
    fn end(self) -> Result<(), Stop> {
        Compound::end(self)
    }
}

impl ser::SerializeTuple for Compound<'_, '_> {
    type Ok = ();
    type Error = Stop;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Stop> {
        self.element(value)
    }
    fn end(self) -> Result<(), Stop> {
        Compound::end(self)
    }
}

impl ser::SerializeTupleStruct for Compound<'_, '_> {
    type Ok = ();
    type Error = Stop;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Stop> {
        self.element(value)
    }
    fn end(self) -> Result<(), Stop> {
        Compound::end(self)
    }
}

impl ser::SerializeTupleVariant for Compound<'_, '_> {
    type Ok = ();
    type Error = Stop;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Stop> {
        self.element(value)
    }
    fn end(self) -> Result<(), Stop> {
        Compound::end(self)
    }
}

impl Compound<'_, '_> {
    fn field<T: Serialize + ?Sized>(&mut self, key: &str, value: &T) -> Result<(), Stop> {
        self.collector.within(key.to_owned(), value)
    }
}

impl ser::SerializeStruct for Compound<'_, '_> {
    type Ok = ();
    type Error = Stop;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Stop> {
        self.field(key, value)
    }
    fn end(self) -> Result<(), Stop> {
        Compound::end(self)
    }
}

impl ser::SerializeStructVariant for Compound<'_, '_> {
    type Ok = ();
    type Error = Stop;
    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Stop> {
        self.field(key, value)
    }
    fn end(self) -> Result<(), Stop> {
        Compound::end(self)
    }
}

impl ser::SerializeMap for Compound<'_, '_> {
    type Ok = ();
    type Error = Stop;
    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), Stop> {
        self.key = match serde_json::to_value(key).map_err(|_| Stop)? {
            Value::String(key) => key,
            other => other.to_string(),
        };
        Ok(())
    }
    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Stop> {
        let key = std::mem::take(&mut self.key);
        self.collector.within(key, value)
    }
    fn end(self) -> Result<(), Stop> {
        Compound::end(self)
    }
}

impl<'a, 'v> ser::Serializer for &'a mut Collector<'v> {
    type Ok = ();
    type Error = Stop;
    type SerializeSeq = Compound<'a, 'v>;
    type SerializeTuple = Compound<'a, 'v>;
    type SerializeTupleStruct = Compound<'a, 'v>;
    type SerializeTupleVariant = Compound<'a, 'v>;
    type SerializeMap = Compound<'a, 'v>;
    type SerializeStruct = Compound<'a, 'v>;
    type SerializeStructVariant = Compound<'a, 'v>;

    fn serialize_bool(self, _: bool) -> Result<(), Stop> {
        Ok(())
    }
    fn serialize_i8(self, _: i8) -> Result<(), Stop> {
        Ok(())
    }
    fn serialize_i16(self, _: i16) -> Result<(), Stop> {
        Ok(())
    }
    fn serialize_i32(self, _: i32) -> Result<(), Stop> {
        Ok(())
    }
    fn serialize_i64(self, _: i64) -> Result<(), Stop> {
        Ok(())
    }
    fn serialize_u8(self, _: u8) -> Result<(), Stop> {
        Ok(())
    }
    fn serialize_u16(self, _: u16) -> Result<(), Stop> {
        Ok(())
    }
    fn serialize_u32(self, _: u32) -> Result<(), Stop> {
        Ok(())
    }
    fn serialize_u64(self, _: u64) -> Result<(), Stop> {
        Ok(())
    }
    fn serialize_f32(self, _: f32) -> Result<(), Stop> {
        Ok(())
    }
    fn serialize_f64(self, _: f64) -> Result<(), Stop> {
        Ok(())
    }
    fn serialize_char(self, _: char) -> Result<(), Stop> {
        Ok(())
    }
    fn serialize_str(self, _: &str) -> Result<(), Stop> {
        Ok(())
    }
    fn serialize_bytes(self, _: &[u8]) -> Result<(), Stop> {
        Ok(())
    }
    fn serialize_none(self) -> Result<(), Stop> {
        Ok(())
    }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<(), Stop> {
        value.serialize(self)
    }
    fn serialize_unit(self) -> Result<(), Stop> {
        Ok(())
    }
    fn serialize_unit_struct(self, _: &'static str) -> Result<(), Stop> {
        Ok(())
    }
    fn serialize_unit_variant(self, _: &'static str, _: u32, _: &'static str) -> Result<(), Stop> {
        Ok(())
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<(), Stop> {
        let Some(index) = serde_json::to_value(value)
            .ok()
            .as_ref()
            .and_then(Value::as_u64)
            .and_then(|i| u32::try_from(i).ok())
        else {
            return value.serialize(self);
        };
        (self.visit)(name, index, &self.path).map_err(|e| {
            self.stop = Some(e);
            Stop
        })
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<(), Stop> {
        self.within(variant.to_owned(), value)
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Compound<'a, 'v>, Stop> {
        Ok(Compound::new(self, None))
    }
    fn serialize_tuple(self, _: usize) -> Result<Compound<'a, 'v>, Stop> {
        Ok(Compound::new(self, None))
    }
    fn serialize_tuple_struct(self, _: &'static str, _: usize) -> Result<Compound<'a, 'v>, Stop> {
        Ok(Compound::new(self, None))
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Compound<'a, 'v>, Stop> {
        Ok(Compound::new(self, Some(variant)))
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Compound<'a, 'v>, Stop> {
        Ok(Compound::new(self, None))
    }
    fn serialize_struct(self, _: &'static str, _: usize) -> Result<Compound<'a, 'v>, Stop> {
        Ok(Compound::new(self, None))
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Compound<'a, 'v>, Stop> {
        Ok(Compound::new(self, Some(variant)))
    }
}
