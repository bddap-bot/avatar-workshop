using System.Collections.Generic;
using System.Linq;
using UnityEditor;
using UnityEditor.Animations;
using UnityEngine;
using VRC.Dynamics;
using VRC.SDK3.Avatars.Components;
using VRC.SDK3.Avatars.ScriptableObjects;
using VRC.SDK3.Dynamics.Constraint.Components;
using VRC.SDKBase;

namespace AvatarWorkshop.Reference
{
    public static class Coverage
    {
        public static readonly string[] Required =
        {
            "physbone.immobile.all_motion", "physbone.immobile.world",
            "physbone.limit.none", "physbone.limit.angle", "physbone.limit.hinge", "physbone.limit.polar",
            "physbone.gravity_falloff", "physbone.stretch", "physbone.squish", "physbone.curve",
            "physbone.version.1_0", "physbone.version.1_1", "physbone.integration.simplified", "physbone.integration.advanced",
            "physbone.multichild.ignore", "physbone.multichild.first", "physbone.multichild.average", "physbone.endpoint",
            "collider.sphere", "collider.capsule", "collider.plane", "collider.inside",
            "contact.sender", "contact.receiver",
            "constraint.parent", "constraint.position", "constraint.rotation", "constraint.scale", "constraint.aim", "constraint.look_at", "constraint.freeze_to_world",
            "menu.button", "menu.toggle", "menu.submenu", "menu.two_axis", "menu.four_axis", "menu.radial",
            "transition.fixed_duration", "transition.normalized_duration",
            "blendtree.simple_1d", "blendtree.simple_directional_2d", "blendtree.freeform_directional_2d", "blendtree.freeform_cartesian_2d", "blendtree.direct",
            "driver.set", "driver.add", "driver.random", "driver.copy", "driver.local_only",
            "write_defaults.on", "write_defaults.off",
            "playable.base", "playable.additive", "playable.gesture", "playable.action", "playable.fx", "playable.sitting", "playable.t_pose", "playable.ik_pose",
            "behaviour.tracking", "behaviour.locomotion", "behaviour.playable_layer", "behaviour.animator_layer", "behaviour.temporary_pose_space",
            "mask.body_part", "mask.transform", "layer.additive", "layer.synced", "parameter.trigger",
            "submachine.entry", "submachine.exit", "transition.exit_time_offset",
            "state.speed_parameter", "state.time_parameter", "state.cycle_offset_parameter", "state.mirror_parameter",
            "curve.animator_parameter", "curve.muscle", "curve.constraint", "curve.contact_sender", "curve.contact_receiver", "curve.stepped", "curve.weighted", "curve.euler", "curve.material_swap",
            "clip.cycle_offset", "clip.additive_reference_pose", "clip.mirror", "clip.loop_pose",
            "clip.root_rotation.bake", "clip.root_rotation.original", "clip.root_rotation.body", "clip.root_rotation.offset",
            "clip.root_height.bake", "clip.root_height.original", "clip.root_height.center_of_mass", "clip.root_height.feet", "clip.root_height.offset",
            "clip.root_xz.bake", "clip.root_xz.original", "clip.root_xz.center_of_mass",
        };

