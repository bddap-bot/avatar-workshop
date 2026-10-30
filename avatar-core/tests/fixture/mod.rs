use std::collections::{BTreeMap, BTreeSet};

use avatar_core::model::*;
use avatar_core::Avatar;
use serde_json::{json, Value};

const IDENTITY: Quat = [0.0, 0.0, 0.0, 1.0];

fn n(i: u32) -> NodeIndex {
    NodeIndex(i)
}

fn key(time: f32, value: f32) -> Keyframe {
    Keyframe {
        time,
        value,
        in_tangent: 0.0,
        out_tangent: 0.0,
        in_weight: 1.0 / 3.0,
        out_weight: 1.0 / 3.0,
        weighted_mode: WeightedMode::None,
    }
}

fn flat(value: f32) -> Curved {
    Curved { value, curve: None }
}

fn curved(value: f32) -> Curved {
    Curved {
        value,
        curve: Some(AnimationCurve {
            keys: vec![
                key(0.0, 1.0),
                Keyframe {
                    in_tangent: -0.5,
                    out_tangent: -0.5,
                    weighted_mode: WeightedMode::Both,
                    in_weight: 0.25,
                    out_weight: 0.75,
                    ..key(1.0, 0.5)
                },
            ],
        }),
    }
}

fn channel() -> Channel {
    Channel {
        at_rest: [0.0, 0.0, 0.0],
        offset: [0.0, 0.1, 0.0],
        affects: [true, false, true],
    }
}

fn source(node: u32) -> ConstraintSource {
    ConstraintSource {
        source: SourceTarget::Node(n(node)),
        weight: 1.0,
        position_offset: [0.0; 3],
        rotation_offset: [0.0; 3],
    }
}

fn transition(
    destination: Destination,
    conditions: Vec<Condition>,
    duration: Duration,
) -> Transition {
    Transition {
        destination,
        conditions,
        exit_time: None,
        duration,
        offset: 0.0,
        interruption_source: InterruptionSource::None,
        ordered_interruption: true,
        can_transition_to_self: false,
        mute: false,
        solo: false,
    }
}

fn cond(mode: ConditionMode, parameter: &str, threshold: f32) -> Condition {
    Condition {
        mode,
        parameter: parameter.into(),
        threshold,
    }
}

fn state(id: u32, name: &str, motion: Option<Motion>, transitions: Vec<Transition>) -> State {
    State {
        id: StateId(id),
        name: name.into(),
        tag: String::new(),
        speed: 1.0,
        speed_parameter: None,
        cycle_offset: 0.0,
        cycle_offset_parameter: None,
        mirror: false,
        mirror_parameter: None,
        time_parameter: None,
        motion,
        write_defaults: false,
        ik_on_feet: false,
        behaviours: vec![],
        transitions,
    }
}

fn control(name: &str, kind: ControlKind) -> Control {
    Control {
        name: name.into(),
        icon: None,
        kind,
    }
}

fn label(name: &str) -> Label {
    Label {
        name: name.into(),
        icon: None,
    }
}

fn curve(binding: Binding, keys: Vec<Keyframe>) -> Curve {
    Curve {
        binding,
        curve: AnimationCurve { keys },
    }
}

fn clip(name: &str, curves: Vec<Curve>) -> Clip {
    Clip {
        name: name.into(),
        length: 1.0 / 60.0,
        looping: false,
        curves,
        slot_curves: vec![],
    }
}

fn material(shader: ShaderFamily, shader_name: &str) -> Material {
    Material {
        shader,
        shader_name: shader_name.into(),
        version: None,
        properties: BTreeMap::new(),
        keywords: vec![],
        render_queue: 2000,
        cull: Cull::Back,
        blend: None,
        depth_write: true,
        depth_test: CompareFunction::LessEqual,
    }
}

fn controller(
    name: &str,
    parameters: Vec<AnimatorParameter>,
    layers: Vec<AnimatorLayer>,
) -> Controller {
    Controller {
        name: name.into(),
        parameters,
        layers,
    }
}

fn layer(name: &str, blending: LayerBlending, state_machine: StateMachine) -> AnimatorLayer {
    AnimatorLayer {
        name: name.into(),
        default_weight: 1.0,
        blending,
        mask: None,
        ik_pass: false,
        content: LayerContent::StateMachine { state_machine },
    }
}

fn machine(id: u32, name: &str, states: Vec<State>, default_state: Option<u32>) -> StateMachine {
    StateMachine {
        id: StateMachineId(id),
        name: name.into(),
        states,
        state_machines: vec![],
        default_state: default_state.map(StateId),
        any_state_transitions: vec![],
        entry_transitions: vec![],
        state_machine_transitions: vec![],
        behaviours: vec![],
    }
}

fn aparam(name: &str, default: AnimatorParameterValue) -> AnimatorParameter {
    AnimatorParameter {
        name: name.into(),
        default,
    }
}

/// Node indices of the fixture's glTF scene.
const AVATAR: u32 = 0;
const BODY: u32 = 1;
const HIPS: u32 = 2;
const SPINE: u32 = 3;
const HEAD: u32 = 4;
const HAIR: u32 = 5;
const HAIR_TIP: u32 = 6;
const PROP: u32 = 7;
const LEFT_HAND: u32 = 8;
const RIGHT_HAND: u32 = 9;
const LEFT_EYE: u32 = 10;
const RIGHT_EYE: u32 = 11;
const NODE_COUNT: u32 = 12;

