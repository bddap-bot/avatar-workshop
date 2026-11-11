//! Unity's humanoid curve names: what each moves, which mask part admits it, and its mirror.

use crate::model::{BodyPart, HumanBodyBone};

/// The body and the IK goals, whose curves are `<name>T.<x|y|z>` and `<name>Q.<x|y|z|w>`.
pub const GOALS: [(&str, BodyPart); 6] = [
    ("Root", BodyPart::Root),
    ("Motion", BodyPart::Root),
    ("LeftFoot", BodyPart::LeftFootIK),
    ("RightFoot", BodyPart::RightFootIK),
    ("LeftHand", BodyPart::LeftHandIK),
    ("RightHand", BodyPart::RightHandIK),
];

/// The goal a body or IK goal curve belongs to, with its component (`T.x` … `Q.w`).
pub fn goal(name: &str) -> Option<(usize, &str)> {
    GOALS.iter().enumerate().find_map(|(i, (g, _))| {
        let rest = name.strip_prefix(g)?;
        (rest.len() == 3 && (rest.starts_with("T.") || rest.starts_with("Q."))).then_some((i, rest))
    })
}

/// The bone a muscle or translation DoF moves; None for the body and IK goals.
pub fn bone(name: &str) -> Option<HumanBodyBone> {
    let named = |n: &str| serde_json::from_value(serde_json::Value::String(n.to_string())).ok();
    if goal(name).is_some() {
        return None;
    }
    if let Some((b, _)) = name.split_once("TDOF.") {
        return named(b);
    }
    for side in ["Left", "Right"] {
        let Some(rest) = name.strip_prefix(&format!("{side}Hand.")) else { continue };
        let (finger, dof) = rest.split_once('.')?;
        let segment = match dof.chars().next()? {
            '2' => "Intermediate",
            '3' => "Distal",
            _ => "Proximal",
        };
        return named(&format!("{side}{finger}{segment}"));
    }
    const LIMBS: [(&str, &str); 9] = [
        ("Upper Leg ", "UpperLeg"),
        ("Lower Leg ", "LowerLeg"),
        ("Foot ", "Foot"),
        ("Toes ", "Toes"),
        ("Shoulder ", "Shoulder"),
        ("Arm ", "UpperArm"),
        ("Forearm ", "LowerArm"),
        ("Hand ", "Hand"),
        ("Eye ", "Eye"),
    ];
    for side in ["Left ", "Right "] {
        let Some(rest) = name.strip_prefix(side) else { continue };
        let (_, b) = LIMBS.iter().find(|(p, _)| rest.starts_with(p))?;
        return named(&format!("{}{b}", side.trim_end()));
    }
    ["UpperChest", "Spine", "Chest", "Neck", "Head", "Jaw"].iter().find(|b| name.starts_with(&format!("{b} "))).and_then(|b| named(b))
}

/// The avatar-mask body part admitting a humanoid curve.
pub fn part(name: &str) -> BodyPart {
    if let Some((g, _)) = goal(name) {
        return GOALS[g].1;
    }
    let Some(b) = bone(name) else { return BodyPart::Body };
    let b = format!("{b:?}");
    let (left, right) = (b.starts_with("Left"), b.starts_with("Right"));
    let finger = ["Thumb", "Index", "Middle", "Ring", "Little"].iter().any(|f| b.contains(f));
    let leg = ["Leg", "Foot", "Toes"].iter().any(|f| b.contains(f));
    match (left, right) {
        _ if b.ends_with("Eye") || ["Neck", "Head", "Jaw"].contains(&b.as_str()) => BodyPart::Head,
        (true, _) if finger => BodyPart::LeftFingers,
        (_, true) if finger => BodyPart::RightFingers,
        (true, _) if leg => BodyPart::LeftLeg,
        (_, true) if leg => BodyPart::RightLeg,
        (true, _) => BodyPart::LeftArm,
        (_, true) => BodyPart::RightArm,
        _ => BodyPart::Body,
    }
}

/// A humanoid curve's counterpart across the body's midline, and whether its value changes sign: midline
/// Left-Right and twist muscles, and each goal's x position and y and z rotation.
pub fn mirrored(name: &str) -> (String, bool) {
    let swapped = if let Some(r) = name.strip_prefix("Left") {
        format!("Right{r}")
    } else if let Some(r) = name.strip_prefix("Right") {
        format!("Left{r}")
    } else {
        name.to_string()
    };
    let negate = match goal(name) {
        Some((_, c)) => matches!(c, "T.x" | "Q.y" | "Q.z"),
        None => swapped == name && name.contains("Left-Right"),
    };
    (swapped, negate)
}