        public static SortedDictionary<string, int> Count(GameObject root)
        {
            var c = new SortedDictionary<string, int>();
            foreach (var k in Required) c[k] = 0;
            void Add(string k) => c[k] = c.TryGetValue(k, out var n) ? n + 1 : 1;

            foreach (var pb in root.GetComponentsInChildren<VRCPhysBoneBase>(true))
            {
                if (pb.immobile > 0) Add(pb.immobileType == VRCPhysBoneBase.ImmobileType.World ? "physbone.immobile.world" : "physbone.immobile.all_motion");
                Add("physbone.limit." + pb.limitType.ToString().ToLowerInvariant());
                if (pb.gravity != 0 && pb.gravityFalloff > 0) Add("physbone.gravity_falloff");
                if (pb.maxStretch > 0) Add("physbone.stretch");
                if (pb.maxSquish > 0) Add("physbone.squish");
                var curves = new[] { pb.pullCurve, pb.springCurve, pb.stiffnessCurve, pb.gravityCurve, pb.gravityFalloffCurve, pb.immobileCurve, pb.radiusCurve,
                    pb.maxAngleXCurve, pb.maxAngleZCurve, pb.limitRotationXCurve, pb.limitRotationYCurve, pb.limitRotationZCurve, pb.maxStretchCurve, pb.maxSquishCurve, pb.stretchMotionCurve };
                if (curves.Any(k => k != null && k.length > 0)) Add("physbone.curve");
                Add(pb.version == VRCPhysBoneBase.Version.Version_1_0 ? "physbone.version.1_0" : "physbone.version.1_1");
                Add("physbone.integration." + pb.integrationType.ToString().ToLowerInvariant());
                var start = pb.rootTransform != null ? pb.rootTransform : pb.transform;
                if (start.childCount > 1) Add("physbone.multichild." + pb.multiChildType.ToString().ToLowerInvariant());
                if (pb.endpointPosition != Vector3.zero) Add("physbone.endpoint");
            }
            foreach (var col in root.GetComponentsInChildren<VRCPhysBoneColliderBase>(true))
            {
                Add("collider." + col.shapeType.ToString().ToLowerInvariant());
                if (col.insideBounds) Add("collider.inside");
            }
            foreach (var s in root.GetComponentsInChildren<ContactSender>(true)) Add("contact.sender");
            foreach (var r in root.GetComponentsInChildren<ContactReceiver>(true)) { Add("contact.receiver"); Add("contact.receiver." + r.receiverType.ToString().ToLowerInvariant()); }
            foreach (var k in root.GetComponentsInChildren<VRCConstraintBase>(true))
            {
                switch (k)
                {
                    case VRCParentConstraint _: Add("constraint.parent"); break;
                    case VRCPositionConstraint _: Add("constraint.position"); break;
                    case VRCRotationConstraint _: Add("constraint.rotation"); break;
                    case VRCScaleConstraint _: Add("constraint.scale"); break;
                    case VRCAimConstraint _: Add("constraint.aim"); break;
                    case VRCLookAtConstraint _: Add("constraint.look_at"); break;
                }
                if (k.FreezeToWorld) Add("constraint.freeze_to_world");
            }

            var d = root.GetComponent<VRCAvatarDescriptor>();
            if (d == null) return c;
            var seen = new HashSet<VRCExpressionsMenu>();
            void Menu(VRCExpressionsMenu m)
            {
                if (m == null || !seen.Add(m)) return;
                foreach (var ctl in m.controls)
                {
                    Add("menu." + Snake(ctl.type.ToString().Replace("Puppet", "")).Replace("sub_menu", "submenu"));
                    if (ctl.type == VRCExpressionsMenu.Control.ControlType.SubMenu) Menu(ctl.subMenu);
                }
            }
            Menu(d.expressionsMenu);

            var custom = d.baseAnimationLayers.Concat(d.specialAnimationLayers ?? new VRCAvatarDescriptor.CustomAnimLayer[0])
                .Where(l => !l.isDefault && l.animatorController is AnimatorController).ToList();
            foreach (var l in custom)
            {
                Add("playable." + Playable[l.type]);
                if (l.mask != null) Mask(l.mask, Add);
            }
            foreach (var ctrl in custom.Select(l => (AnimatorController)l.animatorController).Distinct())
            {
                var parameters = new HashSet<string>(ctrl.parameters.Select(p => p.name));
                var triggers = new HashSet<string>(ctrl.parameters.Where(p => p.type == AnimatorControllerParameterType.Trigger).Select(p => p.name));
                foreach (var layer in ctrl.layers)
                {
                    if (layer.avatarMask != null) Mask(layer.avatarMask, Add);
                    if (layer.blendingMode == AnimatorLayerBlendingMode.Additive) Add("layer.additive");
                    if (layer.syncedLayerIndex >= 0) Add("layer.synced");
                    else Machine(layer.stateMachine, null, parameters, triggers, Add);
                }
                foreach (var clip in ctrl.animationClips.Distinct()) Curves(clip, parameters, Add);
            }
            return c;
        }

