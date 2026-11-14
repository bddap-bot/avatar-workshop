using System;
using System.Collections.Generic;
using System.Linq;
using UnityEditor;
using UnityEditor.Animations;
using UnityEngine;
using VRC.SDK3.Avatars.Components;
using VRC.SDK3.Avatars.ScriptableObjects;

namespace AvatarWorkshop.Reference
{
    // The dense clip probe: a corpus emote keyed on every frame of its sample rate matches Unity on its keys and misses
    // between them, so each case plays one clip whose curves are keyed at one density, rate and tangent.
    public static partial class Fixture
    {
        public static readonly string[] DenseKinds = { "humanoid", "generic" };
        public static readonly string[] DenseKeyings = { "every_frame", "most_frames", "every_other_frame", "off_grid" };
        public static readonly int[] DenseRates = { 30, 60 };
        public static readonly string[] DenseTangents = { "flat", "auto", "clamped_auto", "broken" };
        public static readonly float[] DenseSpeeds = { 1, 0.5f };
        public static readonly string[] DenseMuscles = { "Spine Front-Back", "Left Upper Leg Front-Back", "Right Arm Down-Up" };
        public const string DenseParameter = "Dense";
        public const string DenseShape = "Smile";
        // Longer than a case's 150 frames at speed one, so no case reaches the clip's end.
        public const float DenseLength = 3;

        // Frame numbers on the clip's grid, or off it: the grid's first and last frame and, between them, a key 0.37 of
        // a frame after each frame.
        static float[] DenseTimes(string keying, int rate)
        {
            int n = (int)(DenseLength * rate);
            IEnumerable<float> frames;
            switch (keying)
            {
                case "every_frame": frames = Enumerable.Range(0, n + 1).Select(i => (float)i); break;
                case "most_frames": frames = Enumerable.Range(0, n + 1).Where(i => i == n || i % 5 != 3).Select(i => (float)i); break;
                case "every_other_frame": frames = Enumerable.Range(0, n / 2 + 1).Select(i => 2f * i); break;
                case "off_grid": frames = new[] { 0f }.Concat(Enumerable.Range(1, n - 1).Select(i => i + 0.37f)).Append(n); break;
                default: throw new Exception("no keying " + keying);
            }
            return frames.Select(f => f / rate).ToArray();
        }

        // Values drawn per key index, so every composition's curve takes the same values in the same order and only
        // its keying, rate and tangents differ.
        static AnimationCurve DenseCurve(int seed, float scale, string keying, int rate, string tangent)
        {
            var random = new System.Random(seed);
            var keys = DenseTimes(keying, rate).Select(t =>
            {
                float v = scale * (float)(2 * random.NextDouble() - 1);
                float a = scale * (float)(20 * random.NextDouble() - 10), b = scale * (float)(20 * random.NextDouble() - 10);
                return tangent == "broken" ? new Keyframe(t, v, a, b) : new Keyframe(t, v, 0, 0);
            }).ToArray();
            var curve = new AnimationCurve(keys);
            // Setting a mode other than Free breaks a key, and setting one on a joined key smooths its tangents, so a
            // broken key breaks first and an automatic one is joined again after, as the curve editor leaves it.
            var mode = tangent == "auto" ? AnimationUtility.TangentMode.Auto : tangent == "clamped_auto" ? AnimationUtility.TangentMode.ClampedAuto : AnimationUtility.TangentMode.Free;
            for (int i = 0; i < curve.length; i++)
            {
                if (tangent == "broken") AnimationUtility.SetKeyBroken(curve, i, true);
                AnimationUtility.SetKeyLeftTangentMode(curve, i, mode);
                AnimationUtility.SetKeyRightTangentMode(curve, i, mode);
                if (mode != AnimationUtility.TangentMode.Free) AnimationUtility.SetKeyBroken(curve, i, false);
            }
            return curve;
        }