fn model() -> AwAvatar {
    let mut fur = material(ShaderFamily::Poiyomi, ".poiyomi/Poiyomi Toon");
    fur.version = Some("9.2.38".into());
    fur.properties = BTreeMap::from([
        (
            "_Color".into(),
            MaterialProperty::Color([1.0, 0.9, 0.8, 1.0]),
        ),
        ("_EnableEmission".into(), MaterialProperty::Float(1.0)),
        ("_MainHueShift".into(), MaterialProperty::Float(0.25)),
        ("_StencilRef".into(), MaterialProperty::Int(3)),
        (
            "_RimLightDirection".into(),
            MaterialProperty::Vector([0.0, 1.0, 0.0, 0.0]),
        ),
        (
            "_MainTex".into(),
            MaterialProperty::Texture(TextureProperty {
                texture: Some(TextureIndex(0)),
                tiling: [1.0, 1.0],
                offset: [0.0, 0.0],
            }),
        ),
        (
            "_DecalTexture".into(),
            MaterialProperty::Texture(TextureProperty {
                texture: None,
                tiling: [2.0, 2.0],
                offset: [0.5, 0.0],
            }),
        ),
    ]);
    fur.keywords = vec!["_EMISSION".into(), "VIGNETTE_MASKED".into()];
    fur.cull = Cull::Off;

    let mut glass = material(ShaderFamily::Liltoon, "lilToon/Transparent");
    glass.render_queue = 3000;
    glass.depth_write = false;
    glass.blend = Some(Blend {
        src_color: BlendFactor::SrcAlpha,
        dst_color: BlendFactor::OneMinusSrcAlpha,
        src_alpha: BlendFactor::One,
        dst_alpha: BlendFactor::OneMinusSrcAlpha,
        color_op: BlendOp::Add,
        alpha_op: BlendOp::Max,
    });
    let mut face = material(ShaderFamily::Mtoon, "VRM/MToon");
    face.cull = Cull::Front;
    let others = [
        material(ShaderFamily::VrchatMobile, "VRChat/Mobile/Toon Lit"),
        material(ShaderFamily::Standard, "Standard"),
        material(ShaderFamily::Unknown, "Custom/Sparkles"),
    ];

    let toggle_clip = ClipIndex(0);
    let wave_clip = ClipIndex(1);
    let blend_clips = [ClipIndex(2), ClipIndex(3), ClipIndex(4), ClipIndex(5)];

    let blend_children = || -> Vec<Child2D> {
        blend_clips
            .iter()
            .zip([[0.0, 1.0], [1.0, 0.0], [0.0, -1.0], [-1.0, 0.0]])
            .map(|(c, position)| Child2D {
                motion: Some(Motion::Clip(*c)),
                position,
                time_scale: 1.0,
                cycle_offset: 0.0,
                mirror: false,
            })
            .collect()
    };

    let fx_states = vec![
        State {
            behaviours: vec![BehaviourIndex(0)],
            write_defaults: true,
            ..state(
                1,
                "PropOff",
                Some(Motion::Clip(toggle_clip)),
                vec![Transition {
                    exit_time: Some(0.75),
                    interruption_source: InterruptionSource::SourceThenDestination,
                    ..transition(
                        Destination::State { id: StateId(2) },
                        vec![cond(ConditionMode::If, "Prop", 0.0)],
                        Duration::Fixed(0.25),
                    )
                }],
            )
        },
        State {
            speed_parameter: Some("Speed".into()),
            cycle_offset_parameter: Some("Offset".into()),
            mirror_parameter: Some("Mirror".into()),
            time_parameter: Some("Sweep".into()),
            tag: "on".into(),
            mirror: true,
            ik_on_feet: true,
            ..state(
                2,
                "PropOn",
                Some(Motion::BlendTree(Box::new(BlendTree {
                    name: "Puppet".into(),
                    algorithm: BlendAlgorithm::Simple1D {
                        blend_parameter: "Sweep".into(),
                        children: vec![
                            Child1D {
                                motion: Some(Motion::Clip(blend_clips[0])),
                                threshold: 0.0,
                                time_scale: 1.0,
                                cycle_offset: 0.0,
                                mirror: false,
                            },
                            Child1D {
                                motion: None,
                                threshold: 1.0,
                                time_scale: 2.0,
                                cycle_offset: 0.5,
                                mirror: true,
                            },
                        ],
                    },
                }))),
                vec![transition(
                    Destination::Exit {},
                    vec![
                        cond(ConditionMode::IfNot, "Prop", 0.0),
                        cond(ConditionMode::Greater, "Sweep", 0.1),
                        cond(ConditionMode::Less, "Sweep", 0.9),
                        cond(ConditionMode::Equals, "Outfit", 2.0),
                        cond(ConditionMode::NotEqual, "Outfit", 3.0),
                    ],
                    Duration::Normalized(0.5),
                )],
            )
        },
    ];

    let mut fx_machine = machine(0, "FX", fx_states, Some(1));
    fx_machine.any_state_transitions = vec![Transition {
        can_transition_to_self: true,
        interruption_source: InterruptionSource::Destination,
        ..transition(
            Destination::StateMachine {
                id: StateMachineId(1),
            },
            vec![cond(ConditionMode::If, "Reset", 0.0)],
            Duration::Fixed(0.0),
        )
    }];
    fx_machine.entry_transitions = vec![EntryTransition {
        destination: Destination::State { id: StateId(2) },
        conditions: vec![cond(ConditionMode::If, "Prop", 0.0)],
        mute: false,
        solo: false,
    }];
    fx_machine.behaviours = vec![BehaviourIndex(5)];
    let mut sub = machine(
        1,
        "Blends",
        vec![
            state(
                4,
                "Directional",
                Some(Motion::BlendTree(Box::new(BlendTree {
                    name: "Simple".into(),
                    algorithm: BlendAlgorithm::SimpleDirectional2D {
                        blend_parameter: "PuppetX".into(),
                        blend_parameter_y: "PuppetY".into(),
                        children: blend_children(),
                    },
                }))),
                vec![],
            ),
            state(
                5,
                "Freeform",
                Some(Motion::BlendTree(Box::new(BlendTree {
                    name: "Freeform".into(),
                    algorithm: BlendAlgorithm::FreeformDirectional2D {
                        blend_parameter: "PuppetX".into(),
                        blend_parameter_y: "PuppetY".into(),
                        children: blend_children(),
                    },
                }))),
                vec![],
            ),
            state(
                6,
                "Cartesian",
                Some(Motion::BlendTree(Box::new(BlendTree {
                    name: "Cartesian".into(),
                    algorithm: BlendAlgorithm::FreeformCartesian2D {
                        blend_parameter: "PuppetX".into(),
                        blend_parameter_y: "PuppetY".into(),
                        children: blend_children(),
                    },
                }))),
                vec![],
            ),
            state(
                7,
                "Direct",
                Some(Motion::BlendTree(Box::new(BlendTree {
                    name: "Direct".into(),
                    algorithm: BlendAlgorithm::Direct {
                        normalized_blend_values: true,
                        children: ["Smile", "Frown"]
                            .into_iter()
                            .zip(blend_clips)
                            .map(|(parameter, clip)| ChildDirect {
                                motion: Some(Motion::Clip(clip)),
                                direct_blend_parameter: parameter.into(),
                                time_scale: 1.0,
                                cycle_offset: 0.0,
                                mirror: false,
                            })
                            .collect(),
                    },
                }))),
                vec![],
            ),
        ],
        Some(4),
    );
    sub.state_machine_transitions = vec![EntryTransition {
        destination: Destination::State { id: StateId(1) },
        conditions: vec![],
        mute: true,
        solo: true,
    }];
    fx_machine.state_machines = vec![sub];

    let gesture_machine = machine(
        0,
        "Gesture",
        vec![state(1, "Idle", Some(Motion::Clip(wave_clip)), vec![])],
        Some(1),
    );

    let mut synced = layer(
        "Synced",
        LayerBlending::Additive,
        machine(0, "Synced", vec![], None),
    );
    synced.default_weight = 0.5;
    synced.ik_pass = true;
    synced.mask = Some(AvatarMask {
        body_parts: BTreeSet::from([BodyPart::Head]),
        transforms: vec![MaskTransform {
            node: n(HAIR),
            active: false,
        }],
    });
    synced.content = LayerContent::Synced {
        layer: 0,
        affects_timing: true,
        states: vec![
            SyncedState {
                state: StateId(1),
                motion: Some(Motion::Clip(wave_clip)),
                behaviours: vec![],
            },
            SyncedState {
                state: StateId(2),
                motion: None,
                behaviours: vec![BehaviourIndex(6)],
            },
        ],
    };

    let tracking = TrackingType::NoChange;

    AwAvatar {
        source: Source {
            importer: "avatar-workshop synthetic fixture".into(),
            importer_version: "1".into(),
            format: SourceFormat::Vrchat,
            licence: Some("CC0-1.0".into()),
            author: Some("Avatar Workshop".into()),
        },
        humanoid: Humanoid {
            bones: BTreeMap::from([
                (
                    HumanBodyBone::Hips,
                    HumanBone {
                        node: n(HIPS),
                        limit: None,
                    },
                ),
                (
                    HumanBodyBone::Spine,
                    HumanBone {
                        node: n(SPINE),
                        limit: Some(MuscleLimit {
                            min: [-40.0, -40.0, -40.0],
                            max: [40.0, 40.0, 40.0],
                            center: [0.0, 0.0, 0.0],
                            axis_length: 0.2,
                        }),
                    },
                ),
                (
                    HumanBodyBone::Head,
                    HumanBone {
                        node: n(HEAD),
                        limit: None,
                    },
                ),
                (
                    HumanBodyBone::LeftHand,
                    HumanBone {
                        node: n(LEFT_HAND),
                        limit: None,
                    },
                ),
                (
                    HumanBodyBone::RightHand,
                    HumanBone {
                        node: n(RIGHT_HAND),
                        limit: None,
                    },
                ),
                (
                    HumanBodyBone::LeftEye,
                    HumanBone {
                        node: n(LEFT_EYE),
                        limit: None,
                    },
                ),
                (
                    HumanBodyBone::RightEye,
                    HumanBone {
                        node: n(RIGHT_EYE),
                        limit: None,
                    },
                ),
            ]),
            skeleton: vec![
                SkeletonBone {
                    node: n(HIPS),
                    position: [0.0, 0.9, 0.0],
                    rotation: IDENTITY,
                    scale: [1.0; 3],
                },
                SkeletonBone {
                    node: n(LEFT_HAND),
                    position: [0.5, 0.0, 0.0],
                    rotation: [0.0, 0.0, 0.258_819, 0.965_926],
                    scale: [1.0; 3],
                },
            ],
            arm_stretch: 0.05,
            leg_stretch: 0.05,
            upper_arm_twist: 0.5,
            lower_arm_twist: 0.5,
            upper_leg_twist: 0.5,
            lower_leg_twist: 0.5,
            feet_spacing: 0.0,
            has_translation_do_f: false,
        },
        descriptor: Descriptor {
            view_position: [0.0, 1.5, 0.08],
            lip_sync: LipSync::VisemeBlendShape {
                mesh: n(BODY),
                visemes: Visemes {
                    sil: "vrc.v_sil".into(),
                    pp: "vrc.v_pp".into(),
                    ff: "vrc.v_ff".into(),
                    th: "vrc.v_th".into(),
                    dd: "vrc.v_dd".into(),
                    kk: "vrc.v_kk".into(),
                    ch: "vrc.v_ch".into(),
                    ss: "vrc.v_ss".into(),
                    nn: "vrc.v_nn".into(),
                    rr: "vrc.v_rr".into(),
                    aa: "vrc.v_aa".into(),
                    e: "vrc.v_e".into(),
                    ih: "vrc.v_ih".into(),
                    oh: "vrc.v_oh".into(),
                    ou: "vrc.v_ou".into(),
                },
            },
            eyes: Eyes {
                look: Some(EyeLook {
                    left_eye: Some(n(LEFT_EYE)),
                    right_eye: Some(n(RIGHT_EYE)),
                    straight: EyeRotations {
                        linked: true,
                        left: IDENTITY,
                        right: IDENTITY,
                    },
                    up: EyeRotations {
                        linked: true,
                        left: [-0.130_526, 0.0, 0.0, 0.991_445],
                        right: [-0.130_526, 0.0, 0.0, 0.991_445],
                    },
                    down: EyeRotations {
                        linked: true,
                        left: [0.130_526, 0.0, 0.0, 0.991_445],
                        right: [0.130_526, 0.0, 0.0, 0.991_445],
                    },
                    left: EyeRotations {
                        linked: false,
                        left: [0.0, -0.173_648, 0.0, 0.984_808],
                        right: [0.0, -0.130_526, 0.0, 0.991_445],
                    },
                    right: EyeRotations {
                        linked: false,
                        left: [0.0, 0.130_526, 0.0, 0.991_445],
                        right: [0.0, 0.173_648, 0.0, 0.984_808],
                    },
                }),
                eyelids: Eyelids::BlendShapes {
                    mesh: n(BODY),
                    blink: Some("Blink".into()),
                    looking_up: None,
                    looking_down: Some("LookDown".into()),
                },
                eye_movement: EyeMovement {
                    confidence: 0.5,
                    excitement: 0.25,
                },
            },
        },
        renderers: vec![Renderer {
            node: n(BODY),
            enabled: true,
            materials: vec![MaterialIndex(0), MaterialIndex(1)],
        }],
        nodes: (0..NODE_COUNT)
            .map(|i| Node { active: i != PROP })
            .collect(),
        materials: [vec![fur, glass, face], others.to_vec()].concat(),
        physbones: vec![
            PhysBone {
                node: n(HAIR),
                enabled: true,
                version: PhysBoneVersion::V1_1,
                root: n(HAIR),
                ignore_transforms: vec![],
                endpoint_position: [0.0, -0.1, 0.0],
                multi_child_type: MultiChildType::Ignore,
                integration_type: IntegrationType::Advanced,
                pull: curved(0.2),
                spring: flat(0.4),
                stiffness: flat(0.1),
                gravity: curved(0.3),
                gravity_falloff: flat(0.5),
                immobile_type: ImmobileType::World,
                immobile: flat(0.6),
                limit: Limit::Polar {
                    max_angle_x: curved(45.0),
                    max_angle_z: flat(30.0),
                    rotation: LimitRotation {
                        value: [0.0, 10.0, 0.0],
                        x_curve: None,
                        y_curve: Some(curved(1.0).curve.unwrap()),
                        z_curve: None,
                    },
                },
                radius: curved(0.02),
                max_stretch: flat(0.1),
                stretch_motion: flat(0.5),
                max_squish: flat(0.2),
                colliders: vec![ColliderIndex(0), ColliderIndex(1), ColliderIndex(2)],
                collision: Permission {
                    allow_self: true,
                    allow_others: false,
                },
                grabbing: Permission {
                    allow_self: true,
                    allow_others: true,
                },
                posing: Permission {
                    allow_self: false,
                    allow_others: false,
                },
                grab_movement: 0.5,
                snap_to_hand: true,
                parameter: Some("Hair".into()),
                is_animated: false,
                reset_when_disabled: true,
            },
            PhysBone {
                node: n(SPINE),
                enabled: false,
                version: PhysBoneVersion::V1_0,
                root: n(HEAD),
                ignore_transforms: vec![n(LEFT_EYE), n(RIGHT_EYE)],
                endpoint_position: [0.0; 3],
                multi_child_type: MultiChildType::Average,
                integration_type: IntegrationType::Simplified,
                pull: flat(0.2),
                spring: flat(0.2),
                stiffness: flat(0.2),
                gravity: flat(0.0),
                gravity_falloff: flat(0.0),
                immobile_type: ImmobileType::AllMotion,
                immobile: flat(0.0),
                limit: Limit::Hinge {
                    max_angle_x: flat(20.0),
                    rotation: LimitRotation {
                        value: [0.0; 3],
                        x_curve: None,
                        y_curve: None,
                        z_curve: None,
                    },
                },
                radius: flat(0.0),
                max_stretch: flat(0.0),
                stretch_motion: flat(0.0),
                max_squish: flat(0.0),
                colliders: vec![],
                collision: Permission {
                    allow_self: false,
                    allow_others: true,
                },
                grabbing: Permission {
                    allow_self: false,
                    allow_others: false,
                },
                posing: Permission {
                    allow_self: true,
                    allow_others: true,
                },
                grab_movement: 0.0,
                snap_to_hand: false,
                parameter: None,
                is_animated: true,
                reset_when_disabled: false,
            },
            PhysBone {
                limit: Limit::Angle {
                    max_angle_x: flat(60.0),
                    rotation: LimitRotation {
                        value: [0.0; 3],
                        x_curve: None,
                        y_curve: None,
                        z_curve: None,
                    },
                },
                multi_child_type: MultiChildType::First,
                ..PhysBone {
                    node: n(PROP),
                    enabled: true,
                    version: PhysBoneVersion::V1_1,
                    root: n(PROP),
                    ignore_transforms: vec![],
                    endpoint_position: [0.0; 3],
                    multi_child_type: MultiChildType::Ignore,
                    integration_type: IntegrationType::Simplified,
                    pull: flat(0.2),
                    spring: flat(0.2),
                    stiffness: flat(0.2),
                    gravity: flat(0.0),
                    gravity_falloff: flat(0.0),
                    immobile_type: ImmobileType::AllMotion,
                    immobile: flat(0.0),
                    limit: Limit::None {},
                    radius: flat(0.0),
                    max_stretch: flat(0.0),
                    stretch_motion: flat(0.0),
                    max_squish: flat(0.0),
                    colliders: vec![],
                    collision: Permission {
                        allow_self: true,
                        allow_others: true,
                    },
                    grabbing: Permission {
                        allow_self: true,
                        allow_others: true,
                    },
                    posing: Permission {
                        allow_self: true,
                        allow_others: true,
                    },
                    grab_movement: 0.0,
                    snap_to_hand: false,
                    parameter: None,
                    is_animated: false,
                    reset_when_disabled: false,
                }
            },
        ],
        colliders: vec![
            Collider {
                node: n(HEAD),
                shape: ColliderShape::Sphere {
                    radius: 0.1,
                    inside_bounds: false,
                },
                position: [0.0, 0.05, 0.0],
                rotation: IDENTITY,
                bones_as_spheres: false,
            },
            Collider {
                node: n(SPINE),
                shape: ColliderShape::Capsule {
                    radius: 0.12,
                    height: 0.4,
                    inside_bounds: true,
                },
                position: [0.0; 3],
                rotation: [
                    0.0,
                    0.0,
                    std::f32::consts::FRAC_1_SQRT_2,
                    std::f32::consts::FRAC_1_SQRT_2,
                ],
                bones_as_spheres: true,
            },
            Collider {
                node: n(HIPS),
                shape: ColliderShape::Plane {},
                position: [0.0; 3],
                rotation: IDENTITY,
                bones_as_spheres: false,
            },
        ],
        contacts: Contacts {
            senders: vec![ContactSender {
                node: n(LEFT_HAND),
                enabled: true,
                shape: ContactShape::Sphere { radius: 0.05 },
                position: [0.0; 3],
                rotation: IDENTITY,
                collision_tags: vec!["Hand".into(), "Boop".into()],
            }],
            receivers: vec![
                ContactReceiver {
                    node: n(HEAD),
                    enabled: true,
                    shape: ContactShape::Capsule {
                        radius: 0.05,
                        height: 0.2,
                    },
                    position: [0.0, 0.1, 0.1],
                    rotation: IDENTITY,
                    collision_tags: vec!["Boop".into()],
                    allow_self: true,
                    allow_others: true,
                    local_only: false,
                    receiver_type: ReceiverType::Proximity {},
                    parameter: "Boop".into(),
                },
                ContactReceiver {
                    node: n(HEAD),
                    enabled: false,
                    shape: ContactShape::Sphere { radius: 0.2 },
                    position: [0.0; 3],
                    rotation: IDENTITY,
                    collision_tags: vec!["Head".into()],
                    allow_self: false,
                    allow_others: true,
                    local_only: true,
                    receiver_type: ReceiverType::OnEnter { min_velocity: 0.05 },
                    parameter: "Pat".into(),
                },
                ContactReceiver {
                    node: n(HEAD),
                    enabled: true,
                    shape: ContactShape::Sphere { radius: 0.2 },
                    position: [0.0; 3],
                    rotation: IDENTITY,
                    collision_tags: vec!["Hand".into()],
                    allow_self: true,
                    allow_others: false,
                    local_only: true,
                    receiver_type: ReceiverType::Constant {},
                    parameter: "Touch".into(),
                },
            ],
        },
        constraints: vec![
            Constraint {
                node: n(PROP),
                enabled: true,
                is_active: true,
                target: n(PROP),
                weight: 1.0,
                sources: vec![
                    ConstraintSource {
                        position_offset: [0.0, 0.05, 0.0],
                        rotation_offset: [0.0, 90.0, 0.0],
                        ..source(RIGHT_HAND)
                    },
                    ConstraintSource {
                        source: SourceTarget::World([
                            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0,
                            0.0, 1.0,
                        ]),
                        weight: 0.0,
                        position_offset: [0.0; 3],
                        rotation_offset: [0.0; 3],
                    },
                ],
                solve_in_local_space: false,
                freeze_to_world: false,
                rebake_offsets_when_unfrozen: false,
                kind: ConstraintKind::Parent {
                    position: channel(),
                    rotation: channel(),
                },
            },
            Constraint {
                node: n(HEAD),
                enabled: true,
                is_active: true,
                target: n(LEFT_EYE),
                weight: 0.5,
                sources: vec![source(RIGHT_HAND)],
                solve_in_local_space: true,
                freeze_to_world: false,
                rebake_offsets_when_unfrozen: false,
                kind: ConstraintKind::Aim {
                    rotation: channel(),
                    aim_axis: [0.0, 0.0, 1.0],
                    up_axis: [0.0, 1.0, 0.0],
                    world_up: WorldUp::ObjectRotationUp {
                        node: n(HEAD),
                        vector: [0.0, 1.0, 0.0],
                    },
                },
            },
            Constraint {
                node: n(RIGHT_EYE),
                enabled: false,
                is_active: true,
                target: n(RIGHT_EYE),
                weight: 1.0,
                sources: vec![source(LEFT_HAND)],
                solve_in_local_space: false,
                freeze_to_world: true,
                rebake_offsets_when_unfrozen: true,
                kind: ConstraintKind::LookAt {
                    rotation: channel(),
                    roll: 15.0,
                    world_up_node: Some(n(HEAD)),
                },
            },
            Constraint {
                node: n(HAIR_TIP),
                enabled: true,
                is_active: true,
                target: n(HAIR_TIP),
                weight: 1.0,
                sources: vec![source(HEAD)],
                solve_in_local_space: false,
                freeze_to_world: false,
                rebake_offsets_when_unfrozen: false,
                kind: ConstraintKind::Position {
                    position: channel(),
                },
            },
            Constraint {
                node: n(HAIR_TIP),
                enabled: true,
                is_active: true,
                target: n(HAIR_TIP),
                weight: 1.0,
                sources: vec![source(HEAD)],
                solve_in_local_space: false,
                freeze_to_world: false,
                rebake_offsets_when_unfrozen: false,
                kind: ConstraintKind::Rotation {
                    rotation: channel(),
                },
            },
            Constraint {
                node: n(PROP),
                enabled: true,
                is_active: true,
                target: n(PROP),
                weight: 1.0,
                sources: vec![source(HEAD)],
                solve_in_local_space: false,
                freeze_to_world: false,
                rebake_offsets_when_unfrozen: false,
                kind: ConstraintKind::Scale { scale: channel() },
            },
            Constraint {
                node: n(HEAD),
                enabled: true,
                is_active: true,
                target: n(HEAD),
                weight: 1.0,
                sources: vec![source(HIPS)],
                solve_in_local_space: false,
                freeze_to_world: false,
                rebake_offsets_when_unfrozen: false,
                kind: ConstraintKind::Aim {
                    rotation: channel(),
                    aim_axis: [0.0, 0.0, 1.0],
                    up_axis: [0.0, 1.0, 0.0],
                    world_up: WorldUp::SceneUp,
                },
            },
            Constraint {
                node: n(HEAD),
                enabled: true,
                is_active: true,
                target: n(HEAD),
                weight: 1.0,
                sources: vec![source(HIPS)],
                solve_in_local_space: false,
                freeze_to_world: false,
                rebake_offsets_when_unfrozen: false,
                kind: ConstraintKind::Aim {
                    rotation: channel(),
                    aim_axis: [0.0, 0.0, 1.0],
                    up_axis: [0.0, 1.0, 0.0],
                    world_up: WorldUp::ObjectUp { node: n(SPINE) },
                },
            },
            Constraint {
                node: n(HEAD),
                enabled: true,
                is_active: true,
                target: n(HEAD),
                weight: 1.0,
                sources: vec![source(HIPS)],
                solve_in_local_space: false,
                freeze_to_world: false,
                rebake_offsets_when_unfrozen: false,
                kind: ConstraintKind::Aim {
                    rotation: channel(),
                    aim_axis: [0.0, 0.0, 1.0],
                    up_axis: [0.0, 1.0, 0.0],
                    world_up: WorldUp::Vector {
                        vector: [0.0, 1.0, 0.0],
                    },
                },
            },
            Constraint {
                node: n(HEAD),
                enabled: true,
                is_active: true,
                target: n(HEAD),
                weight: 1.0,
                sources: vec![source(HIPS)],
                solve_in_local_space: false,
                freeze_to_world: false,
                rebake_offsets_when_unfrozen: false,
                kind: ConstraintKind::Aim {
                    rotation: channel(),
                    aim_axis: [0.0, 0.0, 1.0],
                    up_axis: [0.0, 1.0, 0.0],
                    world_up: WorldUp::None {},
                },
            },
        ],
        parameters: vec![
            Parameter {
                name: "Prop".into(),
                default: ParameterValue::Bool(false),
                saved: true,
                synced: true,
            },
            Parameter {
                name: "Outfit".into(),
                default: ParameterValue::Int(1),
                saved: true,
                synced: true,
            },
            Parameter {
                name: "Sweep".into(),
                default: ParameterValue::Float(0.5),
                saved: false,
                synced: false,
            },
        ],
        menu: Menu {
            controls: vec![
                control(
                    "Prop",
                    ControlKind::Toggle {
                        parameter: Some("Prop".into()),
                        value: 1.0,
                    },
                ),
                control(
                    "Wave",
                    ControlKind::Button {
                        parameter: Some("Outfit".into()),
                        value: 2.0,
                    },
                ),
                control(
                    "Sweep",
                    ControlKind::RadialPuppet {
                        parameter: None,
                        value: 0.0,
                        sub_parameter: Some("Sweep".into()),
                    },
                ),
                Control {
                    icon: Some(TextureIndex(0)),
                    ..control(
                        "More",
                        ControlKind::SubMenu {
                            parameter: Some("MenuOpen".into()),
                            value: 1.0,
                            sub_menu: Some(Menu {
                                controls: vec![
                                    control(
                                        "Puppet",
                                        ControlKind::TwoAxisPuppet {
                                            parameter: Some("PuppetOpen".into()),
                                            value: 1.0,
                                            sub_parameters: [
                                                Some("PuppetX".into()),
                                                Some("PuppetY".into()),
                                            ],
                                            labels: [
                                                label("Up"),
                                                label("Right"),
                                                label("Down"),
                                                label("Left"),
                                            ],
                                        },
                                    ),
                                    control(
                                        "Face",
                                        ControlKind::FourAxisPuppet {
                                            parameter: None,
                                            value: 0.0,
                                            sub_parameters: [
                                                Some("Smile".into()),
                                                None,
                                                Some("Frown".into()),
                                                None,
                                            ],
                                            labels: [
                                                Label {
                                                    icon: Some(TextureIndex(0)),
                                                    ..label("Smile")
                                                },
                                                label(""),
                                                label("Frown"),
                                                label(""),
                                            ],
                                        },
                                    ),
                                ],
                            }),
                        },
                    )
                },
            ],
        },
        layers: BTreeMap::from([
            (
                PlayableLayerType::Base,
                PlayableLayer {
                    controller: None,
                    mask: None,
                },
            ),
            (
                PlayableLayerType::Additive,
                PlayableLayer {
                    controller: None,
                    mask: None,
                },
            ),
            (
                PlayableLayerType::Gesture,
                PlayableLayer {
                    controller: Some(controller(
                        "Gesture",
                        vec![aparam("GestureLeft", AnimatorParameterValue::Int(0))],
                        vec![layer("Hands", LayerBlending::Override, gesture_machine)],
                    )),
                    mask: Some(AvatarMask {
                        body_parts: BTreeSet::from([BodyPart::LeftFingers, BodyPart::RightFingers]),
                        transforms: vec![],
                    }),
                },
            ),
            (
                PlayableLayerType::Action,
                PlayableLayer {
                    controller: None,
                    mask: None,
                },
            ),
            (
                PlayableLayerType::Fx,
                PlayableLayer {
                    controller: Some(controller(
                        "FX",
                        vec![
                            aparam("Prop", AnimatorParameterValue::Bool(false)),
                            aparam("Outfit", AnimatorParameterValue::Int(1)),
                            aparam("Sweep", AnimatorParameterValue::Float(0.5)),
                            aparam("Reset", AnimatorParameterValue::Trigger(false)),
                        ],
                        vec![layer("Props", LayerBlending::Override, fx_machine), synced],
                    )),
                    mask: None,
                },
            ),
            (
                PlayableLayerType::Sitting,
                PlayableLayer {
                    controller: None,
                    mask: None,
                },
            ),
            (
                PlayableLayerType::TPose,
                PlayableLayer {
                    controller: None,
                    mask: None,
                },
            ),
            (
                PlayableLayerType::IkPose,
                PlayableLayer {
                    controller: None,
                    mask: None,
                },
            ),
        ]),
        behaviours: vec![
            Behaviour::ParameterDriver {
                local_only: true,
                parameters: vec![
                    DriverEntry::Set {
                        name: "Outfit".into(),
                        value: 2.0,
                    },
                    DriverEntry::Add {
                        name: "Sweep".into(),
                        value: 0.1,
                    },
                    DriverEntry::Random {
                        name: "Prop".into(),
                        value_min: 0.0,
                        value_max: 1.0,
                        chance: 0.5,
                    },
                    DriverEntry::Copy {
                        source: "Sweep".into(),
                        name: "Outfit".into(),
                        convert_range: Some(ConvertRange {
                            source_min: 0.0,
                            source_max: 1.0,
                            dest_min: 0.0,
                            dest_max: 4.0,
                        }),
                    },
                    DriverEntry::Copy {
                        source: "Outfit".into(),
                        name: "Sweep".into(),
                        convert_range: None,
                    },
                ],
            },
            Behaviour::TrackingControl {
                head: TrackingType::Tracking,
                left_hand: TrackingType::Animation,
                right_hand: tracking,
                hip: tracking,
                left_foot: tracking,
                right_foot: tracking,
                left_fingers: tracking,
                right_fingers: tracking,
                eyes: TrackingType::Animation,
                mouth: tracking,
            },
            Behaviour::LocomotionControl {
                disable_locomotion: true,
            },
            Behaviour::PlayableLayerControl {
                layer: ControlledLayer::Action,
                goal_weight: 1.0,
                blend_duration: 0.25,
            },
            Behaviour::AnimatorLayerControl {
                playable: ControlledLayer::Fx,
                layer: 1,
                goal_weight: 0.0,
                blend_duration: 0.5,
            },
            Behaviour::TemporaryPoseSpace {
                enter_pose_space: true,
                fixed_delay: false,
                delay_time: 0.1,
            },
            Behaviour::PlayableLayerControl {
                layer: ControlledLayer::Gesture,
                goal_weight: 1.0,
                blend_duration: 0.0,
            },
            Behaviour::PlayableLayerControl {
                layer: ControlledLayer::Additive,
                goal_weight: 1.0,
                blend_duration: 0.0,
            },
        ],
        clips: vec![
            Clip {
                slot_curves: vec![SlotCurve {
                    renderer: n(BODY),
                    slot: 1,
                    keys: vec![SlotKey {
                        time: 0.0,
                        material: MaterialIndex(2),
                    }],
                }],
                ..clip(
                    "PropOff",
                    vec![
                        curve(
                            Binding::NodeActive { node: n(PROP) },
                            vec![Keyframe {
                                in_tangent: f32::INFINITY,
                                out_tangent: f32::NEG_INFINITY,
                                ..key(0.0, 0.0)
                            }],
                        ),
                        curve(
                            Binding::RendererEnabled { node: n(BODY) },
                            vec![key(0.0, 1.0)],
                        ),
                        curve(
                            Binding::MaterialProperty {
                                node: n(BODY),
                                property: "_Color.r".into(),
                            },
                            vec![key(0.0, 0.5)],
                        ),
                        curve(
                            Binding::PhysBone {
                                index: PhysBoneIndex(0),
                                field: "gravity".into(),
                            },
                            vec![key(0.0, 0.1)],
                        ),
                        curve(
                            Binding::Constraint {
                                index: ConstraintIndex(0),
                                field: "weight".into(),
                            },
                            vec![key(0.0, 0.0)],
                        ),
                        curve(
                            Binding::ContactSender {
                                index: ContactIndex(0),
                                field: "enabled".into(),
                            },
                            vec![key(0.0, 0.0)],
                        ),
                        curve(
                            Binding::ContactReceiver {
                                index: ContactIndex(1),
                                field: "enabled".into(),
                            },
                            vec![key(0.0, 1.0)],
                        ),
                        curve(
                            Binding::AnimatorParameter {
                                name: "Sweep".into(),
                            },
                            vec![key(0.0, 0.25)],
                        ),
                    ],
                )
            },
            Clip {
                length: 2.0,
                looping: true,
                ..clip(
                    "Wave",
                    vec![
                        curve(
                            Binding::Muscle {
                                name: "Left Arm Down-Up".into(),
                            },
                            vec![key(0.0, 0.0), key(1.0, 1.0), key(2.0, 0.0)],
                        ),
                        curve(
                            Binding::Transform {
                                node: n(HAIR),
                                property: TransformProperty::Translation,
                                component: Axis::Y,
                            },
                            vec![key(0.0, 0.0), key(2.0, 0.01)],
                        ),
                        curve(
                            Binding::Transform {
                                node: n(HEAD),
                                property: TransformProperty::Rotation,
                                component: Axis::W,
                            },
                            vec![key(0.0, 1.0)],
                        ),
                        curve(
                            Binding::Transform {
                                node: n(HEAD),
                                property: TransformProperty::EulerRotation,
                                component: Axis::X,
                            },
                            vec![key(0.0, 0.0)],
                        ),
                        curve(
                            Binding::Transform {
                                node: n(PROP),
                                property: TransformProperty::Scale,
                                component: Axis::Z,
                            },
                            vec![key(0.0, 1.0)],
                        ),
                    ],
                )
            },
            clip(
                "Smile",
                vec![curve(
                    Binding::BlendShape {
                        node: n(BODY),
                        name: "vrc.v_aa".into(),
                    },
                    vec![key(0.0, 100.0)],
                )],
            ),
            clip(
                "Frown",
                vec![curve(
                    Binding::BlendShape {
                        node: n(BODY),
                        name: "LookDown".into(),
                    },
                    vec![key(0.0, 100.0)],
                )],
            ),
            clip(
                "Down",
                vec![curve(
                    Binding::BlendShape {
                        node: n(BODY),
                        name: "Blink".into(),
                    },
                    vec![key(0.0, 50.0)],
                )],
            ),
            clip("Left", vec![]),
        ],
        unsupported: vec![
            Unsupported {
                kind: "ParticleSystem".into(),
                count: 2,
            },
            Unsupported {
                kind: "AudioSource".into(),
                count: 1,
            },
            Unsupported {
                kind: "VRCAnimatorPlayAudio".into(),
                count: 1,
            },
        ],
    }
}