        static readonly Dictionary<VRCAvatarDescriptor.AnimLayerType, string> Playable = new Dictionary<VRCAvatarDescriptor.AnimLayerType, string>
        {
            { VRCAvatarDescriptor.AnimLayerType.Base, "base" }, { VRCAvatarDescriptor.AnimLayerType.Additive, "additive" },
            { VRCAvatarDescriptor.AnimLayerType.Gesture, "gesture" }, { VRCAvatarDescriptor.AnimLayerType.Action, "action" },
            { VRCAvatarDescriptor.AnimLayerType.FX, "fx" }, { VRCAvatarDescriptor.AnimLayerType.Sitting, "sitting" },
            { VRCAvatarDescriptor.AnimLayerType.TPose, "t_pose" }, { VRCAvatarDescriptor.AnimLayerType.IKPose, "ik_pose" },
        };

        static void Mask(AvatarMask m, System.Action<string> add)
        {
            if (Enumerable.Range(0, m.transformCount).Any(i => !m.GetTransformActive(i))) add("mask.transform");
            for (var p = AvatarMaskBodyPart.Root; p < AvatarMaskBodyPart.LastBodyPart; p++)
                if (!m.GetHumanoidBodyPartActive(p)) { add("mask.body_part"); break; }
        }

        static void Curves(AnimationClip clip, HashSet<string> parameters, System.Action<string> add)
        {
            foreach (var b in AnimationUtility.GetCurveBindings(clip))
            {
                if (b.type == typeof(Animator) && b.path == "") add(parameters.Contains(b.propertyName) ? "curve.animator_parameter" : "curve.muscle");
                if (typeof(VRCConstraintBase).IsAssignableFrom(b.type)) add("curve.constraint");
                if (typeof(ContactSender).IsAssignableFrom(b.type)) add("curve.contact_sender");
                if (typeof(ContactReceiver).IsAssignableFrom(b.type)) add("curve.contact_receiver");
                if (b.type == typeof(Transform) && b.propertyName.Contains("EulerAngles")) add("curve.euler");
                var keys = AnimationUtility.GetEditorCurve(clip, b).keys;
                if (keys.Any(k => float.IsInfinity(k.inTangent) || float.IsInfinity(k.outTangent))) add("curve.stepped");
                if (keys.Any(k => k.weightedMode != WeightedMode.None)) add("curve.weighted");
            }
            foreach (var b in AnimationUtility.GetObjectReferenceCurveBindings(clip))
                if (b.propertyName.StartsWith("m_Materials.Array.data[")) add("curve.material_swap");
            var s = AnimationUtility.GetAnimationClipSettings(clip);
            if (s.cycleOffset != 0) add("clip.cycle_offset");
            if (s.hasAdditiveReferencePose && s.additiveReferencePoseClip != null) add("clip.additive_reference_pose");
            if (clip.humanMotion) Settings(s, add);
        }

        static void Settings(AnimationClipSettings s, System.Action<string> add)
        {
            if (s.mirror) add("clip.mirror");
            if (s.loopBlend) add("clip.loop_pose");
            if (s.loopBlendOrientation) add("clip.root_rotation.bake");
            add(s.keepOriginalOrientation ? "clip.root_rotation.original" : "clip.root_rotation.body");
            if (s.orientationOffsetY != 0) add("clip.root_rotation.offset");
            if (s.loopBlendPositionY) add("clip.root_height.bake");
            add(s.keepOriginalPositionY ? "clip.root_height.original" : s.heightFromFeet ? "clip.root_height.feet" : "clip.root_height.center_of_mass");
            if (s.level != 0) add("clip.root_height.offset");
            if (s.loopBlendPositionXZ) add("clip.root_xz.bake");
            add(s.keepOriginalPositionXZ ? "clip.root_xz.original" : "clip.root_xz.center_of_mass");
        }

