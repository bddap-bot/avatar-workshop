using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using UnityEditor;
using UnityEditor.Animations;
using UnityEngine;
using VRC.SDK3.Avatars.Components;
using VRC.SDK3.Avatars.ScriptableObjects;

namespace AvatarWorkshop.Reference
{
    // The generic probe. On the crossfade probe a blendshape only one side of a Base-playable additive fade writes stays
    // at its default, which either rule explains: the Base playable writes no generic property, or an additive offset
    // without a reference value adds nothing. And clips without high-quality curves were measured only on muscle and
    // blendshape curves.
    public static partial class Fixture
    {
        public static readonly VRCAvatarDescriptor.AnimLayerType[] GenericPlayables =
            { VRCAvatarDescriptor.AnimLayerType.Base, VRCAvatarDescriptor.AnimLayerType.Additive, VRCAvatarDescriptor.AnimLayerType.FX };
        public static string GenericPlayable(VRCAvatarDescriptor.AnimLayerType t) => t.ToString().ToLowerInvariant();
        public static readonly string[] GenericProperties = { "blendshape", "transform" };
        public static readonly string[] GenericReferences = { "reference_with", "reference_without" };
        public static readonly string[] GenericContexts = { "alone", "source_only", "destination_only" };
        public static string GenericKey(string playable, string property, string reference, string context) => $"additive.{playable}.{property}.{reference}.{context}";

        // Each additive case probes a blendshape and a node no other case animates, as an idle additive layer goes on
        // holding what it last added.
        public static string ProbeShape(int id) => "Probe" + id;
        public static string ProbeNode(int id) => "Marker" + id;
        public const string SampledShape = "Pout", SampledNode = "Sampled";
        // Off zero, so a property held at its default differs from one written to zero.
        public const float GenericShapeDefault = 15;
        static string Common(VRCAvatarDescriptor.AnimLayerType t) => "Common" + t;

        public const int CurveRate = 30;
        // Longer than a case, so only the clip-end cases reach their clip's end.
        public const float CurveLength = 3, CurveEndLength = 1.5f;
        // How far a threshold case's inner keys lie after the frame grid, in frames.
        public static readonly float[] CurveOffsets = { 1e-4f, 5e-4f, 2e-3f, 1e-2f, 0.1f };
        public static string CurveOffset(float o) => "offset_" + o.ToString("0e0", CultureInfo.InvariantCulture);
        public static readonly string[] CurveBindings = { "position", "rotation", "euler", "scale", "blendshape" };
        public static readonly string[] CurveKinds =
            CurveBindings.Select(b => "binding." + b)
                .Concat(new[] { "stepped.all", "stepped.some", "mixed_grid" })
                .Concat(CurveOffsets.Select(o => "threshold." + CurveOffset(o)))
                .Concat(new[] { "end.once", "end.loop" }).ToArray();
        public static readonly string[] CurveQualities = { "low", "high" };
        public static string CurveKey(string kind, string quality) => $"curves.{kind}.quality_{quality}";

        static readonly Vector3 SampledRest = new Vector3(0.3f, 1.1f, 0.25f);

        static Transform Node(Transform root, string name, Vector3 at)
        {
            var go = new GameObject(name);
            go.transform.SetParent(root, false);
            go.transform.localPosition = at;
            return go.transform;
        }

        static void Smooth(AnimationCurve curve)
        {
            for (int i = 0; i < curve.length; i++)
            {
                AnimationUtility.SetKeyLeftTangentMode(curve, i, AnimationUtility.TangentMode.ClampedAuto);
                AnimationUtility.SetKeyRightTangentMode(curve, i, AnimationUtility.TangentMode.ClampedAuto);
            }
        }

        static void Set(AnimationClip c, string path, Type type, string prop, AnimationCurve curve) =>
            AnimationUtility.SetEditorCurve(c, EditorCurveBinding.FloatCurve(path, type, prop), curve);

        // Seeded values every third frame of the grid, the inner keys `offset` frames after it.
        static AnimationCurve Sampled(int seed, float centre, float scale, float length, float offset = 0)
        {
            var random = new System.Random(seed);
            int frames = Mathf.RoundToInt(length * CurveRate);
            var keys = Enumerable.Range(0, frames / 3 + 1).Select(i =>
            {
                int f = 3 * i;
                float t = (f == 0 || f == frames ? f : f + offset) / CurveRate;
                return new Keyframe(t, centre + scale * (float)(2 * random.NextDouble() - 1));
            }).ToArray();
            var curve = new AnimationCurve(keys);
            Smooth(curve);
            return curve;
        }

