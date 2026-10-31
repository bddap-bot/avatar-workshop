use std::collections::BTreeMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;

use avatar_core::validate::{references, Reference};
use avatar_core::{read, write, Error, MAX_JSON_BYTES};
use serde_json::{json, Value};

const CASES: usize = 10_000;

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

struct Seed {
    bytes: Vec<u8>,
    json: Value,
    bin: Vec<u8>,
    references: Vec<Reference>,
}

impl Seed {
    fn load(relative: &str) -> Seed {
        let bytes =
            std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)).unwrap();
        let avatar = read(&bytes).unwrap();
        let len = u32::from_le_bytes(bytes[12..16].try_into().unwrap()) as usize;
        Seed {
            json: serde_json::from_slice(&bytes[20..20 + len]).unwrap(),
            references: references(&avatar.model),
            bin: avatar.bin,
            bytes,
        }
    }
}

fn glb(json: &[u8], bin: &[u8]) -> Vec<u8> {
    let mut json = json.to_vec();
    json.resize(json.len().next_multiple_of(4), b' ');
    let mut out = b"glTF".to_vec();
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&json);
    if !bin.is_empty() {
        out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        out.extend_from_slice(b"BIN\0");
        out.extend_from_slice(bin);
    }
    let total = out.len() as u32;
    out[8..12].copy_from_slice(&total.to_le_bytes());
    out
}

fn with_model(seed: &Seed, edit: impl FnOnce(&mut Value)) -> Vec<u8> {
    let mut json = seed.json.clone();
    edit(&mut json["extensions"]["AW_avatar"]);
    glb(&serde_json::to_vec(&json).unwrap(), &seed.bin)
}