        // The humanoid compositions the composition probe separates. Clip kinds: with IK-goal curves, without them,
        // and RootT.y alone; each root-motion setting on a clip whose body moves ("moving." and the setting); and
        // each context a clip of each kind plays in ("<context>+<kind>"). Every key also counts ".mirrored" where
        // its state mirrors. The Sitting playable's weight is partial while the harness toggles Seated through its
        // case faster than the playable blends.
        static readonly string[] Kinds = { "goal_curves", "no_goal_curves" };
        static readonly string[] Contexts =
        {
            "crossfade", "blendtree.1d", "blendtree.2d", "blendtree.direct", "layer.override_masked", "layer.additive_masked",
            "playable.action_partial", "playable.additive_partial", "playable.sitting_partial",
        };
        static readonly string[] Unmirrored = new[] { "clip.goal_curves", "clip.no_goal_curves", "clip.root_t_y_only",
                "blendtree.direct+root_t_y_only", "layer.override_masked+root_t_y_only", "blendtree.direct+root_t_y_only+layer.override_masked+under_additive" }
            .Concat(new[] { "clip.mirror", "clip.loop_pose", "clip.root_rotation.bake", "clip.root_rotation.original", "clip.root_rotation.body", "clip.root_rotation.offset",
                "clip.root_height.bake", "clip.root_height.original", "clip.root_height.center_of_mass", "clip.root_height.feet", "clip.root_height.offset",
                "clip.root_xz.bake", "clip.root_xz.original", "clip.root_xz.center_of_mass" }.Select(k => "moving." + k))
            .Concat(Contexts.SelectMany(c => Kinds.Select(k => c + "+" + k)))
            .ToArray();
        public static readonly string[] Compositions = Unmirrored.Concat(Unmirrored.Select(k => k + ".mirrored")).ToArray();

        public class ProbeCase
        {
            public int Id;
            public readonly List<string> States = new List<string>();
            public bool Sitting;
        }

        static IEnumerable<(VRCAvatarDescriptor.AnimLayerType type, AnimatorController controller)> Controllers(VRCAvatarDescriptor d) =>
            d.baseAnimationLayers.Concat(d.specialAnimationLayers ?? new VRCAvatarDescriptor.CustomAnimLayer[0])
                .Where(l => !l.isDefault && l.animatorController is AnimatorController).Select(l => (l.type, (AnimatorController)l.animatorController));

        // The Case value each state plays under: the one a transition into it requires, or its source state's.
        static Dictionary<AnimatorState, int> CaseOf(AnimatorStateMachine sm)
        {
            var of = new Dictionary<AnimatorState, int>();
            var states = sm.states.Select(s => s.state).ToList();
            foreach (var s in states)
                foreach (var t in s.transitions)
                    foreach (var c in t.conditions)
                        if (c.parameter == Fixture.CaseParameter && c.mode == AnimatorConditionMode.Equals && t.destinationState != null) of[t.destinationState] = (int)c.threshold;
            for (bool grew = true; grew;)
            {
                grew = false;
                foreach (var s in states.Where(of.ContainsKey).ToList())
                    foreach (var t in s.transitions)
                        if (t.destinationState != null && !of.ContainsKey(t.destinationState) && t.destinationState != sm.defaultState) { of[t.destinationState] = of[s]; grew = true; }
            }
            return of;
        }

        public static List<ProbeCase> Cases(VRCAvatarDescriptor d)
        {
            var cases = new SortedDictionary<int, ProbeCase>();
            foreach (var (type, ctrl) in Controllers(d))
                foreach (var layer in ctrl.layers)
                    foreach (var kv in CaseOf(layer.stateMachine))
                    {
                        if (!cases.TryGetValue(kv.Value, out var c)) cases[kv.Value] = c = new ProbeCase { Id = kv.Value };
                        c.States.Add($"{Playable[type]}/{layer.name}: {kv.Key.name}");
                        if (type == VRCAvatarDescriptor.AnimLayerType.Sitting) c.Sitting = true;
                    }
            return cases.Values.ToList();
        }

        static bool Masked(AvatarMask m)
        {
            if (m == null) return false;
            for (var p = AvatarMaskBodyPart.Root; p < AvatarMaskBodyPart.LastBodyPart; p++)
                if (!m.GetHumanoidBodyPartActive(p)) return true;
            return false;
        }

        static IEnumerable<AnimationClip> Leaves(Motion m) =>
            m is AnimationClip c ? new[] { c } : m is BlendTree t ? t.children.SelectMany(ch => Leaves(ch.motion)) : Enumerable.Empty<AnimationClip>();

        static readonly string[] GoalCurves = new[] { "LeftFoot", "RightFoot", "LeftHand", "RightHand" }.SelectMany(g => new[] { g + "T.", g + "Q." }).ToArray();

