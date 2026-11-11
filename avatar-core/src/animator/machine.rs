use std::collections::HashMap;

use crate::model::{
    AnimatorLayer, AnimatorParameterValue, BehaviourIndex, Condition, ConditionMode, Controller, Destination,
    Duration, EntryTransition, InterruptionSource, LayerContent, Motion, State, StateId, StateMachine,
    StateMachineId, Transition,
};

use super::motion::{leaves, CompiledClip, Leaf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Float,
    Int,
    Bool,
    Trigger,
}

/// A controller's own parameter, as Unity's Animator holds it.
#[derive(Clone, Debug)]
pub struct Param {
    pub name: String,
    pub kind: Kind,
    pub value: f32,
    pub default: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Owner {
    Any(usize),
    State(StateId, usize),
}

struct Flat {
    states: HashMap<StateId, State>,
    /// Each state's machine path from the layer's root machine.
    paths: HashMap<StateId, Vec<StateMachineId>>,
    machines: HashMap<StateMachineId, StateMachine>,
    parent: HashMap<StateMachineId, StateMachineId>,
    root: StateMachineId,
    any: Vec<Transition>,
}

impl Flat {
    fn new(root: &StateMachine) -> Flat {
        let mut f = Flat {
            states: HashMap::new(),
            paths: HashMap::new(),
            machines: HashMap::new(),
            parent: HashMap::new(),
            root: root.id,
            any: Vec::new(),
        };
        fn walk(f: &mut Flat, m: &StateMachine, path: &mut Vec<StateMachineId>) {
            path.push(m.id);
            for s in &m.states {
                f.states.insert(s.id, s.clone());
                f.paths.insert(s.id, path.clone());
            }
            f.any.extend(m.any_state_transitions.iter().cloned());
            let mut shallow = m.clone();
            shallow.state_machines.clear();
            shallow.states.clear();
            f.machines.insert(m.id, shallow);
            for c in &m.state_machines {
                f.parent.insert(c.id, m.id);
                walk(f, c, path);
            }
            path.pop();
        }
        walk(&mut f, root, &mut Vec::new());
        f
    }
}

#[derive(Clone, Debug)]
pub struct Playing {
    pub state: StateId,
    pub time: f32,
    /// Looping direct-tree leaves playing now and the state time each began at: a leaf whose weight rises from zero
    /// plays its clip from the start.
    pub direct_starts: Vec<(u32, f32)>,
    /// The state time at the last update's end, from which a leaf whose weight rises this update begins.
    prev_time: f32,
}

#[derive(Clone, Debug)]
pub enum From {
    State(Playing),
    /// The blended pose of an interrupted transition, held still.
    Frozen(Vec<(u32, f32, f32)>),
}

#[derive(Clone, Debug)]
pub struct Active {
    pub from: From,
    pub to: Playing,
    /// The transition's normalized progress, accumulated per update in single precision as Unity does.
    pub progress: f32,
    pub duration: f32,
    owner: Owner,
    source_state: StateId,
    interruption: InterruptionSource,
    ordered: bool,
}

impl Active {
    pub fn progress(&self) -> f32 {
        if self.duration <= 0.0 {
            1.0
        } else {
            self.progress.clamp(0.0, 1.0)
        }
    }
}

pub struct LayerRt {
    flat: Option<Flat>,
    /// For a synced layer: the layer whose machine it follows, and its own motions and behaviours per state.
    synced: Option<(usize, HashMap<StateId, (Option<Motion>, Vec<BehaviourIndex>)>)>,
    pub current: Option<Playing>,
    pub active: Option<Active>,
    pub weight: f32,
    pub started: bool,
    pub snapped: Option<Playing>,
}

/// A state entered this update, with the behaviours that run on entry.
pub struct Entered {
    pub layer: usize,
    pub state: StateId,
    pub behaviours: Vec<BehaviourIndex>,
}

pub struct ControllerRt {
    pub params: Vec<Param>,
    pub index: HashMap<String, usize>,
    pub layers: Vec<LayerRt>,
    pub layer_defs: Vec<AnimatorLayer>,
    /// Parameters an animator curve of this controller drives.
    pub curve_controlled: Vec<bool>,
    pub names: HashMap<StateId, String>,
}

impl ControllerRt {
    pub fn new(controller: &Controller, clips: &[CompiledClip], anim_params: &dyn Fn(u32) -> Vec<String>) -> ControllerRt {
        let params: Vec<Param> = controller
            .parameters
            .iter()
            .map(|p| {
                let (kind, value) = match p.default {
                    AnimatorParameterValue::Float(v) => (Kind::Float, v),
                    AnimatorParameterValue::Int(v) => (Kind::Int, v as f32),
                    AnimatorParameterValue::Bool(v) => (Kind::Bool, v as u8 as f32),
                    AnimatorParameterValue::Trigger(v) => (Kind::Trigger, v as u8 as f32),
                };
                Param { name: p.name.clone(), kind, value, default: value }
            })
            .collect();
        let mut index = HashMap::new();
        for (i, p) in params.iter().enumerate() {
            index.entry(p.name.clone()).or_insert(i);
        }
        let mut names = HashMap::new();
        let layers: Vec<LayerRt> = controller
            .layers
            .iter()
            .map(|l| match &l.content {
                LayerContent::StateMachine { state_machine } => {
                    let flat = Flat::new(state_machine);
                    for (id, s) in &flat.states {
                        let mut name = state_machine.name.clone();
                        for m in &flat.paths[id][1..] {
                            name.push('.');
                            name.push_str(&flat.machines[m].name);
                        }
                        name.push('.');
                        name.push_str(&s.name);
                        names.insert(*id, name);
                    }
                    LayerRt { flat: Some(flat), synced: None, current: None, active: None, weight: l.default_weight, started: false, snapped: None }
                }
                LayerContent::Synced { layer, states, .. } => {
                    let map = states.iter().map(|s| (s.state, (s.motion.clone(), s.behaviours.clone()))).collect();
                    LayerRt {
                        flat: None,
                        synced: Some((*layer as usize, map)),
                        current: None,
                        active: None,
                        weight: l.default_weight,
                        started: false,
                        snapped: None,
                    }
                }
            })
            .collect();
        let mut curve_controlled = vec![false; params.len()];
        let mut seen = vec![false; clips.len()];
        for l in &controller.layers {
            let mut motions = Vec::new();
            match &l.content {
                LayerContent::StateMachine { state_machine } => collect_motions(state_machine, &mut motions),
                LayerContent::Synced { states, .. } => motions.extend(states.iter().filter_map(|s| s.motion.clone())),
            }
            let mut ids = Vec::new();
            for m in &motions {
                clip_ids(m, &mut ids);
            }
            for c in ids {
                if std::mem::replace(&mut seen[c as usize], true) {
                    continue;
                }
                for name in anim_params(c) {
                    if let Some(&i) = index.get(&name) {
                        curve_controlled[i] = true;
                    }
                }
            }
        }
        ControllerRt { params, index, layers, layer_defs: controller.layers.clone(), curve_controlled, names }
    }