        static void SampledPosition(AnimationClip c, int seed, float length, float offset = 0, string axes = "x")
        {
            foreach (var a in axes)
            {
                int i = "xyz".IndexOf(a);
                Set(c, SampledNode, typeof(Transform), "m_LocalPosition." + a, Sampled(seed + i, SampledRest[i], 0.1f, length, offset));
            }
        }

        static void CurveClip(AnimationClip c, string kind, string quality)
        {
            Quality(c, CurveRate, quality == "high");
            switch (kind)
            {
                case "binding.position": SampledPosition(c, 1, CurveLength, axes: "xyz"); break;
                case "binding.euler":
                    for (int i = 0; i < 3; i++) Set(c, SampledNode, typeof(Transform), "localEulerAnglesRaw." + "xyz"[i], Sampled(20 + i, 0, 70, CurveLength));
                    break;
                case "binding.rotation":
                {
                    var euler = Enumerable.Range(0, 3).Select(i => Sampled(30 + i, 0, 70, CurveLength)).ToArray();
                    var q = euler[0].keys.Select(k => Quaternion.Euler(euler[0].Evaluate(k.time), euler[1].Evaluate(k.time), euler[2].Evaluate(k.time))).ToList();
                    for (int i = 1; i < q.Count; i++) if (Quaternion.Dot(q[i - 1], q[i]) < 0) q[i] = new Quaternion(-q[i].x, -q[i].y, -q[i].z, -q[i].w);
                    for (int a = 0; a < 4; a++)
                    {
                        var curve = new AnimationCurve(euler[0].keys.Select((k, i) => new Keyframe(k.time, q[i][a])).ToArray());
                        Smooth(curve);
                        Set(c, SampledNode, typeof(Transform), "m_LocalRotation." + "xyzw"[a], curve);
                    }
                    break;
                }
                case "binding.scale":
                    for (int i = 0; i < 3; i++) Set(c, SampledNode, typeof(Transform), "m_LocalScale." + "xyz"[i], Sampled(40 + i, 1, 0.3f, CurveLength));
                    break;
                case "binding.blendshape": Set(c, "Body", typeof(SkinnedMeshRenderer), "blendShape." + SampledShape, Sampled(50, 50, 50, CurveLength)); break;
                case "stepped.all":
                case "stepped.some":
                {
                    var curve = Sampled(60, SampledRest.x, 0.1f, CurveLength);
                    for (int i = 0; i < curve.length; i++)
                    {
                        if (kind == "stepped.some" && i % 2 == 1) continue;
                        AnimationUtility.SetKeyBroken(curve, i, true);
                        AnimationUtility.SetKeyLeftTangentMode(curve, i, AnimationUtility.TangentMode.Constant);
                        AnimationUtility.SetKeyRightTangentMode(curve, i, AnimationUtility.TangentMode.Constant);
                    }
                    Set(c, SampledNode, typeof(Transform), "m_LocalPosition.x", curve);
                    break;
                }
                case "mixed_grid":
                    SampledPosition(c, 70, CurveLength, axes: "x");
                    SampledPosition(c, 70, CurveLength, offset: 0.37f, axes: "y");
                    break;
                case "end.once":
                case "end.loop":
                    // The position's last key 0.4 s before the clip's end, which the blendshape's sets.
                    SampledPosition(c, 80, CurveEndLength - 12f / CurveRate);
                    Set(c, "Body", typeof(SkinnedMeshRenderer), "blendShape." + SampledShape, Sampled(81, 50, 50, CurveEndLength));
                    break;
                default:
                    var o = CurveOffsets.Single(x => kind == "threshold." + CurveOffset(x));
                    SampledPosition(c, 90, CurveLength, offset: o);
                    break;
            }
            // Under write defaults Unity writes 0 to a position channel a clip leaves out beside one it animates, so the others hold rest.
            var position = AnimationUtility.GetCurveBindings(c).Where(b => b.propertyName.StartsWith("m_LocalPosition.")).Select(b => b.propertyName).ToList();
            float length = c.length;
            if (position.Count > 0)
                for (int i = 0; i < 3; i++)
                    if (!position.Contains("m_LocalPosition." + "xyz"[i]))
                        Set(c, SampledNode, typeof(Transform), "m_LocalPosition." + "xyz"[i], new AnimationCurve(new Keyframe(0, SampledRest[i]), new Keyframe(length, SampledRest[i])));
        }