        static string Kind(AnimationClip clip, HashSet<string> parameters)
        {
            var names = AnimationUtility.GetCurveBindings(clip).Where(b => b.type == typeof(Animator) && b.path == "" && !parameters.Contains(b.propertyName)).Select(b => b.propertyName).ToList();
            if (names.Count == 0) return null;
            // Unity does not count a clip binding RootT.y alone as human motion, yet the animator applies it.
            if (names.Count == 1 && names[0] == "RootT.y") return "root_t_y_only";
            return names.Any(n => GoalCurves.Any(g => n.StartsWith(g))) ? "goal_curves" : "no_goal_curves";
        }

        static bool MovingBody(AnimationClip clip) => AnimationUtility.GetCurveBindings(clip)
            .Where(b => b.type == typeof(Animator) && b.path == "" && (b.propertyName.StartsWith("RootT.") || b.propertyName.StartsWith("RootQ.")))
            .Any(b => { var k = AnimationUtility.GetEditorCurve(clip, b).keys; return k.Any(x => x.value != k[0].value); });

        public static SortedDictionary<string, int> CountCompositions(VRCAvatarDescriptor d)
        {
            var c = new SortedDictionary<string, int>();
            foreach (var k in Compositions) c[k] = 0;
            foreach (var (type, ctrl) in Controllers(d))
            {
                var parameters = new HashSet<string>(ctrl.parameters.Select(p => p.name));
                var layers = ctrl.layers;
                for (int li = 0; li < layers.Length; li++)
                {
                    var layer = layers[li];
                    var of = CaseOf(layer.stateMachine);
                    foreach (var s in layer.stateMachine.states.Select(x => x.state))
                    {
                        var leaves = Leaves(s.motion).Select(l => (clip: l, kind: Kind(l, parameters))).Where(l => l.kind != null).ToList();
                        if (leaves.Count == 0) continue;
                        var mirrored = s.mirrorParameterActive && parameters.Contains(s.mirrorParameter);
                        void Add(string k, bool clipMirror = false)
                        {
                            c[k] = c.TryGetValue(k, out var n) ? n + 1 : 1;
                            if (mirrored || clipMirror) { var m = k + ".mirrored"; c[m] = c.TryGetValue(m, out var n2) ? n2 + 1 : 1; }
                        }
                        var contexts = new List<string>();
                        if (s.motion is BlendTree bt)
                            contexts.Add(bt.blendType == BlendTreeType.Simple1D ? "blendtree.1d" : bt.blendType == BlendTreeType.Direct ? "blendtree.direct" : "blendtree.2d");
                        var into = layer.stateMachine.states.Select(x => x.state).SelectMany(x => x.transitions.Select(t => (from: x, t)))
                            .Where(x => x.t.destinationState == s && x.t.duration > 0 && Leaves(x.from.motion).Any(l => Kind(l, parameters) != null));
                        if (into.Any()) contexts.Add("crossfade");
                        var masked = li > 0 && Masked(layer.avatarMask);
                        if (masked && layer.blendingMode == AnimatorLayerBlendingMode.Override) contexts.Add("layer.override_masked");
                        if (masked && layer.blendingMode == AnimatorLayerBlendingMode.Additive) contexts.Add("layer.additive_masked");
                        bool Partial(VRCPlayableLayerControl.BlendableLayer target) =>
                            s.behaviours.OfType<VRC_PlayableLayerControl>().Any(b => (int)b.layer == (int)target && b.goalWeight > 0 && b.goalWeight < 1);
                        if (type == VRCAvatarDescriptor.AnimLayerType.Action && Partial(VRCPlayableLayerControl.BlendableLayer.Action)) contexts.Add("playable.action_partial");
                        if (type == VRCAvatarDescriptor.AnimLayerType.Additive && Partial(VRCPlayableLayerControl.BlendableLayer.Additive)) contexts.Add("playable.additive_partial");
                        if (type == VRCAvatarDescriptor.AnimLayerType.Sitting) contexts.Add("playable.sitting_partial");

                        foreach (var (clip, kind) in leaves)
                        {
                            var settings = AnimationUtility.GetAnimationClipSettings(clip);
                            Add("clip." + kind, settings.mirror);
                            if (MovingBody(clip)) Settings(settings, k => Add("moving." + k, settings.mirror));
                            foreach (var ctx in contexts) Add(ctx + "+" + kind, settings.mirror);
                        }
                        if (contexts.Contains("blendtree.direct") && leaves.All(l => l.kind == "root_t_y_only") && contexts.Contains("layer.override_masked")
                            && of.TryGetValue(s, out var id)
                            && layers.Skip(li + 1).Any(above => above.blendingMode == AnimatorLayerBlendingMode.Additive && CaseOf(above.stateMachine).ContainsValue(id)))
                            Add("blendtree.direct+root_t_y_only+layer.override_masked+under_additive");
                    }
                }
            }
            return c;
        }