        static void DenseClip(AnimationClip c, string kind, string keying, int rate, string tangent)
        {
            c.frameRate = rate;
            void Set(string path, Type type, string prop, AnimationCurve curve) => AnimationUtility.SetEditorCurve(c, EditorCurveBinding.FloatCurve(path, type, prop), curve);
            // Not Muscles(): its constant curves key 1/60 s, off a 30 Hz grid, and the rule may depend on the whole clip.
            AnimationCurve Hold(float v) => new AnimationCurve(new Keyframe(0, v, 0, 0), new Keyframe(DenseLength, v, 0, 0));
            if (kind == "generic")
            {
                Set("", typeof(Animator), DenseParameter, DenseCurve(1, 1, keying, rate, tangent));
                Set("Body", typeof(SkinnedMeshRenderer), "blendShape." + DenseShape, DenseCurve(2, 50, keying, rate, tangent));
                return;
            }
            foreach (var (p, v) in new[] { ("RootT.x", 0f), ("RootT.y", 1f), ("RootT.z", 0f), ("RootQ.x", 0f), ("RootQ.y", 0f), ("RootQ.z", 0f), ("RootQ.w", 1f) })
                Set("", typeof(Animator), p, Hold(v));
            for (int i = 0; i < HumanTrait.MuscleCount; i++)
            {
                var m = HumanTrait.MuscleName[i];
                int k = Array.IndexOf(DenseMuscles, m);
                Set("", typeof(Animator), Binding(m), k < 0 ? Hold(RestMuscles[i]) : DenseCurve(10 + k, 0.6f, keying, rate, tangent));
            }
            foreach (var m in DenseMuscles) if (Array.IndexOf(HumanTrait.MuscleName, m) < 0) throw new Exception("no humanoid muscle " + m);
        }

        static void DenseLayers(VRCAvatarDescriptor d)
        {
            AssetDatabase.CreateFolder(Dir, "Clips");
            var k = new Clips(Dir + "/Clips");
            var rest = k.Make("Rest", c => Muscles(c));

            var parameters = ScriptableObject.CreateInstance<VRCExpressionParameters>();
            const VRCExpressionParameters.ValueType I = VRCExpressionParameters.ValueType.Int, F = VRCExpressionParameters.ValueType.Float;
            parameters.parameters = new[]
            {
                P(CaseParameter, I, 0, saved: false),
                P(BlendX, F, 0, saved: false, synced: false), P(BlendY, F, 0, saved: false, synced: false), P(BlendU, F, 0, saved: false, synced: false),
            };
            AssetDatabase.CreateAsset(parameters, $"{Dir}/Parameters.asset");

            Ctl Controller(string name) => new Ctl(name, (CaseParameter, AnimatorControllerParameterType.Int),
                (BlendX, AnimatorControllerParameterType.Float), (BlendY, AnimatorControllerParameterType.Float), (BlendU, AnimatorControllerParameterType.Float));
            AnimatorState Off(AnimatorStateMachine sm, Motion m)
            {
                var s = sm.AddState("Off"); s.writeDefaultValues = false; s.motion = m;
                sm.defaultState = s;
                return s;
            }
            var baseCtl = Controller("DenseBase");
            var body = baseCtl.Layer("Body").stateMachine;
            var bodyOff = Off(body, rest);
            var fx = Controller("DenseFX");
            fx.C.AddParameter(DenseParameter, AnimatorControllerParameterType.Float);
            var generic = fx.Layer("Dense").stateMachine;
            var genericOff = Off(generic, null);

            int next = 1;
            foreach (var kind in DenseKinds)
                foreach (var keying in DenseKeyings)
                    foreach (var rate in DenseRates)
                        foreach (var tangent in DenseTangents)
                        {
                            var clip = k.Make($"{kind} {keying} {rate} {tangent}", c => DenseClip(c, kind, keying, rate, tangent));
                            foreach (var speed in DenseSpeeds)
                            {
                                var (sm, off) = kind == "humanoid" ? (body, bodyOff) : (generic, genericOff);
                                CaseState(sm, off, $"{clip.name} {SpeedKey(speed)}", clip, next++, writeDefaults: false).speed = speed;
                            }
                        }

            d.customExpressions = true;
            d.expressionParameters = parameters;
            d.customizeAnimationLayers = true;
            d.baseAnimationLayers = new[]
            {
                Custom(VRCAvatarDescriptor.AnimLayerType.Base, baseCtl.C),
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