/// JSON pointers to every value below `value` that `keep` selects, `value` itself included.
fn pointers(value: &Value, keep: fn(&Value) -> bool) -> Vec<String> {
    fn walk(value: &Value, at: String, keep: fn(&Value) -> bool, out: &mut Vec<String>) {
        if keep(value) {
            out.push(at.clone());
        }
        match value {
            Value::Array(items) => {
                for (i, item) in items.iter().enumerate() {
                    walk(item, format!("{at}/{i}"), keep, out);
                }
            }
            Value::Object(fields) => {
                for (key, item) in fields {
                    let key = key.replace('~', "~0").replace('/', "~1");
                    walk(item, format!("{at}/{key}"), keep, out);
                }
            }
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(value, String::new(), keep, &mut out);
    out
}

fn below_root(model: &Value) -> Vec<String> {
    let mut all = pointers(model, |_| true);
    all.remove(0);
    all
}

fn is_declaration(r: &Reference) -> bool {
    r.pointer.ends_with("/id") && !r.pointer.ends_with("/destination/id")
}

fn scoped(r: &Reference) -> bool {
    matches!(r.kind, "StateId" | "StateMachineId")
}

fn controller(pointer: &str) -> &str {
    let end = pointer
        .match_indices('/')
        .nth(2)
        .map_or(pointer.len(), |(i, _)| i);
    &pointer[..end]
}

const CLASSES: [&str; 12] = [
    "truncated",
    "header bit flipped",
    "random bytes",
    "value of the wrong type",
    "unknown field",
    "index out of range",
    "dangling state reference",
    "duplicate state id",
    "glTF count mismatch",
    "nesting past the recursion limit",
    "trailing JSON",
    "duplicate parameter name",
];

fn mutate(class: usize, seed: &Seed, rng: &mut Rng) -> Vec<u8> {
    let bytes = &seed.bytes;
    match CLASSES[class] {
        "truncated" => bytes[..rng.below(bytes.len())].to_vec(),
        "header bit flipped" => {
            let mut out = bytes.clone();
            out[rng.below(20)] ^= 1 << rng.below(8);
            out
        }
        "random bytes" => {
            let mut out: Vec<u8> = (0..rng.below(4096)).map(|_| rng.next() as u8).collect();
            if out.starts_with(b"glTF") {
                out[0] = 0;
            }
            out
        }
        "value of the wrong type" => with_model(seed, |model| {
            let all = below_root(model);
            let target = model.pointer_mut(rng.pick::<String>(&all)).unwrap();
            *target = match target {
                Value::Number(_) | Value::Bool(_) => json!("fuzz"),
                Value::String(_) => json!(true),
                Value::Null | Value::Array(_) | Value::Object(_) => json!(7),
            };
        }),
        "unknown field" => with_model(seed, |model| {
            let all = pointers(model, Value::is_object);
            let target = model.pointer_mut(rng.pick::<String>(&all)).unwrap();
            target
                .as_object_mut()
                .unwrap()
                .insert("fuzz".into(), json!({ "fuzz": 1 }));
        }),
        "index out of range" => with_model(seed, |model| {
            let global: Vec<_> = seed.references.iter().filter(|r| !scoped(r)).collect();
            let r = rng.pick(&global);
            *model.pointer_mut(&r.pointer).unwrap() = json!(1_000_000 + rng.below(1 << 31));
        }),
        "dangling state reference" => with_model(seed, |model| {
            let used: Vec<_> = seed
                .references
                .iter()
                .filter(|r| scoped(r) && !is_declaration(r))
                .collect();
            let r = rng.pick(&used);
            *model.pointer_mut(&r.pointer).unwrap() = json!(1_000_000 + rng.below(1 << 31));
        }),
        "duplicate state id" => with_model(seed, |model| {
            let mut groups: BTreeMap<_, Vec<&Reference>> = BTreeMap::new();
            for r in seed.references.iter().filter(|r| is_declaration(r)) {
                groups
                    .entry((controller(&r.pointer), r.kind))
                    .or_default()
                    .push(r);
            }
            let groups: Vec<_> = groups.into_values().filter(|g| g.len() > 1).collect();
            let group = rng.pick(&groups);
            let from = rng.below(group.len());
            let to = (from + 1 + rng.below(group.len() - 1)) % group.len();
            *model.pointer_mut(&group[to].pointer).unwrap() = json!(group[from].index);
        }),
        "glTF count mismatch" => {
            let mut json = seed.json.clone();
            let key = *rng.pick::<&str>(&["nodes", "materials"]);
            let target = if rng.below(2) == 0 {
                &mut json[key]
            } else {
                &mut json["extensions"]["AW_avatar"][key]
            };
            let items = target.as_array_mut().unwrap();
            if rng.below(2) == 0 {
                items.pop();
            } else {
                items.push(items[0].clone());
            }
            glb(&serde_json::to_vec(&json).unwrap(), &seed.bin)
        }
        "nesting past the recursion limit" => with_model(seed, |model| {
            let all = below_root(model);
            let depth = 129 + rng.below(1_000);
            let mut bomb = json!(0);
            for _ in 0..depth {
                bomb = Value::Array(vec![bomb]);
            }
            *model.pointer_mut(rng.pick::<String>(&all)).unwrap() = bomb;
        }),
        "trailing JSON" => {
            let mut json = serde_json::to_vec(&seed.json).unwrap();
            let junk = *rng.pick::<&[u8]>(&[&b"{}"[..], b"0", b"x", b",", b"\"", b"]"]);
            json.extend_from_slice(junk);
            glb(&json, &seed.bin)
        }
        "duplicate parameter name" => with_model(seed, |model| {
            let parameters = model["parameters"].as_array_mut().unwrap();
            let from = rng.below(parameters.len());
            let to = (from + 1 + rng.below(parameters.len() - 1)) % parameters.len();
            parameters[to]["name"] = parameters[from]["name"].clone();
        }),
        other => unreachable!("{other}"),
    }
}

#[test]
fn fuzzed_inputs_are_rejected_with_a_reason() {
    let seeds = [
        Seed::load("fixtures/synthetic.glb"),
        Seed::load("fixtures/unity-synthetic.glb"),
    ];
    let mut rng = Rng(0x5eed);
    let mut rejected = [0usize; CLASSES.len()];
    for case in 0..CASES {
        let class = case % CLASSES.len();
        let seed = &seeds[(case / CLASSES.len()) % seeds.len()];
        let input = mutate(class, seed, &mut rng);
        let outcome = catch_unwind(AssertUnwindSafe(|| read(&input)));
        let name = CLASSES[class];
        match outcome {
            Err(_) => panic!("case {case} ({name}) panicked the reader"),
            Ok(Ok(_)) => panic!("case {case} ({name}) was accepted"),
            Ok(Err(e)) => assert!(!e.to_string().is_empty(), "case {case} ({name})"),
        }
        rejected[class] += 1;
    }
    assert_eq!(rejected.iter().sum::<usize>(), CASES);
    for (name, count) in CLASSES.iter().zip(rejected) {
        println!("{count} rejected: {name}");
    }
}

/// Byte changes inside the JSON chunk may still be valid; whatever the reader accepts, the
/// writer writes back to the same avatar.
#[test]
fn json_byte_changes_never_panic() {
    let seed = Seed::load("fixtures/synthetic.glb");
    let len = u32::from_le_bytes(seed.bytes[12..16].try_into().unwrap()) as usize;
    let mut rng = Rng(0xb17e);
    let mut accepted = 0;
    for case in 0..CASES {
        let mut input = seed.bytes.clone();
        for _ in 0..1 + rng.below(4) {
            input[20 + rng.below(len)] = rng.next() as u8;
        }
        match catch_unwind(|| read(&input)) {
            Err(_) => panic!("case {case} panicked the reader"),
            Ok(Err(e)) => assert!(!e.to_string().is_empty()),
            Ok(Ok(avatar)) => {
                accepted += 1;
                let again = read(&write(&avatar).unwrap()).unwrap();
                assert_eq!(again, avatar, "case {case} did not round-trip");
            }
        }
    }
    println!("{accepted} of {CASES} accepted");
}

#[test]
fn every_reference_out_of_range_is_refused_where_it_is() {
    let seed = Seed::load("fixtures/synthetic.glb");
    let kinds: std::collections::BTreeSet<_> = seed.references.iter().map(|r| r.kind).collect();
    let schema = avatar_core::schema();
    let index_types: std::collections::BTreeSet<_> = schema["$defs"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .filter(|name| name.ends_with("Index") || name.ends_with("Id"))
        .collect();
    assert_eq!(
        kinds, index_types,
        "the fixture refers through every index type"
    );
    for r in seed.references.iter().filter(|r| !is_declaration(r)) {
        let input = with_model(&seed, |model| {
            *model.pointer_mut(&r.pointer).unwrap() = json!(u32::MAX);
        });
        match read(&input) {
            Err(Error::Invalid(invalid)) => assert_eq!(invalid.at, r.pointer, "{}", invalid.reason),
            other => panic!("{} out of range: {other:?}", r.pointer),
        }
    }
}

#[test]
fn an_oversized_json_chunk_is_refused_before_parsing() {
    let input = glb(&vec![b' '; MAX_JSON_BYTES + 4], &[]);
    assert!(matches!(read(&input), Err(Error::TooLarge(_))));
}

#[test]
fn an_unknown_field_is_refused_on_every_object() {
    for file in ["fixtures/synthetic.glb", "fixtures/unity-synthetic.glb"] {
        let seed = Seed::load(file);
        let all = pointers(&seed.json["extensions"]["AW_avatar"], Value::is_object);
        let accepted: Vec<_> = all
            .iter()
            .filter(|at| {
                read(&with_model(&seed, |model| {
                    model
                        .pointer_mut(at)
                        .unwrap()
                        .as_object_mut()
                        .unwrap()
                        .insert("fuzz".into(), json!({ "fuzz": 1 }));
                }))
                .is_ok()
            })
            .collect();
        assert!(accepted.is_empty(), "{file}: {accepted:?}");
    }
}
