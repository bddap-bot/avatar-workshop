mod reference;

use std::path::Path;

use avatar_core::animator::Animator;
use reference::animator::Run;

fn fixture() -> avatar_core::Avatar {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    avatar_core::read(&std::fs::read(root.join("fixtures/unity-synthetic.glb")).unwrap()).unwrap()
}

#[test]
fn fixture_follows_the_reference() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/reference/fixture-animator");
    let rep = Run::new(&dir, fixture()).run(None);
    assert!(rep.frames > 4900, "{} frames", rep.frames);
    assert_eq!(rep.parameters.wrong, 0, "parameters: {}", rep.parameters.summary(20));
    assert_eq!(rep.states.wrong, 0, "states: {}", rep.states.summary(20));
    assert_eq!(rep.values.wrong, 0, "values: {}", rep.values.summary(20));
    assert!(rep.values.compared > 1_500_000, "{} values compared", rep.values.compared);
}

fn wave(local: bool) -> Animator {
    let mut a = Animator::new(&fixture());
    a.set_builtin("IsLocal", local as u8 as f32);
    a.set("Wave", 1.0);
    for _ in 0..30 {
        a.step(1.0 / 60.0);
    }
    a
}

#[test]
fn local_only_drivers_run_for_the_wearer_alone() {
    let (worn, remote) = (wave(true), wave(false));
    assert_eq!((worn.get("Setter"), worn.get("Local")), (Some(1.0), Some(1.0)));
    assert_eq!((remote.get("Setter"), remote.get("Local")), (Some(1.0), Some(0.0)));
}

#[test]
fn saved_parameters_survive_a_reload() {
    let mut a = Animator::new(&fixture());
    a.set("Hat", 1.0);
    a.set("Hue", 0.9);
    let saved = a.saved();
    assert!(saved.iter().any(|(n, _)| n == "Hat") && !saved.iter().any(|(n, _)| n == "Hue"));
    let mut b = Animator::new(&fixture());
    b.restore(&saved);
    b.restore(&[("Hue".to_string(), 0.9)]);
    assert_eq!((b.get("Hat"), b.get("Hue")), (Some(1.0), Some(0.25)));
}