        static void Machine(AnimatorStateMachine sm, AnimatorStateMachine parent, HashSet<string> parameters, HashSet<string> triggers, System.Action<string> add)
        {
            var transitions = sm.anyStateTransitions.Cast<AnimatorStateTransition>().Concat(sm.states.SelectMany(s => s.state.transitions)).ToList();
            foreach (var t in transitions.Cast<AnimatorTransitionBase>().Concat(sm.entryTransitions))
                foreach (var c in t.conditions)
                    if (triggers.Contains(c.parameter)) add("parameter.trigger");
            foreach (var t in transitions)
            {
                if (t.duration > 0) add(t.hasFixedDuration ? "transition.fixed_duration" : "transition.normalized_duration");
                if (t.hasExitTime && t.offset > 0) add("transition.exit_time_offset");
            }
            if (parent != null)
            {
                if (sm.entryTransitions.Length > 0) add("submachine.entry");
                if (transitions.Any(t => t.isExit) && parent.GetStateMachineTransitions(sm).Length > 0) add("submachine.exit");
            }
            Behaviours(sm.behaviours, add);
            foreach (var cs in sm.states)
            {
                var s = cs.state;
                add(s.writeDefaultValues ? "write_defaults.on" : "write_defaults.off");
                if (s.speedParameterActive && parameters.Contains(s.speedParameter)) add("state.speed_parameter");
                if (s.timeParameterActive && parameters.Contains(s.timeParameter)) add("state.time_parameter");
                if (s.cycleOffsetParameterActive && parameters.Contains(s.cycleOffsetParameter)) add("state.cycle_offset_parameter");
                if (s.mirrorParameterActive && parameters.Contains(s.mirrorParameter)) add("state.mirror_parameter");
                if (s.motion is BlendTree bt) Tree(bt, add);
                Behaviours(s.behaviours, add);
            }
            foreach (var child in sm.stateMachines) Machine(child.stateMachine, sm, parameters, triggers, add);
        }

        static void Behaviours(StateMachineBehaviour[] behaviours, System.Action<string> add)
        {
            foreach (var b in behaviours)
                switch (b)
                {
                    case VRC_AvatarParameterDriver d:
                        if (d.localOnly) add("driver.local_only");
                        foreach (var p in d.parameters) add("driver." + p.type.ToString().ToLowerInvariant());
                        break;
                    case VRC_AnimatorTrackingControl _: add("behaviour.tracking"); break;
                    case VRC_AnimatorLocomotionControl _: add("behaviour.locomotion"); break;
                    case VRC_PlayableLayerControl _: add("behaviour.playable_layer"); break;
                    case VRC_AnimatorLayerControl _: add("behaviour.animator_layer"); break;
                    case VRC_AnimatorTemporaryPoseSpace _: add("behaviour.temporary_pose_space"); break;
                }
        }

        static void Tree(BlendTree bt, System.Action<string> add)
        {
            add("blendtree." + Snake(bt.blendType.ToString()).Replace("2_d", "2d").Replace("1_d", "1d"));
            foreach (var ch in bt.children) if (ch.motion is BlendTree sub) Tree(sub, add);
        }

        static string Snake(string s) => string.Concat(s.Select((ch, i) => i > 0 && (char.IsUpper(ch) || char.IsDigit(ch) && !char.IsDigit(s[i - 1])) ? "_" + char.ToLowerInvariant(ch) : char.ToLowerInvariant(ch).ToString()));
    }
}