    pub fn clip_ids(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = (0..self.layer_defs.len()).flat_map(|l| self.layer_clip_ids(l)).collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    /// The clips one layer's states play.
    pub fn layer_clip_ids(&self, layer: usize) -> Vec<u32> {
        let mut ids = Vec::new();
        for l in &self.layer_defs[layer..=layer] {
            let mut motions = Vec::new();
            match &l.content {
                LayerContent::StateMachine { state_machine } => collect_motions(state_machine, &mut motions),
                LayerContent::Synced { states, .. } => motions.extend(states.iter().filter_map(|s| s.motion.clone())),
            }
            for m in &motions {
                clip_ids(m, &mut ids);
            }
        }
        ids.sort_unstable();
        ids.dedup();
        ids
    }

    pub fn get(&self, name: &str) -> f32 {
        self.index.get(name).map_or(0.0, |&i| self.params[i].value)
    }

    fn holds(&self, c: &Condition) -> bool {
        let Some(&i) = self.index.get(&c.parameter) else { return false };
        let p = &self.params[i];
        let v = p.value;
        match c.mode {
            ConditionMode::If => v != 0.0,
            ConditionMode::IfNot => v == 0.0,
            ConditionMode::Greater => v > c.threshold,
            ConditionMode::Less => v < c.threshold,
            ConditionMode::Equals => v == c.threshold,
            ConditionMode::NotEqual => v != c.threshold,
        }
    }

    fn all_hold(&self, conds: &[Condition]) -> bool {
        conds.iter().all(|c| self.holds(c))
    }

    fn consume_triggers(&mut self, conds: &[Condition]) {
        for c in conds {
            if let Some(&i) = self.index.get(&c.parameter) {
                if self.params[i].kind == Kind::Trigger {
                    self.params[i].value = 0.0;
                }
            }
        }
    }

    /// A state's motion as this layer plays it, a synced layer's own motion replacing its source's.
    pub fn motion(&self, layer: usize, state: StateId) -> Option<Motion> {
        let l = &self.layers[layer];
        if let Some((_, map)) = &l.synced {
            return map.get(&state).and_then(|m| m.0.clone());
        }
        l.flat.as_ref()?.states.get(&state)?.motion.clone()
    }

    pub fn state(&self, layer: usize, id: StateId) -> Option<&State> {
        let l = &self.layers[layer];
        let src = match &l.synced {
            Some((s, _)) => *s,
            None => layer,
        };
        self.layers[src].flat.as_ref()?.states.get(&id)
    }

    /// The leaves of a state's motion with the state's own time settings applied.
    pub fn state_leaves(&self, layer: usize, id: StateId, out: &mut Vec<Leaf>) {
        let Some(m) = self.motion(layer, id) else { return };
        let mirror = self.state(layer, id).is_some_and(|s| s.mirror_parameter.as_ref().map_or(s.mirror, |n| self.get(n) != 0.0));
        let get = |n: &str| self.get(n);
        leaves(&m, &get, Leaf { clip: 0, weight: 1.0, time_scale: 1.0, cycle_offset: 0.0, direct: false, id: 0, mirror }, out);
    }

    /// Seconds of the state's motion at speed 1.
    pub fn duration(&self, layer: usize, id: StateId, clips: &[CompiledClip]) -> f32 {
        let mut ls = Vec::new();
        self.state_leaves(layer, id, &mut ls);
        let mut d = 0.0;
        for l in &ls {
            if l.time_scale != 0.0 {
                d += l.weight * clips[l.clip as usize].length / l.time_scale.abs();
            }
        }
        if d > 0.0 {
            d
        } else {
            1.0
        }
    }

    fn rate(&self, layer: usize, id: StateId, clips: &[CompiledClip]) -> f32 {
        let Some(s) = self.state(layer, id) else { return 0.0 };
        let mut speed = s.speed;
        if let Some(p) = &s.speed_parameter {
            speed *= self.get(p);
        }
        let d = self.duration(layer, id, clips);
        if d <= 0.0 {
            0.0
        } else {
            speed / d
        }
    }

    /// A playing state's normalized time after `dt`, which runs on under a time parameter.
    fn advanced(&self, layer: usize, p: &Playing, dt: f32, clips: &[CompiledClip]) -> f32 {
        p.time + dt * self.rate(layer, p.state, clips)
    }

    /// The normalized time a state's motion is sampled at: its time parameter's value when it has one.
    pub fn sampled(&self, layer: usize, id: StateId, time: f32) -> f32 {
        match self.state(layer, id).and_then(|s| s.time_parameter.as_ref()) {
            Some(n) => self.get(n),
            None => time,
        }
    }

    /// A state's cycle offset: its cycle-offset parameter's value when it has one.
    pub fn cycle_offset(&self, layer: usize, id: StateId) -> f32 {
        match self.state(layer, id) {
            Some(s) => s.cycle_offset_parameter.as_ref().map_or(s.cycle_offset, |n| self.get(n)),
            None => 0.0,
        }
    }

    fn behaviours_of(&self, layer: usize, id: StateId) -> Vec<BehaviourIndex> {
        let l = &self.layers[layer];
        if let Some((_, map)) = &l.synced {
            return map.get(&id).map(|m| m.1.clone()).unwrap_or_default();
        }
        let Some(f) = &l.flat else { return vec![] };
        let mut out = Vec::new();
        for m in &f.paths[&id] {
            out.extend(f.machines[m].behaviours.iter().copied());
        }
        out.extend(f.states[&id].behaviours.iter().copied());
        out
    }

    /// The state an entry into `machine` lands in, following entry transitions into sub-machines.
    fn enter_machine(&self, flat: &Flat, machine: StateMachineId) -> Option<StateId> {
        let m = flat.machines.get(&machine)?;
        for t in pick_entry(&m.entry_transitions) {
            if self.all_hold(&t.conditions) {
                return self.resolve(flat, &t.destination, machine);
            }
        }
        m.default_state
    }

    fn resolve(&self, flat: &Flat, d: &Destination, from_machine: StateMachineId) -> Option<StateId> {
        match d {
            Destination::State { id } => Some(*id),
            Destination::StateMachine { id } => self.enter_machine(flat, *id),
            Destination::Exit {} => {
                let Some(&parent) = flat.parent.get(&from_machine) else {
                    return self.enter_machine(flat, flat.root);
                };
                let p = &flat.machines[&parent];
                for t in pick_entry(&p.state_machine_transitions_for(from_machine, flat)) {
                    if self.all_hold(&t.conditions) {
                        return self.resolve(flat, &t.destination, parent);
                    }
                }
                p.default_state
            }
        }
    }

    pub fn update(&mut self, dt: f32, clips: &[CompiledClip], entered: &mut Vec<Entered>, frozen: &dyn Fn(&ControllerRt, usize) -> Vec<(u32, f32, f32)>) {
        for li in 0..self.layers.len() {
            if self.layers[li].synced.is_some() {
                continue;
            }
            self.update_layer(li, dt, clips, entered, frozen);
            self.track_direct_leaves(li, clips);
        }
        for li in 0..self.layers.len() {
            let Some((src, _)) = self.layers[li].synced.clone().map(|(s, _)| (s, ())) else { continue };
            let (cur, act) = (self.layers[src].current.clone(), self.layers[src].active.clone());
            let before = self.layers[li].current.as_ref().map(|p| p.state);
            let before_to = self.layers[li].active.as_ref().map(|a| a.to.state);
            self.layers[li].snapped = self.layers[src].snapped.clone();
            self.layers[li].current = cur.map(|mut p| {
                p.direct_starts.clear();
                p
            });
            self.layers[li].active = act.map(|mut a| {
                a.to.direct_starts.clear();
                if let From::State(p) = &mut a.from {
                    p.direct_starts.clear();
                }
                a
            });
            let now = self.layers[li].current.as_ref().map(|p| p.state);
            let now_to = self.layers[li].active.as_ref().map(|a| a.to.state);
            if let Some(s) = now_to.filter(|s| Some(*s) != before_to && Some(*s) != before) {
                entered.push(Entered { layer: li, state: s, behaviours: self.behaviours_of(li, s) });
            } else if let Some(s) = now.filter(|s| !self.layers[li].started || (Some(*s) != before && Some(*s) != before_to)) {
                entered.push(Entered { layer: li, state: s, behaviours: self.behaviours_of(li, s) });
            }
            self.layers[li].started = true;
        }
    }

    fn track_direct_leaves(&mut self, li: usize, clips: &[CompiledClip]) {
        let track = |rt: &Self, p: &mut Playing| {
            let scrubbed = rt.state(li, p.state).is_some_and(|s| s.time_parameter.is_some());
            if scrubbed || !matches!(rt.motion(li, p.state), Some(Motion::BlendTree(_))) {
                return;
            }
            let mut ls = Vec::new();
            rt.state_leaves(li, p.state, &mut ls);
            let live: Vec<u32> = ls.iter().filter(|l| l.direct && l.weight > 0.0 && clips[l.clip as usize].looping).map(|l| l.id).collect();
            p.direct_starts.retain(|(id, _)| live.contains(id));
            let began = p.prev_time;
            for id in live {
                if !p.direct_starts.iter().any(|(i, _)| *i == id) {
                    p.direct_starts.push((id, began));
                }
            }
            p.prev_time = p.time;
        };
        let mut layer_cur = self.layers[li].current.take();
        let mut layer_act = self.layers[li].active.take();
        if let Some(p) = layer_cur.as_mut() {
            track(self, p);
        }
        if let Some(a) = layer_act.as_mut() {
            track(self, &mut a.to);
            if let From::State(p) = &mut a.from {
                track(self, p);
            }
        }
        self.layers[li].current = layer_cur;
        self.layers[li].active = layer_act;
    }

    fn update_layer(&mut self, li: usize, dt: f32, clips: &[CompiledClip], entered: &mut Vec<Entered>, frozen: &dyn Fn(&ControllerRt, usize) -> Vec<(u32, f32, f32)>) {
        self.layers[li].snapped = None;
        if !self.layers[li].started {
            self.layers[li].started = true;
            let flat = self.layers[li].flat.as_ref().unwrap();
            let start = self.enter_machine(flat, flat.root);
            if let Some(s) = start {
                self.layers[li].current = Some(Playing { state: s, time: 0.0, direct_starts: Vec::new(), prev_time: 0.0 });
                entered.push(Entered { layer: li, state: s, behaviours: self.behaviours_of(li, s) });
            }
        }
        let Some(cur) = self.layers[li].current.clone() else { return };
        let prev = cur.time;
        if let Some(a) = &self.layers[li].active {
            if matches!(a.from, From::State(_)) {
                let t = self.advanced(li, &cur, dt, clips);
                self.layers[li].current.as_mut().unwrap().time = t;
            }
            if self.interrupt(li, clips, entered, frozen) {
                return;
            }
            let mut a = self.layers[li].active.take().unwrap();
            if a.duration > 0.0 {
                a.progress += dt / a.duration;
            }
            if let From::State(p) = &mut a.from {
                p.time = self.advanced(li, p, dt, clips);
            }
            a.to.time = self.advanced(li, &a.to, dt, clips);
            if a.duration <= 0.0 || a.progress >= 1.0 {
                self.layers[li].current = Some(a.to.clone());
            } else {
                if let From::State(p) = &a.from {
                    self.layers[li].current = Some(p.clone());
                }
                self.layers[li].active = Some(a);
            }
            return;
        } else {
            let t = self.advanced(li, &cur, dt, clips);
            self.layers[li].current.as_mut().unwrap().time = t;
        }
        self.try_start(li, prev, clips, entered);
    }

    fn candidates_from(&self, li: usize, state: StateId) -> Vec<(Owner, Transition)> {
        let f = self.layers[li].flat.as_ref().unwrap();
        let mut out = Vec::new();
        let s = &f.states[&state];
        for (i, t) in pick(&s.transitions) {
            out.push((Owner::State(state, i), t.clone()));
        }
        out
    }

    fn any_candidates(&self, li: usize) -> Vec<(Owner, Transition)> {
        let f = self.layers[li].flat.as_ref().unwrap();
        pick(&f.any).into_iter().map(|(i, t)| (Owner::Any(i), t.clone())).collect()
    }

    /// Whether a transition fires now, given the state's normalized time before and after this update; returns
    /// the seconds past its exit time already spent.
    fn fires(&self, t: &Transition, prev: f32, now: f32, rate: f32) -> Option<f32> {
        if t.mute {
            return None;
        }
        if t.exit_time.is_none() && t.conditions.is_empty() {
            return None;
        }
        if !self.all_hold(&t.conditions) {
            return None;
        }
        match t.exit_time {
            None => Some(0.0),
            Some(e) => {
                let crossing = if e <= 1.0 {
                    let k = (now - e).floor();
                    let at = k + e;
                    (at > prev && at <= now).then_some(at)
                } else {
                    (e <= now).then_some(e)
                };
                let at = crossing?;
                Some(if rate > 0.0 { (now - at) / rate } else { 0.0 })
            }
        }
    }

    fn try_start(&mut self, li: usize, prev: f32, clips: &[CompiledClip], entered: &mut Vec<Entered>) {
        let cur = self.layers[li].current.clone().unwrap();
        let rate = self.rate(li, cur.state, clips);
        let mut cands = self.any_candidates(li);
        cands.extend(self.candidates_from(li, cur.state));
        for (owner, t) in cands {
            let Some(spent) = self.fires(&t, prev, cur.time, rate) else { continue };
            let flat = self.layers[li].flat.as_ref().unwrap();
            let machine = *flat.paths[&cur.state].last().unwrap();
            let Some(dest) = self.resolve(flat, &t.destination, machine) else { continue };
            if matches!(owner, Owner::Any(_)) && !t.can_transition_to_self && dest == cur.state {
                continue;
            }
            self.start(li, owner, &t, From::State(cur.clone()), cur.state, dest, spent, clips, entered);
            return;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn start(&mut self, li: usize, owner: Owner, t: &Transition, from: From, source_state: StateId, dest: StateId, spent: f32, clips: &[CompiledClip], entered: &mut Vec<Entered>) {
        self.consume_triggers(&t.conditions);
        let duration = match t.duration {
            Duration::Fixed(s) => s,
            Duration::Normalized(n) => {
                let r = self.rate(li, source_state, clips);
                if r > 0.0 {
                    n / r
                } else {
                    0.0
                }
            }
        };
        let dr = self.rate(li, dest, clips);
        let time = t.offset + spent * dr;
        let to = Playing { state: dest, time, direct_starts: Vec::new(), prev_time: 0.0 };
        let a = Active {
            from,
            to,
            progress: if duration > 0.0 { spent / duration } else { 1.0 },
            duration,
            owner,
            source_state,
            interruption: t.interruption_source,
            ordered: t.ordered_interruption,
        };
        entered.push(Entered { layer: li, state: dest, behaviours: self.behaviours_of(li, dest) });
        if a.duration <= 0.0 {
            if let From::State(p) = &a.from {
                self.layers[li].snapped = Some(p.clone());
            }
            self.layers[li].current = Some(a.to.clone());
            self.layers[li].active = None;
            return;
        }
        self.layers[li].active = Some(a);
    }

    fn interrupt(&mut self, li: usize, clips: &[CompiledClip], entered: &mut Vec<Entered>, frozen: &dyn Fn(&ControllerRt, usize) -> Vec<(u32, f32, f32)>) -> bool {
        let a = self.layers[li].active.clone().unwrap();
        let src = a.source_state;
        let dst = a.to.state;
        let any = || {
            let mut c = self.any_candidates(li);
            if a.ordered {
                if let Owner::Any(k) = a.owner {
                    c.retain(|(o, _)| matches!(o, Owner::Any(i) if *i < k));
                }
            }
            c
        };
        let source_list = || {
            let mut c = any();
            let mut own = self.candidates_from(li, src);
            if a.ordered {
                match a.owner {
                    Owner::State(s, k) if s == src => own.retain(|(o, _)| matches!(o, Owner::State(_, i) if *i < k)),
                    Owner::Any(_) => own.clear(),
                    _ => {}
                }
            }
            c.extend(own);
            c
        };
        let dest_list = || self.candidates_from(li, dst);
        let mut cands = Vec::new();
        match a.interruption {
            InterruptionSource::None => {}
            InterruptionSource::Source => cands.extend(source_list()),
            InterruptionSource::Destination => cands.extend(dest_list()),
            InterruptionSource::SourceThenDestination => {
                cands.extend(source_list());
                cands.extend(dest_list());
            }
            InterruptionSource::DestinationThenSource => {
                cands.extend(dest_list());
                cands.extend(source_list());
            }
        }
        for (owner, t) in cands {
            if t.mute || (t.exit_time.is_none() && t.conditions.is_empty()) || !self.all_hold(&t.conditions) {
                continue;
            }
            if t.exit_time.is_some() {
                continue;
            }
            let flat = self.layers[li].flat.as_ref().unwrap();
            let machine = *flat.paths[&dst].last().unwrap();
            let Some(dest) = self.resolve(flat, &t.destination, machine) else { continue };
            let pose = frozen(self, li);
            self.start(li, owner, &t, From::Frozen(pose), src, dest, 0.0, clips, entered);
            return true;
        }
        false
    }
}

trait MachineTransitions {
    fn state_machine_transitions_for(&self, child: StateMachineId, flat: &Flat) -> Vec<EntryTransition>;
}

impl MachineTransitions for StateMachine {
    /// Unity keeps a sub-machine's exit transitions on its parent; the model lists them on the child.
    fn state_machine_transitions_for(&self, child: StateMachineId, flat: &Flat) -> Vec<EntryTransition> {
        flat.machines.get(&child).map(|c| c.state_machine_transitions.clone()).unwrap_or_default()
    }
}

fn pick(list: &[Transition]) -> Vec<(usize, &Transition)> {
    let solo = list.iter().any(|t| t.solo);
    list.iter().enumerate().filter(|(_, t)| !t.mute && (!solo || t.solo)).collect()
}

fn pick_entry(list: &[EntryTransition]) -> Vec<EntryTransition> {
    let solo = list.iter().any(|t| t.solo);
    list.iter().filter(|t| !t.mute && (!solo || t.solo)).cloned().collect()
}

fn collect_motions(m: &StateMachine, out: &mut Vec<Motion>) {
    for s in &m.states {
        if let Some(mo) = &s.motion {
            out.push(mo.clone());
        }
    }
    for c in &m.state_machines {
        collect_motions(c, out);
    }
}

pub fn clip_ids(m: &Motion, out: &mut Vec<u32>) {
    match m {
        Motion::Clip(c) => out.push(c.0),
        Motion::BlendTree(t) => {
            use crate::model::BlendAlgorithm::*;
            let kids: Vec<&Option<Motion>> = match &t.algorithm {
                Simple1D { children, .. } => children.iter().map(|c| &c.motion).collect(),
                SimpleDirectional2D { children, .. }
                | FreeformDirectional2D { children, .. }
                | FreeformCartesian2D { children, .. } => children.iter().map(|c| &c.motion).collect(),
                Direct { children, .. } => children.iter().map(|c| &c.motion).collect(),
            };
            for k in kids.into_iter().flatten() {
                clip_ids(k, out);
            }
        }
    }
}
