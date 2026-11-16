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

            var custom = Controllers(d).ToList();
            foreach (var (type, _, mask) in custom)
            {
                Add("playable." + Playable[type]);
                if (mask != null) Mask(mask, Add);
            }
            foreach (var ctrl in custom.Select(l => l.controller).Distinct())
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
            if (BodyPartOff(m)) add("mask.body_part");
        }

        static bool BodyPartOff(AvatarMask m)
        {
            if (m == null) return false;
            for (var p = AvatarMaskBodyPart.Root; p < AvatarMaskBodyPart.LastBodyPart; p++)
                if (!m.GetHumanoidBodyPartActive(p)) return true;
            return false;
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
            if (HasReference(s)) add("clip.additive_reference_pose");
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
        // its state mirrors. Only states a case enters count, and the harness fails a case whose states the
        // recording never entered or whose partial playable never held a weight strictly between 0 and 1.
        static readonly string[] Kinds = { "goal_curves", "no_goal_curves", "root_t_y_only" };
        static readonly string[] Contexts =
        {
            "crossfade", "blendtree.1d", "blendtree.2d", "blendtree.direct", "layer.override_masked", "layer.additive_masked",
            "playable.action_partial", "playable.additive_partial", "playable.sitting_partial",
        };
        const string UnderAdditive = "blendtree.direct+root_t_y_only+layer.override_masked+under_additive";
        static readonly string[] Unmirrored = Kinds.Select(k => "clip." + k)
            .Concat(Required.Where(k => k == "clip.mirror" || k == "clip.loop_pose" || k.StartsWith("clip.root_")).Select(k => "moving." + k))
            .Concat(Contexts.SelectMany(c => Kinds.Select(k => c + "+" + k)))
            .Append(UnderAdditive)
            .ToArray();
        public static readonly string[] Compositions = Unmirrored.Concat(Unmirrored.Select(k => k + ".mirrored")).ToArray();

        public class ProbeCase
        {
            public int Id;
            // Each state's label and full-path hash, as the animator reports its current state.
            public readonly List<(string label, int hash)> States = new List<(string, int)>();
            public readonly HashSet<VRCAvatarDescriptor.AnimLayerType> Partial = new HashSet<VRCAvatarDescriptor.AnimLayerType>();
            public bool Sitting => Partial.Contains(VRCAvatarDescriptor.AnimLayerType.Sitting);
        }

        static IEnumerable<(VRCAvatarDescriptor.AnimLayerType type, AnimatorController controller, AvatarMask mask)> Controllers(VRCAvatarDescriptor d) =>
            d.baseAnimationLayers.Concat(d.specialAnimationLayers ?? new VRCAvatarDescriptor.CustomAnimLayer[0])
                .Where(l => !l.isDefault && l.animatorController is AnimatorController).Select(l => (l.type, (AnimatorController)l.animatorController, l.mask));

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

        static bool Partial(AnimatorState s, VRC_PlayableLayerControl.BlendableLayer layer) =>
            s.behaviours.OfType<VRC_PlayableLayerControl>().Any(b => b.layer == layer && b.goalWeight > 0 && b.goalWeight < 1);

        // A state in the Action, Additive, Gesture or FX controller plays at partial weight when it sets its playable's weight
        // strictly between 0 and 1; the Sitting playable's while the harness toggles Seated faster than it blends.
        static VRCAvatarDescriptor.AnimLayerType? PartialPlayable(VRCAvatarDescriptor.AnimLayerType type, AnimatorState s)
        {
            switch (type)
            {
                case VRCAvatarDescriptor.AnimLayerType.Action when Partial(s, VRC_PlayableLayerControl.BlendableLayer.Action):
                case VRCAvatarDescriptor.AnimLayerType.Additive when Partial(s, VRC_PlayableLayerControl.BlendableLayer.Additive):
                case VRCAvatarDescriptor.AnimLayerType.Gesture when Partial(s, VRC_PlayableLayerControl.BlendableLayer.Gesture):
                case VRCAvatarDescriptor.AnimLayerType.FX when Partial(s, VRC_PlayableLayerControl.BlendableLayer.FX):
                case VRCAvatarDescriptor.AnimLayerType.Sitting:
                    return type;
                default: return null;
            }
        }

        public static List<ProbeCase> Cases(VRCAvatarDescriptor d)
        {
            var cases = new SortedDictionary<int, ProbeCase>();
            foreach (var (type, ctrl, _) in Controllers(d))
                foreach (var layer in ctrl.layers)
                    foreach (var kv in CaseOf(layer.stateMachine))
                    {
                        if (!cases.TryGetValue(kv.Value, out var c)) cases[kv.Value] = c = new ProbeCase { Id = kv.Value };
                        c.States.Add(($"{Playable[type]}/{layer.name}: {kv.Key.name}", Animator.StringToHash(layer.name + "." + kv.Key.name)));
                        if (PartialPlayable(type, kv.Key) is VRCAvatarDescriptor.AnimLayerType p) c.Partial.Add(p);
                    }
            return cases.Values.ToList();
        }

        static IEnumerable<AnimationClip> Leaves(Motion m) =>
            m is AnimationClip c ? new[] { c } : m is BlendTree t ? t.children.SelectMany(ch => Leaves(ch.motion)) : Enumerable.Empty<AnimationClip>();

        static readonly string[] GoalCurves = Fixture.Goals.SelectMany(g => new[] { g + "T.", g + "Q." }).ToArray();

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
            foreach (var (type, ctrl, _) in Controllers(d))
            {
                var parameters = new HashSet<string>(ctrl.parameters.Select(p => p.name));
                bool Human(Motion m) => Leaves(m).Any(l => Kind(l, parameters) != null);
                var layers = ctrl.layers;
                for (int li = 0; li < layers.Length; li++)
                {
                    var layer = layers[li];
                    var of = CaseOf(layer.stateMachine);
                    var all = layer.stateMachine.states.Select(x => x.state).ToList();
                    foreach (var s in of.Keys)
                    {
                        var leaves = Leaves(s.motion).Select(l => (clip: l, kind: Kind(l, parameters))).Where(l => l.kind != null).ToList();
                        if (leaves.Count == 0) continue;
                        var mirrored = s.mirrorParameterActive && parameters.Contains(s.mirrorParameter);
                        void Add(string k)
                        {
                            c[k] = c.TryGetValue(k, out var n) ? n + 1 : 1;
                            if (mirrored) c[k + ".mirrored"] = c.TryGetValue(k + ".mirrored", out var m) ? m + 1 : 1;
                        }
                        var contexts = new List<string>();
                        if (s.motion is BlendTree bt)
                            contexts.Add(bt.blendType == BlendTreeType.Simple1D ? "blendtree.1d" : bt.blendType == BlendTreeType.Direct ? "blendtree.direct" : "blendtree.2d");
                        var fadesIn = all.Any(x => x.transitions.Any(t => t.destinationState == s && t.duration > 0 && Human(x.motion)));
                        var fadesOut = s.transitions.Any(t => t.duration > 0 && t.destinationState != null && Human(t.destinationState.motion));
                        if (fadesIn || fadesOut) contexts.Add("crossfade");
                        var masked = li > 0 && BodyPartOff(layer.avatarMask);
                        if (masked && layer.blendingMode == AnimatorLayerBlendingMode.Override) contexts.Add("layer.override_masked");
                        if (masked && layer.blendingMode == AnimatorLayerBlendingMode.Additive) contexts.Add("layer.additive_masked");
                        var partial = PartialPlayable(type, s);
                        if (partial.HasValue) contexts.Add("playable." + Playable[partial.Value] + "_partial");

                        foreach (var (clip, kind) in leaves)
                        {
                            Add("clip." + kind);
                            if (MovingBody(clip)) Settings(AnimationUtility.GetAnimationClipSettings(clip), k => Add("moving." + k));
                            foreach (var ctx in contexts) Add(ctx + "+" + kind);
                        }
                        if (contexts.Contains("blendtree.direct") && contexts.Contains("layer.override_masked") && leaves.All(l => l.kind == "root_t_y_only")
                            && layers.Skip(li + 1).Any(above => above.blendingMode == AnimatorLayerBlendingMode.Additive && CaseOf(above.stateMachine).ContainsValue(of[s])))
                            Add(UnderAdditive);
                    }
                }
            }
            return c;
        }

        // The blend-tree compositions the blend duration probe separates, by the tree a state plays: its algorithm
        // (".normalized" for a direct tree with normalized blend values, ".nested" for a tree with a child tree) and
        // whether its clips are all zero-length, all with length, all empty (a clip without curves, which reports a
        // length of one second) or mixed, and beside what an empty clip plays. A flat direct tree also counts its weight
        // sum, or "ramped" where the harness drives a weight, or which kind of clip a zero weight silences; a mixed one
        // which kind weighs more and whether its zero-length clips are many. A nested direct tree counts each child
        // tree's algorithm. Only states a case enters and default states count.
        const int Many = 8;
        static readonly string[] Contents = { "length_only", "mixed" };
        public static readonly string[] BlendCompositions =
            new[] { "blendtree.direct", "blendtree.direct.normalized" }.SelectMany(t =>
                Contents.SelectMany(c => new[] { "sum_below_one", "sum_one", "sum_above_one", "ramped" }.Select(sum => $"{t}.{c}.{sum}"))
                .Concat(new[] { "zero_length_only", "nested.mixed" }.Select(k => $"{t}.{k}"))
                .Concat(new[] { "default_state", "write_defaults_off", "zero_length_heavier", "zero_length_lighter", "zero_length_unweighted", "length_unweighted", "many_zero_length" }.Select(k => $"{t}.mixed.{k}")))
            .Concat(new[] { "length_only", "inner_zero_length_only", "many_inner_zero_length_only", "inner_sum_below_one", "outer_sum_below_one", "twice",
                "inner.direct", "inner.direct.normalized", "inner.simple_1d", "inner.freeform_cartesian_2d" }.Select(k => "blendtree.direct.nested." + k))
            .Concat(new[] { "inner.direct", "inner.simple_1d" }.Select(k => "blendtree.direct.normalized.nested." + k))
            .Append("blendtree.simple_1d.nested.mixed")
            .Concat(new[] { "direct.empty.alone", "direct.empty.beside_length", "direct.empty.beside_zero_length", "direct.normalized.empty.beside_length",
                "direct.nested.empty.beside_length", "simple_1d.empty.beside_length" }.Select(k => "blendtree." + k))
            .Concat(new[] { "clip.empty", "clip.zero_length" })
            .Concat(new[] { "simple_1d", "simple_directional_2d", "freeform_directional_2d", "freeform_cartesian_2d" }.SelectMany(t => new[] { $"blendtree.{t}.zero_length_only", $"blendtree.{t}.mixed" }))
            .ToArray();

        static string Span(AnimationClip clip) =>
            AnimationUtility.GetCurveBindings(clip).Length + AnimationUtility.GetObjectReferenceCurveBindings(clip).Length == 0 ? "empty" : clip.length == 0 ? "zero_length" : "length";

        static string Content(Motion m)
        {
            var spans = Leaves(m).Select(Span).Distinct().ToList();
            return spans.Contains("empty") ? spans.Count == 1 ? "empty" : "with_empty" : spans.Count == 1 ? spans[0] + "_only" : "mixed";
        }

        public static SortedDictionary<string, int> CountBlendCompositions(VRCAvatarDescriptor d)
        {
            var c = new SortedDictionary<string, int>();
            foreach (var k in BlendCompositions) c[k] = 0;
            void Add(string k) => c[k] = c.TryGetValue(k, out var n) ? n + 1 : 1;
            foreach (var (_, ctrl, _) in Controllers(d))
            {
                var defaults = ctrl.parameters.ToDictionary(p => p.name, p => p.defaultFloat);
                bool Ramped(BlendTree t) => t.children.Any(ch => Fixture.Ramps.Contains(ch.directBlendParameter));
                float Weight(BlendTree t, System.Func<ChildMotion, bool> of) => t.children.Where(of).Sum(ch => defaults[ch.directBlendParameter]);
                foreach (var layer in ctrl.layers)
                {
                    var sm = layer.stateMachine;
                    var cases = CaseOf(sm);
                    foreach (var s in sm.states.Select(x => x.state))
                    {
                        if (!cases.ContainsKey(s) && s != sm.defaultState) continue;
                        if (s.motion is AnimationClip alone && Span(alone) != "length") Add("clip." + Span(alone));
                        if (!(s.motion is BlendTree t)) continue;
                        var content = Content(t);
                        var direct = t.blendType == BlendTreeType.Direct;
                        var inner = t.children.Select(ch => ch.motion).OfType<BlendTree>().ToList();
                        var nested = inner.Count > 0;
                        var tree = Algorithm(t);
                        var key = $"{tree}{(nested ? ".nested" : "")}.{content}";
                        Add(key);
                        if (!s.writeDefaultValues) Add(key + ".write_defaults_off");
                        if (s == sm.defaultState) Add(key + ".default_state");
                        var spans = Leaves(t).Select(Span).Distinct().ToList();
                        if (spans.Count == 1 && spans[0] == "empty") Add(key + ".alone");
                        else if (spans.Contains("empty")) foreach (var beside in spans.Where(x => x != "empty")) Add($"{tree}{(nested ? ".nested" : "")}.empty.beside_{beside}");
                        if (!direct) continue;
                        if (nested)
                        {
                            int still = inner.Count(sub => Content(sub) == "zero_length_only");
                            if (content == "mixed" && still > 0) Add(tree + ".nested.inner_zero_length_only");
                            if (content == "mixed" && still >= Many) Add(tree + ".nested.many_inner_zero_length_only");
                            foreach (var sub in inner.Select(Algorithm).Distinct()) Add($"{tree}.nested.inner.{sub.Substring("blendtree.".Length)}");
                            if (inner.Any(sub => sub.children.Any(ch => ch.motion is BlendTree))) Add(tree + ".nested.twice");
                            bool Fixed(BlendTree x) => Algorithm(x) == "blendtree.direct" && !Ramped(x);
                            if (inner.Any(sub => Fixed(sub) && Weight(sub, _ => true) < 1)) Add(tree + ".nested.inner_sum_below_one");
                            if (Fixed(t) && Weight(t, _ => true) < 1) Add(tree + ".nested.outer_sum_below_one");
                            continue;
                        }
                        if (Ramped(t)) { Add(key + ".ramped"); continue; }
                        bool ZeroLength(ChildMotion ch) => Span((AnimationClip)ch.motion) == "zero_length";
                        var zeros = t.children.Where(ZeroLength).ToList();
                        float zero = Weight(t, ZeroLength), other = Weight(t, ch => !ZeroLength(ch));
                        bool both = zeros.Count > 0 && zeros.Count < t.children.Length;
                        if (both && zero == 0) Add(key + ".zero_length_unweighted");
                        else if (both && other == 0) Add(key + ".length_unweighted");
                        else Add(key + "." + SumKey(zero + other));
                        if (!both || zero == 0 || other == 0) continue;
                        if (zero != other) Add(key + (zero > other ? ".zero_length_heavier" : ".zero_length_lighter"));
                        if (zeros.Count >= Many) Add(key + ".many_zero_length");
                    }
                }
            }
            return c;
        }

        // "above" and "below" name the layer without write defaults' side by layer order: a layer above it is
        // later in the controller and blends over it.
        public static readonly string[] LayeringCompositions =
        {
            "toggle_return.alone",
            "toggle_return.below_write_defaults_off.with_motion",
            "toggle_return.below_write_defaults_off.without_motion",
            "toggle_return.above_write_defaults_off.with_motion",
            "toggle_return.above_write_defaults_off.without_motion",
            "toggle_return.first_layer.below_write_defaults_off.with_motion",
            "toggle_return.first_layer.below_write_defaults_off.without_motion",
            "write_defaults_off.empty_after_motion",
            "zero_length_transition.direct.into_write_defaults_on",
            "zero_length_transition.simple_1d.into_write_defaults_on",
            "zero_length_transition.direct.nested.into_write_defaults_on",
            "zero_length_transition.freeform_cartesian_2d.into_write_defaults_on",
            "zero_length_transition.direct.into_write_defaults_off",
            "direct.write_defaults_off.binds_some.sum_below_one",
            "direct.write_defaults_off.binds_some.sum_one",
            "direct.write_defaults_off.binds_some.sum_above_one",
            "direct.write_defaults_off.binds_all.sum_below_one",
            "direct.write_defaults_off.binds_all.sum_one",
            "direct.write_defaults_off.binds_all.sum_above_one",
            "direct.write_defaults_off.binds_none.sum_below_one",
            "direct.write_defaults_off.binds_none.sum_one",
            "direct.write_defaults_off.binds_none.sum_above_one",
            "direct.normalized.sum_below_one.length_only",
            "direct.normalized.sum_below_one.zero_length_only",
            "direct.normalized.sum_below_one.mixed",
            "direct.normalized.sum_below_one.length_only.write_defaults_off",
        };

        static string SumKey(float sum) => Mathf.Approximately(sum, 1) ? "sum_one" : sum < 1 ? "sum_below_one" : "sum_above_one";

        static IEnumerable<EditorCurveBinding> Bindings(Motion m) =>
            Leaves(m).SelectMany(c => AnimationUtility.GetCurveBindings(c).Concat(AnimationUtility.GetObjectReferenceCurveBindings(c)));

        public static SortedDictionary<string, int> CountLayeringCompositions(VRCAvatarDescriptor d)
        {
            var c = new SortedDictionary<string, int>();
            foreach (var k in LayeringCompositions) c[k] = 0;
            void Add(string k) => c[k] = c.TryGetValue(k, out var n) ? n + 1 : 1;
            foreach (var (_, ctrl, _) in Controllers(d))
            {
                var defaults = ctrl.parameters.ToDictionary(p => p.name, p => p.defaultFloat);
                var layers = ctrl.layers;
                var cases = layers.Select(l => CaseOf(l.stateMachine)).ToArray();
                for (int i = 0; i < layers.Length; i++)
                {
                    var sm = layers[i].stateMachine;
                    var states = sm.states.Select(x => x.state).ToList();
                    var targets = new HashSet<EditorCurveBinding>(states.Where(s => s.motion is AnimationClip).SelectMany(s => Bindings(s.motion)));
                    foreach (var s in states.Where(cases[i].ContainsKey))
                    {
                        var id = cases[i][s];
                        foreach (var t in s.transitions.Where(t => t.destinationState != null && t.destinationState.motion == null))
                        {
                            var to = t.destinationState;
                            if (s.motion is AnimationClip && s.writeDefaultValues && to.writeDefaultValues)
                            {
                                if (!cases.Where((of, j) => j != i && of.ContainsValue(id)).Any()) Add("toggle_return.alone");
                                for (int j = 0; j < layers.Length; j++)
                                    foreach (var other in cases[j].Where(kv => j != i && kv.Value == id && !kv.Key.writeDefaultValues).Select(kv => kv.Key))
                                        Add($"toggle_return.{(i == 0 ? "first_layer." : "")}{(j < i ? "above" : "below")}_write_defaults_off.{(other.motion != null ? "with_motion" : "without_motion")}");
                            }
                            if (s.motion is AnimationClip && !s.writeDefaultValues && !to.writeDefaultValues) Add("write_defaults_off.empty_after_motion");
                            if (s.motion is BlendTree bt && t.duration == 0)
                                Add($"zero_length_transition.{Algorithm(bt).Substring("blendtree.".Length)}{(bt.children.Any(ch => ch.motion is BlendTree) ? ".nested" : "")}.into_write_defaults_{(to.writeDefaultValues ? "on" : "off")}");
                        }
                        if (!(s.motion is BlendTree tree) || tree.blendType != BlendTreeType.Direct || tree.children.Any(ch => ch.motion is BlendTree)) continue;
                        var sum = SumKey(tree.children.Sum(ch => defaults[ch.directBlendParameter]));
                        if (AvatarWorkshop.Exporter.Exporter.NormalizedBlendValues(tree))
                        {
                            if (sum == "sum_below_one") Add($"direct.normalized.sum_below_one.{Content(tree)}{(s.writeDefaultValues ? "" : ".write_defaults_off")}");
                            continue;
                        }
                        if (s.writeDefaultValues) continue;
                        var written = new HashSet<EditorCurveBinding>(Bindings(tree));
                        var bound = targets.Count(written.Contains);
                        Add($"direct.write_defaults_off.binds_{(bound == 0 ? "none" : bound == targets.Count ? "all" : "some")}.{sum}");
                    }
                }
            }
            return c;
        }

        public static readonly string[] SampleTimeCompositions =
            (from entry in Fixture.SampleTimeEntries
             from container in Fixture.SampleTimeContainers
             from speed in Fixture.SampleTimeSpeeds
             from e in Fixture.SampleTimeExponents
             select $"sample_time.{entry}.{container}.{Fixture.SpeedKey(speed)}.length_1e{e}").ToArray();

        // The parameter an identity clip animates and its length: one curve on a float parameter of the controller,
        // keys (0, 0) and (L, L) with unit tangents, looping, without a cycle offset.
        static (string parameter, float length)? Identity(AnimationClip clip, Dictionary<string, AnimatorControllerParameter> parameters)
        {
            var bindings = AnimationUtility.GetCurveBindings(clip);
            if (bindings.Length != 1 || bindings[0].type != typeof(Animator) || bindings[0].path != "") return null;
            if (!parameters.TryGetValue(bindings[0].propertyName, out var p) || p.type != AnimatorControllerParameterType.Float) return null;
            var keys = AnimationUtility.GetEditorCurve(clip, bindings[0]).keys;
            if (keys.Length != 2 || keys[0].time != 0 || keys[0].value != 0 || keys[1].time <= 0 || keys[1].value != keys[1].time) return null;
            if (keys.Any(k => k.inTangent != 1 || k.outTangent != 1 || k.weightedMode != WeightedMode.None)) return null;
            var settings = AnimationUtility.GetAnimationClipSettings(clip);
            if (!settings.loopTime || settings.cycleOffset != 0) return null;
            return (p.name, keys[1].time);
        }

        public static SortedDictionary<string, int> CountSampleTimeCompositions(VRCAvatarDescriptor d)
        {
            var c = new SortedDictionary<string, int>();
            foreach (var k in SampleTimeCompositions) c[k] = 0;
            foreach (var (_, ctrl, _) in Controllers(d))
            {
                var parameters = ctrl.parameters.ToDictionary(p => p.name);
                var states = ctrl.layers.SelectMany(l => l.stateMachine.states.Select(x => x.state)).ToList();
                var writers = states.SelectMany(s => Leaves(s.motion).SelectMany(AnimationUtility.GetCurveBindings).Where(b => b.type == typeof(Animator) && b.path == "")
                    .Select(b => b.propertyName).Distinct()).GroupBy(n => n).ToDictionary(g => g.Key, g => g.Count());
                foreach (var layer in ctrl.layers)
                {
                    var sm = layer.stateMachine;
                    if (layer.defaultWeight != 1 || layer.blendingMode != AnimatorLayerBlendingMode.Override || layer.syncedLayerIndex >= 0) continue;
                    var cases = CaseOf(sm);
                    var into = sm.states.SelectMany(x => x.state.transitions).Where(t => t.destinationState != null).ToLookup(t => t.destinationState);
                    foreach (var s in sm.states.Select(x => x.state))
                    {
                        var entry = cases.ContainsKey(s) && into[s].All(t => t.duration == 0) && s.transitions.Length == 0 ? "later"
                            : s == sm.defaultState && !cases.ContainsKey(s) && s.transitions.Length == 0 ? "first_frame" : null;
                        if (entry == null || s.timeParameterActive || s.cycleOffsetParameterActive || s.cycleOffset != 0) continue;
                        string container;
                        if (s.motion is AnimationClip) container = "state";
                        else if (s.motion is BlendTree bt && bt.children.All(ch => ch.motion is AnimationClip && ch.timeScale == 1 && ch.cycleOffset == 0))
                        {
                            if (bt.blendType == BlendTreeType.Simple1D && parameters.TryGetValue(bt.blendParameter, out var bp) && !Fixture.Ramps.Contains(bp.name)
                                && bt.children.Length > 0 && bt.children[0].threshold == bp.defaultFloat && bt.children.Skip(1).All(ch => ch.threshold > bp.defaultFloat)
                                && bt.children.All(ch => ch.motion == bt.children[0].motion))
                                container = "simple_1d";
                            else if (bt.blendType == BlendTreeType.Direct && bt.children.Length == 1 && parameters.TryGetValue(bt.children[0].directBlendParameter, out var w)
                                && !Fixture.Ramps.Contains(w.name) && w.defaultFloat == 1 && !AvatarWorkshop.Exporter.Exporter.NormalizedBlendValues(bt))
                                container = "direct";
                            else continue;
                        }
                        else continue;
                        var identities = Leaves(s.motion).Select(l => Identity(l, parameters)).Distinct().ToList();
                        if (identities.Count != 1 || identities[0] == null) continue;
                        var (parameter, length) = identities[0].Value;
                        var e = Fixture.SampleTimeExponents.Where(x => Mathf.Pow(10, x) == length).Select(x => (int?)x).FirstOrDefault();
                        float? speed = s.speedParameterActive ? (float?)null : s.speed;
                        if (s.speedParameterActive && (s.speed != 1 || !parameters.TryGetValue(s.speedParameter, out var sp) || Fixture.Ramps.Contains(sp.name) || sp.defaultFloat == 0)) continue;
                        if (e == null || !Fixture.SampleTimeSpeeds.Contains(speed)) continue;
                        // A parameter another state also animates holds neither's sample time alone.
                        if (writers[parameter] != 1) continue;
                        var key = $"sample_time.{entry}.{container}.{Fixture.SpeedKey(speed)}.length_1e{e}";
                        c[key] = c.TryGetValue(key, out var n) ? n + 1 : 1;
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
            add(TreeKey(bt));
            foreach (var ch in bt.children) if (ch.motion is BlendTree sub) Tree(sub, add);
        }

        static string Algorithm(BlendTree t) =>
            TreeKey(t) + (t.blendType == BlendTreeType.Direct && AvatarWorkshop.Exporter.Exporter.NormalizedBlendValues(t) ? ".normalized" : "");

        // The rotation probe's compositions: "rotation.<context>+<kind>.axes_<axes>.<setting>", ".unnormalized" after a
        // pose whose body rotation is not of unit length, and ".mirrored" after each where its state mirrors. A pose
        // holds one key per curve at time zero; a moving clip's body moves. The axes are those the body and every IK
        // goal all turn about. The setting joins how the clip's root-motion settings depart from the original ones.
        // Base plays at full weight from its first layer and at partial weight from a later layer weighted strictly
        // between 0 and 1; Action at the weight the state's playable-layer control sets.
        static readonly Dictionary<string, string> RotationSettingKeys = Fixture.RotationSettings.ToDictionary(x => x.name, x =>
        {
            var s = new AnimationClipSettings();
            Fixture.Original(s); x.set(s);
            return RotationSetting(s);
        });
        static readonly string[] RotationUnmirrored =
            (from kind in Fixture.RotationKinds from setting in RotationSettingKeys.Values select $"rotation.base_full+{kind}.axes_xyz.{setting}")
            .Concat(from kind in Fixture.RotationKinds from axes in Fixture.RotationAxes.Where(a => a != "xyz") from setting in Fixture.RotationPaired
                    select $"rotation.base_full+{kind}.axes_{axes}.{RotationSettingKeys[setting]}")
            .Concat(from context in Fixture.RotationContexts.Where(c => c != "base_full") from kind in Fixture.RotationKinds from setting in Fixture.RotationPaired
                    select $"rotation.{context}+{kind}.axes_xyz.{RotationSettingKeys[setting]}")
            .Concat(Fixture.RotationUnnormalizedContexts.Select(c => $"rotation.{c}+pose.axes_xyz.{RotationSettingKeys["emote"]}.unnormalized"))
            .ToArray();
        public static readonly string[] RotationCompositions = RotationUnmirrored.Concat(RotationUnmirrored.Select(k => k + ".mirrored")).ToArray();

        static string RotationSetting(AnimationClipSettings s)
        {
            var on = new List<string>();
            Settings(s, k => { if (!k.EndsWith(".original")) on.Add(k.Substring("clip.".Length)); });
            return on.Count == 0 ? "original" : string.Join("+", on);
        }

        static Keyframe[][] QuaternionKeys(AnimationClip clip, string prop) =>
            "xyzw".Select(a => AnimationUtility.GetEditorCurve(clip, EditorCurveBinding.FloatCurve("", typeof(Animator), $"{prop}.{a}"))?.keys).ToArray();

        static string TurnAxes(AnimationClip clip, string prop)
        {
            var q = QuaternionKeys(clip, prop);
            if (q.Any(k => k == null)) return null;
            return string.Concat(Enumerable.Range(0, 3).Where(i => q[i].Any(k => Mathf.Abs(k.value) > 1e-4f)).Select(i => "xyz"[i]));
        }

        static string RotationKind(AnimationClip clip)
        {
            if (Span(clip) == "zero_length" && AnimationUtility.GetCurveBindings(clip).All(b => AnimationUtility.GetEditorCurve(clip, b).keys.All(k => k.time == 0))) return "pose";
            return Span(clip) == "length" && MovingBody(clip) ? "moving" : null;
        }

        static string RotationContext(VRCAvatarDescriptor.AnimLayerType type, int li, AnimatorControllerLayer layer, AnimatorState s)
        {
            if (type == VRCAvatarDescriptor.AnimLayerType.Base)
                return li == 0 && layer.defaultWeight == 1 ? "base_full" : li > 0 && layer.defaultWeight > 0 && layer.defaultWeight < 1 ? "base_partial" : null;
            if (type != VRCAvatarDescriptor.AnimLayerType.Action) return null;
            var goals = s.behaviours.OfType<VRC_PlayableLayerControl>().Where(b => b.layer == VRC_PlayableLayerControl.BlendableLayer.Action).Select(b => b.goalWeight).ToList();
            return goals.Count != 1 ? null : goals[0] == 1 ? "action_full" : goals[0] > 0 && goals[0] < 1 ? "action_partial" : null;
        }

        public static SortedDictionary<string, int> CountRotationCompositions(VRCAvatarDescriptor d)
        {
            var c = new SortedDictionary<string, int>();
            foreach (var k in RotationCompositions) c[k] = 0;
            foreach (var (type, ctrl, _) in Controllers(d))
            {
                var parameters = new HashSet<string>(ctrl.parameters.Select(p => p.name));
                var layers = ctrl.layers;
                for (int li = 0; li < layers.Length; li++)
                    foreach (var s in CaseOf(layers[li].stateMachine).Keys)
                    {
                        if (!(s.motion is AnimationClip clip)) continue;
                        var kind = RotationKind(clip);
                        var axes = TurnAxes(clip, "RootQ");
                        var context = RotationContext(type, li, layers[li], s);
                        if (kind == null || context == null || string.IsNullOrEmpty(axes) || Fixture.Goals.Any(g => TurnAxes(clip, g + "Q") != axes)) continue;
                        var q = QuaternionKeys(clip, "RootQ").Select(k => k[0].value).ToArray();
                        var unnormalized = Mathf.Abs(q.Sum(x => x * x) - 1) > 1e-3f;
                        var key = $"rotation.{context}+{kind}.axes_{axes}.{RotationSetting(AnimationUtility.GetAnimationClipSettings(clip))}{(unnormalized ? ".unnormalized" : "")}";
                        c[key] = c.TryGetValue(key, out var n) ? n + 1 : 1;
                        if (s.mirrorParameterActive && parameters.Contains(s.mirrorParameter)) c[key + ".mirrored"] = c.TryGetValue(key + ".mirrored", out var m) ? m + 1 : 1;
                    }
            }
            return c;
        }

        // The dense clip probe's compositions: "dense.<kind>.<keying>.rate_<sample rate>.<tangents>.<speed>". A clip's
        // probed curves are those with more than two keys, and all of them must share a keying and tangents. Keyings
        // are read against the clip's own frame grid: a key on each frame, on most (a gap of two among gaps of one),
        // on every other, or keys off the grid between its first and last frame.
        public static readonly string[] DenseCompositions =
            (from kind in Fixture.DenseKinds
             from keying in Fixture.DenseKeyings
             from rate in Fixture.DenseRates
             from tangent in Fixture.DenseTangents
             from quality in Fixture.DenseQualities
             where Fixture.DenseCase(tangent, quality)
             from speed in Fixture.DenseSpeeds
             select $"dense.{kind}.{keying}.rate_{rate}.{tangent}.quality_{quality}.{Fixture.SpeedKey(speed)}").ToArray();

        static string Keying(Keyframe[] keys, float rate)
        {
            var frames = keys.Select(k => (double)k.time * rate).ToArray();
            bool OnGrid(double f) => System.Math.Abs(f - System.Math.Round(f)) < 1e-3;
            if (!OnGrid(frames[0]) || !OnGrid(frames[frames.Length - 1])) return null;
            if (frames.Skip(1).Take(frames.Length - 2).All(f => !OnGrid(f))) return "off_grid";
            if (!frames.All(OnGrid)) return null;
            var gaps = frames.Zip(frames.Skip(1), (a, b) => (int)System.Math.Round(b - a)).ToList();
            if (gaps.All(g => g == 1)) return "every_frame";
            if (gaps.All(g => g == 2)) return "every_other_frame";
            if (gaps.All(g => g == 1 || g == 2) && gaps.Count(g => g == 1) > gaps.Count(g => g == 2)) return "most_frames";
            return null;
        }

        static string Tangents(AnimationCurve curve)
        {
            var keys = curve.keys;
            var all = Enumerable.Range(0, keys.Length).ToList();
            if (keys.Any(k => k.weightedMode != WeightedMode.None)) return null;
            if (all.All(i => AnimationUtility.GetKeyBroken(curve, i)) && keys.Any(k => k.inTangent != k.outTangent)
                && all.All(i => AnimationUtility.GetKeyLeftTangentMode(curve, i) == AnimationUtility.TangentMode.Free && AnimationUtility.GetKeyRightTangentMode(curve, i) == AnimationUtility.TangentMode.Free))
                return "broken";
            if (all.Any(i => AnimationUtility.GetKeyBroken(curve, i))) return null;
            foreach (var (mode, name) in new[] { (AnimationUtility.TangentMode.Auto, "auto"), (AnimationUtility.TangentMode.ClampedAuto, "clamped_auto") })
                if (all.All(i => AnimationUtility.GetKeyLeftTangentMode(curve, i) == mode && AnimationUtility.GetKeyRightTangentMode(curve, i) == mode))
                    return name;
            if (all.All(i => AnimationUtility.GetKeyLeftTangentMode(curve, i) == AnimationUtility.TangentMode.Free && AnimationUtility.GetKeyRightTangentMode(curve, i) == AnimationUtility.TangentMode.Free)
                && keys.All(k => k.inTangent == 0 && k.outTangent == 0))
                return "flat";
            return null;
        }

        static string DenseClip(AnimationClip clip)
        {
            var probed = AnimationUtility.GetCurveBindings(clip).Select(b => AnimationUtility.GetEditorCurve(clip, b)).Where(c => c.length > 2).ToList();
            if (probed.Any(c => c.keys.Select(k => k.value).Distinct().Count() < 2)) return null;
            if (probed.Count == 0 || AnimationUtility.GetObjectReferenceCurveBindings(clip).Length > 0 || clip.frameRate != Mathf.Round(clip.frameRate)) return null;
            var keyings = probed.Select(c => Keying(c.keys, clip.frameRate)).Distinct().ToList();
            var tangents = probed.Select(Tangents).Distinct().ToList();
            if (keyings.Count != 1 || keyings[0] == null || tangents.Count != 1 || tangents[0] == null) return null;
            var quality = new SerializedObject(clip).FindProperty("m_UseHighQualityCurve").boolValue ? "high" : "low";
            return $"dense.{(clip.humanMotion ? "humanoid" : "generic")}.{keyings[0]}.rate_{(int)clip.frameRate}.{tangents[0]}.quality_{quality}";
        }

        public static SortedDictionary<string, int> CountDenseCompositions(VRCAvatarDescriptor d)
        {
            var c = new SortedDictionary<string, int>();
            foreach (var k in DenseCompositions) c[k] = 0;
            foreach (var (type, ctrl, mask) in Controllers(d))
                foreach (var layer in ctrl.layers.Where(l => l.defaultWeight == 1 && l.blendingMode == AnimatorLayerBlendingMode.Override && l.syncedLayerIndex < 0 && l.avatarMask == null && mask == null))
                    foreach (var s in CaseOf(layer.stateMachine).Keys)
                    {
                        if (!(s.motion is AnimationClip clip) || clip.humanMotion != (type == VRCAvatarDescriptor.AnimLayerType.Base) || s.speedParameterActive || s.timeParameterActive || s.cycleOffsetParameterActive || s.cycleOffset != 0) continue;
                        if (AnimationUtility.GetAnimationClipSettings(clip).loopTime || !(DenseClip(clip) is string key)) continue;
                        key += "." + Fixture.SpeedKey(s.speed);
                        c[key] = c.TryGetValue(key, out var n) ? n + 1 : 1;
                    }
            return c;
        }

        // The crossfade probe's compositions: "crossfade.<hemisphere>.<fixed|normalized>", ".interrupted" after a fade
        // the destination's own transition interrupts while it runs, for each fade Pulse starts in an additive Base
        // layer from the state a case enters to another, both clips subtracting a reference pose; the hemisphere is how
        // the destination's body rotation offset lies against the source's, or, where the destination's clip and
        // reference both have negative w, whether they turn alike. "crossfade.generic.<side>_only" where only that
        // side's clip writes a generic property. "scrub.<mirrored|unmirrored>.<loop|once>.<time>" for each humanoid clip
        // state of an override Base layer, by the default of the parameter driving its time ("time_ramp" for one the
        // harness ramps, "own_clock" for none). Only the fades Pulse starts count: a fade back or on from a destination
        // would otherwise credit a composition whose own case is missing.
        public static readonly string[] CrossfadeCompositions =
            (from f in new[] { true, false } from h in Fixture.CrossfadeHemispheres select Fixture.CrossfadeKey(h, f, false))
            .Concat(Fixture.CrossfadeInterruptedCases.Select(x => Fixture.CrossfadeKey(x.hemisphere, x.fixedDuration, true)))
            .Concat(new[] { "crossfade.generic.source_only", "crossfade.generic.destination_only" })
            .Concat(from x in Fixture.ScrubCases from t in x.times select Fixture.ScrubKey(x.mirrored, x.loop, t.key))
            .ToArray();

        static Vector4 RootQAt(AnimationClip clip, float time)
        {
            var q = "xyzw".Select(a => AnimationUtility.GetEditorCurve(clip, EditorCurveBinding.FloatCurve("", typeof(Animator), $"RootQ.{a}"))
                ?? throw new System.Exception($"{clip.name} has no RootQ.{a} curve")).Select(curve => curve.Evaluate(time)).ToArray();
            return new Vector4(q[0], q[1], q[2], q[3]);
        }

        static bool HasReference(AnimationClipSettings s) => s.hasAdditiveReferencePose && s.additiveReferencePoseClip != null;

        static AnimationClip Subtracting(Motion m) => m is AnimationClip c && HasReference(AnimationUtility.GetAnimationClipSettings(c)) ? c : null;

        static (Vector4 clip, Vector4 reference) BodyRotations(AnimationClip c)
        {
            var s = AnimationUtility.GetAnimationClipSettings(c);
            return (RootQAt(c, 0), RootQAt(s.additiveReferencePoseClip, s.additiveReferencePoseTime));
        }

        static Vector4 RotationOffset(AnimationClip c)
        {
            var (clip, reference) = BodyRotations(c);
            return clip - reference + new Vector4(0, 0, 0, 1);
        }

        static bool WritesGeneric(AnimationClip c) => AnimationUtility.GetCurveBindings(c).Any(b => b.type != typeof(Animator));

        static bool On(AnimatorStateTransition t, AnimatorConditionMode mode) =>
            t.conditions.Any(x => x.parameter == Fixture.PulseParameter && x.mode == mode);

        public static SortedDictionary<string, int> CountCrossfadeCompositions(VRCAvatarDescriptor d)
        {
            var c = new SortedDictionary<string, int>();
            foreach (var k in CrossfadeCompositions) c[k] = 0;
            void Add(string key) => c[key] = c.TryGetValue(key, out var n) ? n + 1 : 1;
            float pulse = (Fixture.PulseOff - Fixture.PulseOn) / (float)Harness.Rate;
            foreach (var (_, ctrl, _) in Controllers(d).Where(x => x.type == VRCAvatarDescriptor.AnimLayerType.Base && x.mask == null))
            {
                var parameters = ctrl.parameters.ToDictionary(p => p.name);
                foreach (var layer in ctrl.layers.Where(l => l.defaultWeight == 1 && l.avatarMask == null && l.syncedLayerIndex < 0))
                {
                    var sm = layer.stateMachine;
                    var cases = CaseOf(sm);
                    var entered = new HashSet<AnimatorState>(sm.states.SelectMany(x => x.state.transitions)
                        .Where(t => t.conditions.Any(x => x.parameter == Fixture.CaseParameter && x.mode == AnimatorConditionMode.Equals)).Select(t => t.destinationState));
                    foreach (var s in cases.Keys)
                    {
                        if (layer.blendingMode == AnimatorLayerBlendingMode.Additive)
                        {
                            if (!entered.Contains(s) || !(Subtracting(s.motion) is AnimationClip from)) continue;
                            foreach (var t in s.transitions.Where(t => t.duration > 0 && On(t, AnimatorConditionMode.If)))
                            {
                                if (!(t.destinationState is AnimatorState dst) || !cases.ContainsKey(dst) || !(Subtracting(dst.motion) is AnimationClip to)) continue;
                                var (target, reference) = BodyRotations(to);
                                var hemisphere = target.w < 0 && reference.w < 0 ? ((target - reference).magnitude < 1e-4f ? "negative_alike" : "negative_pair")
                                    : Vector4.Dot(RotationOffset(from), RotationOffset(to)) >= 0 ? "same" : "other";
                                var seconds = t.hasFixedDuration ? t.duration : t.duration * from.length;
                                var interrupted = (t.interruptionSource == TransitionInterruptionSource.Destination || t.interruptionSource == TransitionInterruptionSource.DestinationThenSource)
                                    && seconds > pulse && dst.transitions.Any(x => x.duration > 0 && On(x, AnimatorConditionMode.IfNot) && x.destinationState != null && cases.ContainsKey(x.destinationState));
                                Add(Fixture.CrossfadeKey(hemisphere, t.hasFixedDuration, interrupted));
                                if (WritesGeneric(from) != WritesGeneric(to)) Add($"crossfade.generic.{(WritesGeneric(from) ? "source" : "destination")}_only");
                            }
                        }
                        else if (layer.blendingMode == AnimatorLayerBlendingMode.Override && s.motion is AnimationClip clip && clip.humanMotion && Subtracting(clip) == null)
                        {
                            string time = !s.timeParameterActive ? "own_clock"
                                : Fixture.Ramps.Contains(s.timeParameter) ? "time_ramp"
                                : parameters.TryGetValue(s.timeParameter, out var p) && p.type == AnimatorControllerParameterType.Float ? Fixture.ScrubTime(p.defaultFloat) : null;
                            if (time != null) Add(Fixture.ScrubKey(s.mirror, AnimationUtility.GetAnimationClipSettings(clip).loopTime, time));
                        }
                    }
                }
            }
            return c;
        }

        // The generic probe's compositions. "additive.<playable>.<blendshape|transform>.<reference_with|reference_without>.<context>"
        // for each generic property a state of a full-weight additive layer plays against a reference pose: "alone" when
        // its state fades nowhere, "<side>_only" where only that side of a fade Pulse starts writes it; the reference is
        // the writing side's, by whether it holds the property. "curves.<kind>.quality_<low|high>" for each generic clip a
        // full-weight override FX layer plays at speed one with no reference, by Unity's m_UseHighQualityCurve and by
        // what its curves hold: stepped keys, one curve on its frame grid beside one off it, inner keys a listed offset
        // after the grid, a clip a case outlasts (looping or not), or otherwise the one kind of property it binds.
        public static readonly string[] GenericCompositions =
            (from t in Fixture.GenericPlayables from p in Fixture.GenericProperties from r in Fixture.GenericReferences from x in Fixture.GenericContexts
                select Fixture.GenericKey(Fixture.GenericPlayable(t), p, r, x))
            .Concat(from k in Fixture.CurveKinds from q in Fixture.CurveQualities select Fixture.CurveKey(k, q))
            .ToArray();

        static string Bound(EditorCurveBinding b) =>
            b.type == typeof(SkinnedMeshRenderer) && b.propertyName.StartsWith("blendShape.") ? "blendshape"
            : b.type != typeof(Transform) ? null
            : b.propertyName.StartsWith("m_LocalPosition.") ? "position" : b.propertyName.StartsWith("m_LocalRotation.") ? "rotation"
            : b.propertyName.StartsWith("localEulerAngles") ? "euler" : b.propertyName.StartsWith("m_LocalScale.") ? "scale" : null;

        static string GenericProperty(EditorCurveBinding b) => Bound(b) is string p ? (p == "blendshape" ? p : "transform") : null;

        static float OffGrid(AnimationCurve curve, float rate) => curve.keys.Max(k => Mathf.Abs(k.time * rate - Mathf.Round(k.time * rate)));

        static string CurveKind(AnimationClip clip, float caseSeconds)
        {
            var bindings = AnimationUtility.GetCurveBindings(clip);
            if (bindings.Length == 0 || clip.humanMotion || AnimationUtility.GetObjectReferenceCurveBindings(clip).Length > 0 || bindings.Any(b => Bound(b) == null)) return null;
            var curves = bindings.Select(b => AnimationUtility.GetEditorCurve(clip, b)).Where(c => c.keys.Select(k => k.value).Distinct().Count() > 1).ToList();
            if (curves.Count == 0) return null;
            var stepped = curves.SelectMany(c => c.keys).Select(k => float.IsInfinity(k.inTangent) || float.IsInfinity(k.outTangent)).ToList();
            if (stepped.Any(x => x)) return curves.Count == 1 ? (stepped.All(x => x) ? "stepped.all" : "stepped.some") : null;
            var off = curves.Select(c => OffGrid(c, clip.frameRate)).ToList();
            // Above float error in frames over the clip's length, and well below the least listed offset.
            const float onGrid = 2e-5f;
            if (off.Any(o => o < onGrid) && off.Any(o => o > 0.3f && o < 0.7f)) return curves.Count == 2 ? "mixed_grid" : null;
            if (off.All(o => o >= onGrid))
            {
                var near = Fixture.CurveOffsets.Where(x => off.All(o => Mathf.Abs(o - x) < 0.25f * x)).ToList();
                return near.Count == 1 && curves.Count == 1 ? "threshold." + Fixture.CurveOffset(near[0]) : null;
            }
            if (off.Any(o => o >= onGrid)) return null;
            if (clip.length < caseSeconds)
                return curves.Select(c => c.keys.Last().time).Distinct().Count() > 1 ? (AnimationUtility.GetAnimationClipSettings(clip).loopTime ? "end.loop" : "end.once") : null;
            if (AnimationUtility.GetAnimationClipSettings(clip).loopTime) return null;
            var kinds = bindings.Select(Bound).Distinct().ToList();
            return kinds.Count == 1 ? "binding." + kinds[0] : null;
        }

        public static SortedDictionary<string, int> CountGenericCompositions(VRCAvatarDescriptor d)
        {
            var c = new SortedDictionary<string, int>();
            foreach (var k in GenericCompositions) c[k] = 0;
            void Add(string key) => c[key] = c.TryGetValue(key, out var n) ? n + 1 : 1;
            float caseSeconds = Fixture.CaseFrames / (float)Harness.Rate;
            foreach (var (type, ctrl, _) in Controllers(d).Where(x => x.mask == null && Fixture.GenericPlayables.Contains(x.type)))
                foreach (var layer in ctrl.layers.Where(l => l.defaultWeight == 1 && l.avatarMask == null && l.syncedLayerIndex < 0))
                {
                    var sm = layer.stateMachine;
                    var cases = CaseOf(sm);
                    var entered = new HashSet<AnimatorState>(sm.states.SelectMany(x => x.state.transitions)
                        .Where(t => t.conditions.Any(x => x.parameter == Fixture.CaseParameter && x.mode == AnimatorConditionMode.Equals)).Select(t => t.destinationState));
                    foreach (var s in cases.Keys.Where(entered.Contains))
                    {
                        if (layer.blendingMode == AnimatorLayerBlendingMode.Additive)
                        {
                            if (!(Subtracting(s.motion) is AnimationClip from)) continue;
                            string Key(AnimationClip writer, EditorCurveBinding b, string context)
                            {
                                var held = AnimationUtility.GetCurveBindings(AnimationUtility.GetAnimationClipSettings(writer).additiveReferencePoseClip).Contains(b);
                                return Fixture.GenericKey(Fixture.GenericPlayable(type), GenericProperty(b), held ? "reference_with" : "reference_without", context);
                            }
                            var fades = s.transitions.Where(t => t.duration > 0 && On(t, AnimatorConditionMode.If) && t.destinationState is AnimatorState dst && cases.ContainsKey(dst)).ToList();
                            if (fades.Count == 0)
                                foreach (var b in AnimationUtility.GetCurveBindings(from).Where(b => GenericProperty(b) != null)) Add(Key(from, b, "alone"));
                            foreach (var t in fades)
                            {
                                if (!(Subtracting(t.destinationState.motion) is AnimationClip to)) continue;
                                var a = AnimationUtility.GetCurveBindings(from).Where(b => GenericProperty(b) != null).ToList();
                                var z = AnimationUtility.GetCurveBindings(to).Where(b => GenericProperty(b) != null).ToList();
                                foreach (var b in a.Except(z)) Add(Key(from, b, "source_only"));
                                foreach (var b in z.Except(a)) Add(Key(to, b, "destination_only"));
                            }
                        }
                        else if (type == VRCAvatarDescriptor.AnimLayerType.FX && layer.blendingMode == AnimatorLayerBlendingMode.Override && s.motion is AnimationClip clip
                            && Subtracting(clip) == null && s.speed == 1 && !s.speedParameterActive && !s.timeParameterActive && !s.cycleOffsetParameterActive && s.cycleOffset == 0
                            && CurveKind(clip, caseSeconds) is string kind)
                            Add(Fixture.CurveKey(kind, new SerializedObject(clip).FindProperty("m_UseHighQualityCurve").boolValue ? "high" : "low"));
                    }
                }
            return c;
        }

        // The frame grid, write-defaults fill and additive hold probe's compositions, read from the authored assets.
        // "grid.<offset>.<shape>" for a clip without high-quality curves a full-weight override FX layer plays in a
        // write-defaults state, exactly one of whose varying curves lies off its frame grid, by a listed offset: a
        // generic curve alone, beside an on-grid curve with all, one or two inner keys off, stepped beside smooth on-grid
        // curves, or a muscle curve beside on-grid generic ones. "fill.<write_defaults|no_write_defaults>.<above>" for an
        // override FX state animating one position and one scale channel of a node, "additive_above" when an additive
        // layer above it plays a motion in the same case. "hold.idle.write_defaults_then_without" for an additive FX layer
        // whose motion leaves on Pulse for an empty state writing defaults, and that for one without; "hold.fade_out" for
        // an empty state without write defaults, entered from a motion, that fades on Pulse clearing into a motion.
        // "body.additive_over.<turned|turning>" for a Base additive state whose clip turns the body from its reference,
        // over a state of a layer beneath in the same case whose humanoid clip holds or moves a turned body.
        public static readonly string[] GridHoldCompositions =
            (from s in Fixture.GridShapes from o in Fixture.GridOffsets select Fixture.GridKey(o, s))
            .Concat(from s in Fixture.FillStates from a in Fixture.FillAbove select Fixture.FillKey(s, a))
            .Concat(new[] { Fixture.HoldIdleKey, Fixture.HoldFadeKey })
            .Concat(Fixture.BodyBeneath.Select(Fixture.BodyKey))
            .ToArray();

        static bool Muscle(EditorCurveBinding b) => b.type == typeof(Animator) && b.path == "" && System.Array.IndexOf(HumanTrait.MuscleName, b.propertyName) >= 0;

        static (float offset, string shape)? GridShape(AnimationClip clip)
        {
            if (new SerializedObject(clip).FindProperty("m_UseHighQualityCurve").boolValue) return null;
            var bindings = AnimationUtility.GetCurveBindings(clip);
            if (bindings.Any(b => Bound(b) == null && !Muscle(b))) return null;
            var curves = bindings.Select(b => (b, curve: AnimationUtility.GetEditorCurve(clip, b))).Where(x => x.curve.keys.Select(k => k.value).Distinct().Count() > 1).ToList();
            const float onGrid = 2e-5f;
            float Off(Keyframe k) => Mathf.Abs(k.time * clip.frameRate - Mathf.Round(k.time * clip.frameRate));
            var off = curves.Where(x => x.curve.keys.Any(k => Off(k) >= onGrid)).ToList();
            if (off.Count != 1) return null;
            var (binding, curve) = off[0];
            var on = curves.Where(x => x.b != binding).ToList();
            var moved = curve.keys.Where(k => Off(k) >= onGrid).ToList();
            var offsets = Fixture.GridOffsets.Where(o => moved.All(k => Mathf.Abs(Off(k) - o) < 0.01f)).ToList();
            if (offsets.Count != 1) return null;
            string Shape()
            {
                bool Stepped(AnimationCurve c) => c.keys.All(k => float.IsInfinity(k.inTangent) || float.IsInfinity(k.outTangent));
                int inner = curve.length - 2;
                if (Muscle(binding)) return on.Count > 0 && on.All(x => Bound(x.b) != null) && moved.Count == inner ? "muscle" : null;
                if (on.Any(x => Muscle(x.b))) return null;
                if (Stepped(curve)) return on.Count > 1 && on.All(x => !Stepped(x.curve)) && moved.Count == inner ? "stepped" : null;
                if (curve.keys.Any(k => float.IsInfinity(k.inTangent) || float.IsInfinity(k.outTangent))) return null;
                if (on.Count == 0) return moved.Count == inner ? "alone" : null;
                return moved.Count == inner ? "beside_on_grid" : moved.Count == 1 ? "one_key" : moved.Count == 2 ? "two_keys" : null;
            }
            return Shape() is string shape ? (offsets[0], shape) : ((float, string)?)null;
        }

        static bool Fills(AnimationClip clip)
        {
            var b = AnimationUtility.GetCurveBindings(clip);
            return b.Length == 2 && b.All(x => x.type == typeof(Transform)) && b.Select(x => x.path).Distinct().Count() == 1
                && b.Count(x => x.propertyName.StartsWith("m_LocalPosition.")) == 1 && b.Count(x => x.propertyName.StartsWith("m_LocalScale.")) == 1;
        }

        static bool Turned(AnimationClip clip, out bool turning)
        {
            var q = "xyzw".Select(a => AnimationUtility.GetEditorCurve(clip, EditorCurveBinding.FloatCurve("", typeof(Animator), $"RootQ.{a}"))).ToArray();
            turning = false;
            if (q.Any(c => c == null)) return false;
            turning = q.Any(c => c.keys.Any(k => k.value != c.keys[0].value));
            return turning || Quaternion.Angle(new Quaternion(q[0].keys[0].value, q[1].keys[0].value, q[2].keys[0].value, q[3].keys[0].value), Quaternion.identity) > 1;
        }

        public static SortedDictionary<string, int> CountGridHoldCompositions(VRCAvatarDescriptor d)
        {
            var c = new SortedDictionary<string, int>();
            foreach (var k in GridHoldCompositions) c[k] = 0;
            void Add(string key) => c[key] = c.TryGetValue(key, out var n) ? n + 1 : 1;
            foreach (var (type, ctrl, _) in Controllers(d).Where(x => x.mask == null))
            {
                var layers = ctrl.layers.Where(l => l.defaultWeight == 1 && l.avatarMask == null && l.syncedLayerIndex < 0).ToList();
                var cased = layers.Select(l =>
                {
                    var sm = l.stateMachine;
                    var entered = new HashSet<AnimatorState>(sm.states.SelectMany(x => x.state.transitions)
                        .Where(t => t.conditions.Any(x => x.parameter == Fixture.CaseParameter && x.mode == AnimatorConditionMode.Equals)).Select(t => t.destinationState));
                    var of = CaseOf(sm);
                    return (layer: l, of, entered);
                }).ToList();
                bool PlaysAbove(int index, int id, System.Func<AnimatorState, bool> what) =>
                    cased.Skip(index + 1).Any(x => x.layer.blendingMode == AnimatorLayerBlendingMode.Additive && x.of.Any(kv => kv.Value == id && x.entered.Contains(kv.Key) && what(kv.Key)));
                for (int i = 0; i < cased.Count; i++)
                {
                    var (layer, of, entered) = cased[i];
                    foreach (var kv in of)
                    {
                        var s = kv.Key;
                        bool plain = s.speed == 1 && !s.speedParameterActive && !s.timeParameterActive && !s.cycleOffsetParameterActive && s.cycleOffset == 0;
                        if (type == VRCAvatarDescriptor.AnimLayerType.FX && layer.blendingMode == AnimatorLayerBlendingMode.Override && entered.Contains(s) && plain
                            && s.motion is AnimationClip clip && Subtracting(clip) == null)
                        {
                            if (s.writeDefaultValues && GridShape(clip) is { } grid) Add(Fixture.GridKey(grid.offset, grid.shape));
                            if (Fills(clip))
                                Add(Fixture.FillKey(s.writeDefaultValues ? "write_defaults" : "no_write_defaults",
                                    PlaysAbove(i, kv.Value, x => x.motion is AnimationClip) ? "additive_above" : "alone"));
                        }
                        if (type == VRCAvatarDescriptor.AnimLayerType.FX && layer.blendingMode == AnimatorLayerBlendingMode.Additive)
                        {
                            bool Empty(AnimatorState x) => x != null && x.motion == null && of.TryGetValue(x, out var id) && id == kv.Value;
                            if (entered.Contains(s) && Subtracting(s.motion) != null)
                                foreach (var t in s.transitions.Where(t => t.duration == 0 && On(t, AnimatorConditionMode.If) && Empty(t.destinationState) && t.destinationState.writeDefaultValues))
                                    if (t.destinationState.transitions.Any(u => On(u, AnimatorConditionMode.IfNot) && Empty(u.destinationState) && !u.destinationState.writeDefaultValues))
                                        Add(Fixture.HoldIdleKey);
                            if (Empty(s) && !s.writeDefaultValues && of.Keys.Any(x => entered.Contains(x) && Subtracting(x.motion) != null && x.transitions.Any(t => t.destinationState == s && On(t, AnimatorConditionMode.If))))
                                foreach (var t in s.transitions.Where(t => t.duration > 0 && t.duration <= (Fixture.CaseFrames - Fixture.PulseOff) / (float)Harness.Rate
                                    && t.hasFixedDuration && On(t, AnimatorConditionMode.IfNot) && t.destinationState is AnimatorState dst && Subtracting(dst.motion) != null))
                                    Add(Fixture.HoldFadeKey);
                        }
                        if (type == VRCAvatarDescriptor.AnimLayerType.Base && layer.blendingMode == AnimatorLayerBlendingMode.Additive && entered.Contains(s)
                            && Subtracting(s.motion) is AnimationClip turn && Quaternion.Angle(ToQ(RotationOffset(turn)), Quaternion.identity) > 1)
                            foreach (var (beneath, bOf, bEntered) in cased.Take(i).Where(x => x.layer.blendingMode == AnimatorLayerBlendingMode.Override))
                                foreach (var b in bOf.Where(x => x.Value == kv.Value && bEntered.Contains(x.Key)))
                                    if (b.Key.motion is AnimationClip under && under.humanMotion && Turned(under, out var turning))
                                        Add(Fixture.BodyKey(turning ? "turning" : "turned"));
                    }
                }
            }
            return c;
        }

        static Quaternion ToQ(Vector4 v) => new Quaternion(v.x, v.y, v.z, v.w).normalized;

        // The frame-grid rule probe's compositions, read from the authored assets: a 30 Hz clip without high-quality curves
        // a full-weight override FX layer plays in a write-defaults state, exactly one of whose curves has every
        // inner key Fixture.RuleOffset of a frame after the grid. "rule.<kind>.<variant>" when that curve is the clip's only
        // one, bound to the probe's blendshape, node position channel, node scale channel or animator parameter, and keyed
        // every third frame over a listed range (amplitude), every listed number of frames over the spacing family's range
        // at its frame-line error (spacing), or every third frame over the error family's range at a listed frame-line
        // error (error). "rule.mixed.<blendshape|position>_off" for the probe's blendshape and position curves in one clip,
        // both keyed every third frame over the mixed range, the named one off the grid and the other on it.
        public static readonly string[] GridRuleCompositions =
            (from k in Fixture.RuleKinds
             from v in Fixture.RuleAmplitudes.Select(Fixture.RuleAmplitude)
                 .Concat(Fixture.RuleSpacings.Select(Fixture.RuleSpacing))
                 .Concat(Fixture.RuleErrors.Select(Fixture.RuleError))
             select Fixture.RuleKey(k, v))
            .Concat(Fixture.RuleMixed.Select(Fixture.RuleMixedKey))
            .ToArray();

        static string RuleKind(EditorCurveBinding b) =>
            b.type == typeof(Animator) && b.path == "" && b.propertyName == Fixture.RuleParameter ? "parameter"
            : b.type == typeof(SkinnedMeshRenderer) && b.path == "Body" && b.propertyName == "blendShape." + Fixture.RuleShape ? "blendshape"
            : b.type == typeof(Transform) && b.path == Fixture.RuleMover && b.propertyName == "m_LocalPosition.x" ? "position"
            : b.type == typeof(Transform) && b.path == Fixture.RuleScaled && b.propertyName == "m_LocalScale.y" ? "scale"
            : null;

        // A rule curve's key spacing in frames and value range: its ends on the grid, every inner key Fixture.RuleOffset
        // after the grid (or on it, for `onGrid`) and evenly spaced.
        static (int spacing, float range)? RuleKeys(AnimationCurve curve, float rate, bool onGrid)
        {
            const float tolerance = 2e-5f;
            var keys = curve.keys;
            int n = keys.Length - 1, frames = Mathf.RoundToInt(keys[n].time * rate);
            if (n < 2 || keys[0].time != 0 || Mathf.Abs(keys[n].time * rate - frames) >= tolerance || frames % n != 0) return null;
            int spacing = frames / n;
            float offset = onGrid ? 0 : Fixture.RuleOffset;
            for (int i = 1; i < n; i++)
                if (Mathf.Abs(keys[i].time * rate - (spacing * i + offset)) >= (onGrid ? tolerance : 1e-3f)) return null;
            return (spacing, keys.Max(k => k.value) - keys.Min(k => k.value));
        }

        static string RuleComposition(AnimationClip clip)
        {
            if (new SerializedObject(clip).FindProperty("m_UseHighQualityCurve").boolValue || clip.frameRate != Fixture.CurveRate) return null;
            var bindings = AnimationUtility.GetCurveBindings(clip);
            if (bindings.Length == 0 || bindings.Length > 2 || bindings.Any(b => RuleKind(b) == null)) return null;
            float rate = clip.frameRate;
            var curves = bindings.Select(b => (kind: RuleKind(b), curve: AnimationUtility.GetEditorCurve(clip, b))).ToList();
            var off = curves.Select(x => (x.kind, x.curve, keys: RuleKeys(x.curve, rate, onGrid: false))).Where(x => x.keys != null).ToList();
            if (off.Count != 1) return null;
            var (kind, curve, k) = off[0];
            var (spacing, range) = k.Value;
            bool Range(float r) => Mathf.Abs(range - r) <= 1e-3f * r;
            if (curves.Count == 2)
            {
                var on = curves.Single(x => x.curve != curve);
                var onKeys = RuleKeys(on.curve, rate, onGrid: true);
                bool pair = new[] { kind, on.kind }.OrderBy(x => x).SequenceEqual(new[] { "blendshape", "position" });
                bool same = onKeys is { } o && o.spacing == Fixture.RuleKeySpacing && Mathf.Abs(o.range - Fixture.RuleMixedRange) <= 1e-3f * Fixture.RuleMixedRange;
                return pair && same && spacing == Fixture.RuleKeySpacing && Range(Fixture.RuleMixedRange) ? Fixture.RuleMixedKey(kind + "_off") : null;
            }
            float err = Fixture.FrameLineError(curve, rate);
            // A tenth of the closest listed errors' gap.
            const float near = 5e-4f;
            if (spacing == Fixture.RuleKeySpacing && Fixture.RuleAmplitudes.Where(Range).ToList() is var a && a.Count == 1)
                return Fixture.RuleKey(kind, Fixture.RuleAmplitude(a[0]));
            if (Range(Fixture.RuleSpacingRange) && Fixture.RuleSpacings.Contains(spacing) && Mathf.Abs(err - Fixture.RuleSpacingError) < near)
                return Fixture.RuleKey(kind, Fixture.RuleSpacing(spacing));
            if (Range(Fixture.RuleErrorRange) && spacing == Fixture.RuleKeySpacing && Fixture.RuleErrors.Where(x => Mathf.Abs(err - x) < near).ToList() is var e && e.Count == 1)
                return Fixture.RuleKey(kind, Fixture.RuleError(e[0]));
            return null;
        }

        public static SortedDictionary<string, int> CountGridRuleCompositions(VRCAvatarDescriptor d)
        {
            var c = new SortedDictionary<string, int>();
            foreach (var k in GridRuleCompositions) c[k] = 0;
            foreach (var (type, ctrl, _) in Controllers(d).Where(x => x.mask == null && x.type == VRCAvatarDescriptor.AnimLayerType.FX))
                foreach (var layer in ctrl.layers.Where(l => l.defaultWeight == 1 && l.avatarMask == null && l.syncedLayerIndex < 0 && l.blendingMode == AnimatorLayerBlendingMode.Override))
                {
                    var sm = layer.stateMachine;
                    var entered = new HashSet<AnimatorState>(sm.states.SelectMany(x => x.state.transitions)
                        .Where(t => t.conditions.Any(x => x.parameter == Fixture.CaseParameter && x.mode == AnimatorConditionMode.Equals)).Select(t => t.destinationState));
                    foreach (var s in CaseOf(sm).Keys)
                    {
                        bool plain = s.speed == 1 && !s.speedParameterActive && !s.timeParameterActive && !s.cycleOffsetParameterActive && s.cycleOffset == 0;
                        if (entered.Contains(s) && plain && s.writeDefaultValues && s.motion is AnimationClip clip && RuleComposition(clip) is string key)
                            c[key] = c.TryGetValue(key, out var m) ? m + 1 : 1;
                    }
                }
            return c;
        }

        // The generic-rig playable probe's compositions, read from the authored assets of an avatar whose animator has no
        // humanoid avatar. "rig.<playable>.<property>.<context>.<weight>" for each case of a playable's controller: a
        // state its Case value enters in an override layer, at full weight (the controller's first layer, or one at
        // weight 1) or partial (a later layer strictly between 0 and 1), without write defaults, playing a clip that binds
        // the case's own blendshape, node position or animator parameter; under_override when a later override layer
        // strictly between 0 and 1 enters a state of the same case binding the same property, under_additive when a later
        // additive layer does, alone when no other layer has a state of the case. An Action case's state also sets the Action
        // playable's weight (1, or 0.5 for partial_playable), which is otherwise 0.
        public static readonly string[] RigCompositions =
            (from p in Fixture.RigPlayables
             from property in Fixture.RigProperties
             from context in Fixture.RigContexts
             from weight in Fixture.RigWeights(p)
             select Fixture.RigKey(p, property, context, weight)).ToArray();

        static string RigProperty(EditorCurveBinding b, int id) =>
            b.type == typeof(SkinnedMeshRenderer) && b.path == "Body" && b.propertyName == "blendShape." + Fixture.RigShape(id) ? "blendshape"
            : b.type == typeof(Transform) && b.path == Fixture.RigNode(id) && b.propertyName == "m_LocalPosition.y" ? "position"
            : b.type == typeof(Animator) && b.path == "" && b.propertyName == Fixture.RigParameter(id) ? "parameter"
            : null;

        static bool RigPlain(AnimatorState s) =>
            !s.writeDefaultValues && s.speed == 1 && !s.speedParameterActive && !s.timeParameterActive && !s.cycleOffsetParameterActive && !s.mirror && !s.mirrorParameterActive;

        static HashSet<string> RigBound(AnimatorState s, int id) =>
            s.motion is AnimationClip clip
                ? new HashSet<string>(AnimationUtility.GetCurveBindings(clip).Select(b => RigProperty(b, id)).Where(x => x != null))
                : new HashSet<string>();

        public static SortedDictionary<string, int> CountRigCompositions(VRCAvatarDescriptor d)
        {
            var c = new SortedDictionary<string, int>();
            foreach (var k in RigCompositions) c[k] = 0;
            var animator = d.GetComponent<Animator>();
            if (animator == null || animator.avatar == null || animator.avatar.isHuman) return c;
            foreach (var (type, ctrl, _) in Controllers(d).Where(x => x.mask == null && Fixture.RigPlayables.Contains(x.type)))
            {
                var layers = ctrl.layers;
                var floats = new HashSet<string>(ctrl.parameters.Where(p => p.type == AnimatorControllerParameterType.Float).Select(p => p.name));
                var states = layers.Select((l, i) => (layer: l, index: i, of: CaseOf(l.stateMachine))).ToList();
                foreach (var (layer, index, of) in states)
                {
                    if (layer.blendingMode != AnimatorLayerBlendingMode.Override || layer.avatarMask != null || layer.syncedLayerIndex >= 0) continue;
                    string layerWeight = index == 0 || layer.defaultWeight == 1 ? "full" : layer.defaultWeight > 0 && layer.defaultWeight < 1 ? "partial_layer" : null;
                    if (layerWeight == null) continue;
                    foreach (var kv in of)
                    {
                        var (s, id) = (kv.Key, kv.Value);
                        var blendable = Fixture.RigBlendable(type);
                        var sets = s.behaviours.OfType<VRC_PlayableLayerControl>().ToList();
                        if (sets.Count > 1 || sets.Any(b => blendable == null || (int)b.layer != (int)blendable.Value || b.blendDuration != 0)) continue;
                        float? goal = sets.Count == 1 ? sets[0].goalWeight : (float?)null;
                        bool action = type == VRCAvatarDescriptor.AnimLayerType.Action;
                        string weight = goal == null ? (action ? null : layerWeight)
                            : goal == 1 ? (action ? layerWeight : null)
                            : goal > 0 && goal < 1 && layerWeight == "full" ? "partial_playable"
                            : null;
                        if (weight == null || !RigPlain(s)) continue;
                        var bound = RigBound(s, id);
                        if (!floats.Contains(Fixture.RigParameter(id)) || d.expressionParameters?.FindParameter(Fixture.RigParameter(id)) != null) bound.Remove("parameter");
                        var others = states.Where(x => x.index != index).SelectMany(x => x.of.Where(o => o.Value == id).Select(o => (x.layer, x.index, state: o.Key))).ToList();
                        foreach (var property in bound)
                        {
                            string context = null;
                            if (others.Count == 0) context = "alone";
                            else if (others.Count == 1 && others[0] is var other && other.index > index && other.layer.avatarMask == null && RigPlain(other.state) && other.state.behaviours.Length == 0 && RigBound(other.state, id).Contains(property))
                                context = other.layer.blendingMode == AnimatorLayerBlendingMode.Additive && other.layer.defaultWeight == 1 ? "under_additive"
                                    : other.layer.blendingMode == AnimatorLayerBlendingMode.Override && other.layer.defaultWeight > 0 && other.layer.defaultWeight < 1 ? "under_override"
                                    : null;
                            if (context == null) continue;
                            var key = Fixture.RigKey(type, property, context, weight);
                            c[key] = c.TryGetValue(key, out var n) ? n + 1 : 1;
                        }
                    }
                }
            }
            return c;
        }

        static string TreeKey(BlendTree bt) => "blendtree." + Snake(bt.blendType.ToString()).Replace("2_d", "2d").Replace("1_d", "1d");

        static string Snake(string s) => string.Concat(s.Select((ch, i) => i > 0 && (char.IsUpper(ch) || char.IsDigit(ch) && !char.IsDigit(s[i - 1])) ? "_" + char.ToLowerInvariant(ch) : char.ToLowerInvariant(ch).ToString()));
    }
}