struct Bin(Vec<u8>);

impl Bin {
    fn view(&mut self, bytes: &[u8], target: Option<u32>) -> Value {
        while !self.0.len().is_multiple_of(4) {
            self.0.push(0);
        }
        let offset = self.0.len();
        self.0.extend_from_slice(bytes);
        let mut view = json!({ "buffer": 0, "byteOffset": offset, "byteLength": bytes.len() });
        if let Some(target) = target {
            view["target"] = target.into();
        }
        view
    }
}

fn f32s(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// A skinned two-slot triangle with the 15 visemes and three more blendshapes, posed on the
/// humanoid nodes the model names.
fn gltf(materials: usize) -> (serde_json::Map<String, Value>, Vec<u8>) {
    const ARRAY_BUFFER: u32 = 34962;
    const ELEMENT_ARRAY_BUFFER: u32 = 34963;
    const FLOAT: u32 = 5126;
    const UNSIGNED_BYTE: u32 = 5121;
    const UNSIGNED_SHORT: u32 = 5123;

    let positions = [0.0, 1.0, 0.0, -0.2, 0.8, 0.0, 0.2, 0.8, 0.0];
    let normals = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0];
    let mut target_names = vec![
        "vrc.v_sil",
        "vrc.v_pp",
        "vrc.v_ff",
        "vrc.v_th",
        "vrc.v_dd",
        "vrc.v_kk",
        "vrc.v_ch",
        "vrc.v_ss",
        "vrc.v_nn",
        "vrc.v_rr",
        "vrc.v_aa",
        "vrc.v_e",
        "vrc.v_ih",
        "vrc.v_oh",
        "vrc.v_ou",
    ];
    target_names.extend(["Blink", "LookDown", "Smile"]);

    let mut bin = Bin(Vec::new());
    let mut views = vec![
        bin.view(&f32s(&positions), Some(ARRAY_BUFFER)),
        bin.view(&f32s(&normals), Some(ARRAY_BUFFER)),
        bin.view(&[3, 4, 5, 0, 3, 4, 0, 0, 2, 0, 0, 0], Some(ARRAY_BUFFER)),
        bin.view(
            &f32s(&[0.5, 0.5, 0.0, 0.0, 0.7, 0.3, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0]),
            Some(ARRAY_BUFFER),
        ),
        bin.view(&[0, 0, 1, 0, 2, 0], Some(ELEMENT_ARRAY_BUFFER)),
    ];
    let mut accessors = vec![
        json!({ "bufferView": 0, "componentType": FLOAT, "count": 3, "type": "VEC3",
                "min": [-0.2, 0.8, 0.0], "max": [0.2, 1.0, 0.0] }),
        json!({ "bufferView": 1, "componentType": FLOAT, "count": 3, "type": "VEC3" }),
        json!({ "bufferView": 2, "componentType": UNSIGNED_BYTE, "count": 3, "type": "VEC4" }),
        json!({ "bufferView": 3, "componentType": FLOAT, "count": 3, "type": "VEC4" }),
        json!({ "bufferView": 4, "componentType": UNSIGNED_SHORT, "count": 3, "type": "SCALAR" }),
    ];
    let mut targets = Vec::new();
    for (i, _) in target_names.iter().enumerate() {
        let d = 0.001 * (i as f32 + 1.0);
        let moved = [0.0, d, 0.0, 0.0, -d, 0.0, 0.0, -d, 0.0];
        let turned = [0.0, d, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        views.push(bin.view(&f32s(&moved), Some(ARRAY_BUFFER)));
        accessors.push(
            json!({ "bufferView": views.len() - 1, "componentType": FLOAT, "count": 3,
                               "type": "VEC3", "min": [0.0, -d, 0.0], "max": [0.0, d, 0.0] }),
        );
        views.push(bin.view(&f32s(&turned), Some(ARRAY_BUFFER)));
        accessors.push(
            json!({ "bufferView": views.len() - 1, "componentType": FLOAT, "count": 3,
                               "type": "VEC3" }),
        );
        targets.push(json!({ "POSITION": accessors.len() - 2, "NORMAL": accessors.len() - 1 }));
    }
    let joints = [HIPS, SPINE, HEAD, HAIR, HAIR_TIP, LEFT_HAND];
    let translations: [[f32; 3]; 6] = [
        [0.0, -0.9, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, -1.4, 0.0],
        [0.0, -1.5, 0.1],
        [0.0, -1.3, 0.1],
        [-0.5, -1.2, 0.0],
    ];
    let inverse_binds: Vec<f32> = translations
        .iter()
        .flat_map(|[x, y, z]| {
            [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, *x, *y, *z, 1.0,
            ]
        })
        .collect();
    views.push(bin.view(&f32s(&inverse_binds), None));
    accessors.push(
        json!({ "bufferView": views.len() - 1, "componentType": FLOAT,
                           "count": joints.len(), "type": "MAT4" }),
    );
    let inverse_bind_accessor = accessors.len() - 1;
    let texture_png: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
        0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8,
        0xcf, 0xc0, 0xf0, 0x1f, 0x00, 0x05, 0x00, 0x01, 0xff, 0x89, 0x99, 0x3d, 0x1d, 0x00, 0x00,
        0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];
    views.push(bin.view(texture_png, None));
    let image_view = views.len() - 1;
    while !bin.0.len().is_multiple_of(4) {
        bin.0.push(0);
    }

    let attributes = json!({ "POSITION": 0, "NORMAL": 1, "JOINTS_0": 2, "WEIGHTS_0": 3 });
    let primitive = |material: usize| json!({ "attributes": attributes, "indices": 4, "material": material, "targets": targets });
    let node = |name: &str, translation: [f32; 3], children: &[u32]| {
        let mut node = json!({ "name": name, "translation": translation });
        if !children.is_empty() {
            node["children"] = json!(children);
        }
        node
    };
    let mut body = node("Body", [0.0; 3], &[]);
    body["mesh"] = 0.into();
    body["skin"] = 0.into();

    let document = json!({
        "asset": { "version": "2.0", "generator": "avatar-core synthetic fixture" },
        "scene": 0,
        "scenes": [{ "nodes": [AVATAR] }],
        "nodes": [
            node("Avatar", [0.0; 3], &[BODY, HIPS]),
            body,
            node("Hips", [0.0, 0.9, 0.0], &[SPINE]),
            node("Spine", [0.0, 0.1, 0.0], &[HEAD, LEFT_HAND, RIGHT_HAND]),
            node("Head", [0.0, 0.4, 0.0], &[HAIR, PROP, LEFT_EYE, RIGHT_EYE]),
            node("Hair", [0.0, 0.1, 0.1], &[HAIR_TIP]),
            node("HairTip", [0.0, -0.2, 0.0], &[]),
            node("Prop", [0.0, 0.2, 0.0], &[]),
            node("LeftHand", [0.5, 0.2, 0.0], &[]),
            node("RightHand", [-0.5, 0.2, 0.0], &[]),
            node("LeftEye", [0.03, 0.05, 0.08], &[]),
            node("RightEye", [-0.03, 0.05, 0.08], &[]),
        ],
        "meshes": [{
            "name": "Body",
            "primitives": [primitive(0), primitive(1)],
            "weights": vec![0.0; target_names.len()],
            "extras": { "targetNames": target_names },
        }],
        "skins": [{ "joints": joints, "inverseBindMatrices": inverse_bind_accessor, "skeleton": HIPS }],
        "materials": (0..materials)
            .map(|i| json!({ "name": format!("Material{i}"), "pbrMetallicRoughness": { "baseColorTexture": { "index": 0 } } }))
            .collect::<Vec<_>>(),
        "textures": [{ "source": 0, "sampler": 0 }],
        "samplers": [{ "magFilter": 9729, "minFilter": 9987 }],
        "images": [{ "bufferView": image_view, "mimeType": "image/png" }],
        "accessors": accessors,
        "bufferViews": views,
        "buffers": [{ "byteLength": bin.0.len() }],
    });
    let Value::Object(document) = document else {
        unreachable!()
    };
    (document, bin.0)
}

pub fn avatar() -> Avatar {
    let model = model();
    assert_eq!(model.nodes.len(), NODE_COUNT as usize);
    let (gltf, bin) = gltf(model.materials.len());
    Avatar { gltf, bin, model }
}
