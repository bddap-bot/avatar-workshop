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
    // The frame-grid rule probe. No offset tolerance fits the low-quality recordings at hand: blendshape curves keyed
    // every third frame 0.37 of a frame off sample on the frames, a position curve keyed the same way does not, and
    // curves keyed every frame 0.37 off do not either. The one measure consistent with all of them, the frame-sampled
    // line's largest error at a key relative to the curve's range, has too narrow a window to adopt while nothing
    // separates it from amplitude, key spacing and binding kind; each case here varies one of those alone.
    public static partial class Fixture
    {
        public const float RuleOffset = 0.37f;
        public static readonly string[] RuleKinds = { "blendshape", "position", "scale", "parameter" };
        public static readonly float[] RuleAmplitudes = { 1, 100, 0.01f };
        public static readonly int[] RuleSpacings = { 1, 2, 3 };
        public static readonly float[] RuleErrors = { 0.03f, 0.04f, 0.045f, 0.05f, 0.06f };
        public const float RuleSpacingError = 0.045f;
        // Each family's range sets it apart from the others when the counter reads the clips back.
        public const float RuleSpacingRange = 10, RuleErrorRange = 2, RuleMixedRange = 1;
        public const int RuleKeySpacing = 3;
        public static readonly string[] RuleMixed = { "blendshape_off", "position_off" };

        static string Decimal(float v, string format) => v.ToString(format, CultureInfo.InvariantCulture).Replace('.', '_');
        public static string RuleAmplitude(float a) => "amplitude_" + Decimal(a, "0.###");
        public static string RuleSpacing(int s) => "spacing_" + s;
        public static string RuleError(float e) => "error_" + Decimal(e, "0.000");
        public static string RuleKey(string kind, string variant) => $"rule.{kind}.{variant}";
        public static string RuleMixedKey(string off) => $"rule.mixed.{off}";

        public const string RuleShape = "RuleShape", RuleMover = "RuleMover", RuleScaled = "RuleScaled", RuleParameter = "Rule";
        static readonly Vector3 RuleMoverRest = new Vector3(0.2f, 1.0f, 0.3f), RuleScaledRest = new Vector3(-0.2f, 1.1f, 0.3f);

        // The largest distance, over the curve's keys, between a key's value and the line through the curve's values on
        // the two frames around it, over the range of its key values: how far sampling on the frames and joining them
        // linearly misses the keys.
        public static float FrameLineError(AnimationCurve curve, float rate)
        {
            float lo = curve.keys.Min(k => k.value), hi = curve.keys.Max(k => k.value);
            if (hi <= lo) return 0;
            float worst = 0;
            foreach (var k in curve.keys)
            {
                double f = k.time * (double)rate;
                double f0 = Math.Floor(f), f1 = f0 + 1, u = f - f0;
                double line = curve.Evaluate((float)(f0 / rate)) * (1 - u) + curve.Evaluate((float)(f1 / rate)) * u;
                worst = Math.Max(worst, (float)Math.Abs(line - k.value));
            }
            return worst / (hi - lo);
        }

        static int RuleFrames => Mathf.RoundToInt(CurveLength * CurveRate);

        // Keys every `spacing` frames of the 30 Hz grid, the inner ones RuleOffset after it, with values `unit` scaled so
        // the keys span exactly [origin, origin + range].
        static AnimationCurve RuleCurve(int spacing, float offset, IList<double> unit, float origin, float range)
        {
            double lo = unit.Min(), hi = unit.Max();
            int n = RuleFrames / spacing;
            var keys = Enumerable.Range(0, n + 1).Select(i =>
            {
                int f = spacing * i;
                float t = (i == 0 || i == n ? f : f + offset) / CurveRate;
                return new Keyframe(t, origin + range * (float)((unit[i] - lo) / (hi - lo)));
            }).ToArray();
            var curve = new AnimationCurve(keys);
            Smooth(curve);
            return curve;
        }

        static double[] RuleRandom(int seed, int spacing)
        {
            var random = new System.Random(seed);
            return Enumerable.Range(0, RuleFrames / spacing + 1).Select(_ => 2 * random.NextDouble() - 1).ToArray();
        }

        // A ramp with an alternating bend whose size `bend` sets the curvature at each key.
        static double[] BentRamp(double bend, int spacing)
        {
            int n = RuleFrames / spacing;
            return Enumerable.Range(0, n + 1).Select(i => (double)i / n + (i % 2 == 0 ? bend : -bend)).ToArray();
        }

        // The bend at which the bent ramp's frame-line error reaches `target`: the first crossing on a fine scan, then
        // bisected.
        static double BendForError(float target, int spacing)
        {
            float Error(double b) => FrameLineError(RuleCurve(spacing, RuleOffset, BentRamp(b, spacing), 0, 1), CurveRate);
            const int steps = 2000;
            const double top = 2;
            double below = 0;
            for (int i = 1; i <= steps; i++)
            {
                double b = top * i / steps;
                if (Error(b) < target) { below = b; continue; }
                double above = b;
                for (int j = 0; j < 60; j++)
                {
                    double mid = (below + above) / 2;
                    if (Error(mid) < target) below = mid; else above = mid;
                }
                return above;
            }
            throw new Exception($"no bend reaches a frame-line error of {target}");
        }

        static void RuleSet(AnimationClip c, string kind, Func<float, AnimationCurve> curve)
        {
            switch (kind)
            {
                case "blendshape": Set(c, "Body", typeof(SkinnedMeshRenderer), "blendShape." + RuleShape, curve(0)); break;
                case "position": Set(c, RuleMover, typeof(Transform), "m_LocalPosition.x", curve(RuleMoverRest.x)); break;
                case "scale": Set(c, RuleScaled, typeof(Transform), "m_LocalScale.y", curve(1)); break;
                case "parameter": Set(c, "", typeof(Animator), RuleParameter, curve(0)); break;
                default: throw new Exception("no rule kind " + kind);
            }
        }

        static void RuleLayers(VRCAvatarDescriptor d)
        {
            var root = d.transform;
            var smr = root.Find("Body").GetComponent<SkinnedMeshRenderer>();
            var mesh = smr.sharedMesh;
            var delta = new Vector3[mesh.vertexCount];
            mesh.GetBlendShapeFrameVertices(mesh.GetBlendShapeIndex(SampledShape), 0, delta, null, null);
            mesh.AddBlendShapeFrame(RuleShape, 100, delta, null, null);
            smr.SetBlendShapeWeight(mesh.GetBlendShapeIndex(RuleShape), GenericShapeDefault);
            Node(root, RuleMover, RuleMoverRest);
            Node(root, RuleScaled, RuleScaledRest);
            EditorUtility.SetDirty(mesh);

            AssetDatabase.CreateFolder(Dir, "Clips");
            var k = new Clips(Dir + "/Clips");

            var parameters = ScriptableObject.CreateInstance<VRCExpressionParameters>();
            const VRCExpressionParameters.ValueType I = VRCExpressionParameters.ValueType.Int, F = VRCExpressionParameters.ValueType.Float;
            parameters.parameters = new[]
            {
                P(CaseParameter, I, 0, saved: false),
                P(BlendX, F, 0, saved: false, synced: false), P(BlendY, F, 0, saved: false, synced: false), P(BlendU, F, 0, saved: false, synced: false),
            };
            AssetDatabase.CreateAsset(parameters, $"{Dir}/Parameters.asset");
            var fx = new Ctl("RuleFX", (CaseParameter, AnimatorControllerParameterType.Int), (BlendX, AnimatorControllerParameterType.Float),
                (BlendY, AnimatorControllerParameterType.Float), (BlendU, AnimatorControllerParameterType.Float), (RuleParameter, AnimatorControllerParameterType.Float));
            var sm = fx.Layer("Rule").stateMachine;
            var off = sm.AddState("Off");
            off.writeDefaultValues = false;
            sm.defaultState = off;

            int next = 1;
            void Case(string name, Action<AnimationClip> fill)
            {
                var clip = k.Make(name, c => { Quality(c, CurveRate, high: false); fill(c); });
                var err = AnimationUtility.GetCurveBindings(clip).Select(b => FrameLineError(AnimationUtility.GetEditorCurve(clip, b), CurveRate)).Max();
                CaseState(sm, off, $"{name} err_{Decimal(err, "0.0000")}", clip, next++, writeDefaults: true);
            }

            // The frame grid, fill and hold probe's GridOff curve, which sampled on the frames: its seed, spacing and offset.
            var amplitudeShape = RuleRandom(100, RuleKeySpacing);
            var bends = RuleErrors.Select(e => BendForError(e, RuleKeySpacing)).ToArray();
            foreach (var kind in RuleKinds)
            {
                foreach (var a in RuleAmplitudes)
                    Case($"rule {kind} {RuleAmplitude(a)}", c => RuleSet(c, kind, origin => RuleCurve(RuleKeySpacing, RuleOffset, amplitudeShape, origin, a)));
                foreach (var s in RuleSpacings)
                {
                    var bend = BendForError(RuleSpacingError, s);
                    Case($"rule {kind} {RuleSpacing(s)}", c => RuleSet(c, kind, origin => RuleCurve(s, RuleOffset, BentRamp(bend, s), origin, RuleSpacingRange)));
                }
                for (int i = 0; i < RuleErrors.Length; i++)
                {
                    var bend = bends[i];
                    Case($"rule {kind} {RuleError(RuleErrors[i])}", c => RuleSet(c, kind, origin => RuleCurve(RuleKeySpacing, RuleOffset, BentRamp(bend, RuleKeySpacing), origin, RuleErrorRange)));
                }
            }
            foreach (var mixed in RuleMixed)
            {
                bool shapeOff = mixed == "blendshape_off";
                Case($"rule mixed {mixed}", c =>
                {
                    RuleSet(c, "blendshape", origin => RuleCurve(RuleKeySpacing, shapeOff ? RuleOffset : 0, RuleRandom(300, RuleKeySpacing), origin, RuleMixedRange));
                    RuleSet(c, "position", origin => RuleCurve(RuleKeySpacing, shapeOff ? 0 : RuleOffset, RuleRandom(301, RuleKeySpacing), origin, RuleMixedRange));
                });
            }

            d.customExpressions = true;
            d.expressionParameters = parameters;
            d.customizeAnimationLayers = true;
            d.baseAnimationLayers = new[]
            {
                new VRCAvatarDescriptor.CustomAnimLayer { type = VRCAvatarDescriptor.AnimLayerType.Base, isDefault = true, isEnabled = true },
                new VRCAvatarDescriptor.CustomAnimLayer { type = VRCAvatarDescriptor.AnimLayerType.Additive, isDefault = true, isEnabled = true },
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