        static void GenericLayers(VRCAvatarDescriptor d)
        {
            var root = d.transform;
            var smr = root.Find("Body").GetComponent<SkinnedMeshRenderer>();
            var mesh = smr.sharedMesh;
            var delta = new Vector3[mesh.vertexCount];
            mesh.GetBlendShapeFrameVertices(mesh.GetBlendShapeIndex(SampledShape), 0, delta, null, null);
            void Shaped(string shape)
            {
                if (mesh.GetBlendShapeIndex(shape) < 0) mesh.AddBlendShapeFrame(shape, 100, delta, null, null);
                smr.SetBlendShapeWeight(mesh.GetBlendShapeIndex(shape), GenericShapeDefault);
            }
            Shaped(SampledShape);
            for (int i = 0; i < GenericPlayables.Length; i++) Node(root, Common(GenericPlayables[i]), new Vector3(-0.3f + 0.3f * i, 0.8f, 0.2f));
            Node(root, SampledNode, SampledRest);
            EditorUtility.SetDirty(mesh);

            AssetDatabase.CreateFolder(Dir, "Clips");
            var k = new Clips(Dir + "/Clips");
            var rest = k.Make("Rest", c => Muscles(c));

            var parameters = ScriptableObject.CreateInstance<VRCExpressionParameters>();
            const VRCExpressionParameters.ValueType B = VRCExpressionParameters.ValueType.Bool, I = VRCExpressionParameters.ValueType.Int, F = VRCExpressionParameters.ValueType.Float;
            parameters.parameters = new[]
            {
                P(CaseParameter, I, 0, saved: false), P(PulseParameter, B, 0, saved: false),
                P(BlendX, F, 0, saved: false, synced: false), P(BlendY, F, 0, saved: false, synced: false), P(BlendU, F, 0, saved: false, synced: false),
            };
            AssetDatabase.CreateAsset(parameters, $"{Dir}/Parameters.asset");
            Ctl Controller(string name) => new Ctl(name, (CaseParameter, AnimatorControllerParameterType.Int), (PulseParameter, AnimatorControllerParameterType.Bool),
                (BlendX, AnimatorControllerParameterType.Float), (BlendY, AnimatorControllerParameterType.Float), (BlendU, AnimatorControllerParameterType.Float));
            AnimatorState Off(AnimatorStateMachine sm, Motion m)
            {
                var s = sm.AddState("Off"); s.writeDefaultValues = false; s.motion = m;
                sm.defaultState = s;
                return s;
            }

            var controllers = new Dictionary<VRCAvatarDescriptor.AnimLayerType, Ctl>();
            int next = 1;
            foreach (var type in GenericPlayables)
            {
                var ctl = controllers[type] = Controller("Generic" + type);
                if (type == VRCAvatarDescriptor.AnimLayerType.Base) Off(ctl.Layer("Body").stateMachine, rest);
                var additive = ctl.Layer("Additive", AnimatorLayerBlendingMode.Additive).stateMachine;
                var off = Off(additive, null);
                string common = Common(type);
                var commonRest = root.Find(common).localPosition;
                string shape = null, marker = null;
                var markerRest = new Vector3(0, 1.3f, 0.2f);
                void Probing(int id)
                {
                    shape = ProbeShape(id); marker = ProbeNode(id);
                    Shaped(shape);
                    Node(root, marker, markerRest);
                }
                void Probed(AnimationClip c, string property, bool destination)
                {
                    if (property == "blendshape")
                        Curve(c, "Body", typeof(SkinnedMeshRenderer), "blendShape." + shape, destination ? new[] { (0f, 60f), (2f, 20f) } : new[] { (0f, 10f), (1f, 80f), (2f, 10f) });
                    else
                        Curve(c, marker, typeof(Transform), "m_LocalPosition.y", destination
                            ? new[] { (0f, markerRest.y + 0.3f), (2f, markerRest.y + 0.1f) } : new[] { (0f, markerRest.y + 0.05f), (1f, markerRest.y + 0.2f), (2f, markerRest.y + 0.05f) });
                }
                void Shared(AnimationClip c, bool destination) => Curve(c, common, typeof(Transform), "m_LocalPosition.z",
                    destination ? new[] { (0f, commonRest.z + 0.2f), (2f, commonRest.z) } : new[] { (0f, commonRest.z + 0.1f), (1f, commonRest.z - 0.1f), (2f, commonRest.z + 0.1f) });
                // Every reference holds the shared property at rest; one holding the probed property holds it off its default.
                AnimationClip Holding(string property) => k.Make($"{shape} reference", c =>
                {
                    Const(c, common, typeof(Transform), "m_LocalPosition.z", commonRest.z);
                    if (property == "blendshape") Shape(c, shape, 25);
                    else Const(c, marker, typeof(Transform), "m_LocalPosition.y", markerRest.y + 0.1f);
                });
                var sharedOnly = k.Make($"{type} reference shared", c => Const(c, common, typeof(Transform), "m_LocalPosition.z", commonRest.z));
                Action<AnimationClipSettings> Against(AnimationClip r) => s =>
                {
                    s.hasAdditiveReferencePose = true; s.additiveReferencePoseClip = r; s.additiveReferencePoseTime = 0;
                };
                foreach (var property in GenericProperties)
                    foreach (var withReference in new[] { true, false })
                    {
                        string label = $"{GenericPlayable(type)} {property} {(withReference ? "reference_with" : "reference_without")}";
                        Probing(next);
                        var alone = k.Make($"{label} alone", c => Probed(c, property, false), loop: true, settings: Against(withReference ? Holding(property) : sharedOnly));
                        CaseState(additive, off, $"{label} alone", alone, next++, writeDefaults: false);
                        foreach (var sourceWrites in new[] { true, false })
                        {
                            Probing(next);
                            var against = Against(withReference ? Holding(property) : sharedOnly);
                            var from = k.Make($"{label} {(sourceWrites ? "source_only" : "destination_only")} source", c =>
                            {
                                Shared(c, false);
                                if (sourceWrites) Probed(c, property, false);
                            }, loop: true, settings: sourceWrites ? against : Against(sharedOnly));
                            var to = k.Make($"{label} {(sourceWrites ? "source_only" : "destination_only")} destination", c =>
                            {
                                Shared(c, true);
                                if (!sourceWrites) Probed(c, property, true);
                            }, settings: sourceWrites ? Against(sharedOnly) : against);
                            int id = next++;
                            string name = $"{label} {(sourceWrites ? "source_only" : "destination_only")}";
                            var a = CaseState(additive, off, name + " source", from, id, writeDefaults: false);
                            var b = additive.AddState($"{id} {name} destination");
                            b.writeDefaultValues = false; b.motion = to;
                            Go(b, off, 0).AddCondition(AnimatorConditionMode.NotEqual, id, CaseParameter);
                            Go(a, b, CrossfadeSeconds).AddCondition(AnimatorConditionMode.If, 0, PulseParameter);
                            Go(b, a, CrossfadeSeconds).AddCondition(AnimatorConditionMode.IfNot, 0, PulseParameter);
                        }
                    }
            }

            var fx = controllers[VRCAvatarDescriptor.AnimLayerType.FX];
            var curves = fx.Layer("Curves").stateMachine;
            var curvesOff = Off(curves, null);
            foreach (var kind in CurveKinds)
                foreach (var quality in CurveQualities)
                {
                    var clip = k.Make($"curves {kind} {quality}", c => CurveClip(c, kind, quality), loop: kind == "end.loop");
                    // Writing defaults, so no case holds a channel of the node an earlier one left.
                    CaseState(curves, curvesOff, clip.name, clip, next++, writeDefaults: true);
                }

            d.customExpressions = true;
            d.expressionParameters = parameters;
            d.customizeAnimationLayers = true;
            d.baseAnimationLayers = new[]
            {
                Custom(VRCAvatarDescriptor.AnimLayerType.Base, controllers[VRCAvatarDescriptor.AnimLayerType.Base].C),
                Custom(VRCAvatarDescriptor.AnimLayerType.Additive, controllers[VRCAvatarDescriptor.AnimLayerType.Additive].C),
                new VRCAvatarDescriptor.CustomAnimLayer { type = VRCAvatarDescriptor.AnimLayerType.Gesture, isDefault = true, isEnabled = true },
                new VRCAvatarDescriptor.CustomAnimLayer { type = VRCAvatarDescriptor.AnimLayerType.Action, isDefault = true, isEnabled = true },
                Custom(VRCAvatarDescriptor.AnimLayerType.FX, fx.C),
            };
            d.specialAnimationLayers = new[]
            {
                new VRCAvatarDescriptor.CustomAnimLayer { type = VRCAvatarDescriptor.AnimLayerType.Sitting, isDefault = true, isEnabled = true },
                new VRCAvatarDescriptor.CustomAnimLayer { type = VRCAvatarDescriptor.AnimLayerType.TPose, isDefault = true, isEnabled = true },
                new VRCAvatarDescriptor.CustomAnimLayer { type = VRCAvatarDescriptor.AnimLayerType.IKPose, isDefault = true, isEnabled = true },
            };
        }
    }
}
