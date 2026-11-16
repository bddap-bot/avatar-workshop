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
    // The Loop Pose drift probe: the rotation probe's one moving clip under Loop Pose turns about all three axes at
    // once and by one size, so it cannot show how Loop Pose spreads a turn about one axis, or a small or large one.
    public static partial class Fixture
    {
        internal static readonly (string name, Vector3 axis)[] DriftAxes =
        {
            ("x", Vector3.right), ("y", Vector3.up), ("z", Vector3.forward),
            ("xy", new Vector3(1, 1, 0).normalized), ("xyz", Vector3.one.normalized),
        };
        internal static readonly int[] DriftDegrees = { 10, 45, 120 };
        internal static readonly (string name, Action<AnimationClipSettings> set)[] DriftSettings =
        {
            ("unbaked original", s => { }),
            ("unbaked body", s => s.keepOriginalOrientation = false),
            ("baked original", s => s.loopBlendOrientation = true),
            ("baked body", s => { s.loopBlendOrientation = true; s.keepOriginalOrientation = false; }),
        };
        internal static readonly string[] DriftContexts = { "base_full", "action_partial" };
        const float DriftLength = 1;

        // A one-second walk whose body ends turned by `degrees` about `axis`, higher and moved in XZ. Its IK goals turn
        // with the body from rests of their own and end where they started, so the body's turn is the only drift.
        static void DriftingWalk(AnimationClip c, Vector3 axis, float degrees)
        {
            Muscles(c);
            Curve(c, "", typeof(Animator), "Left Upper Leg Front-Back", (0, 0.3f), (0.5f, -0.3f), (DriftLength, 0.3f));
            Curve(c, "", typeof(Animator), "Right Upper Leg Front-Back", (0, -0.3f), (0.5f, 0.3f), (DriftLength, -0.3f));
            Curve(c, "", typeof(Animator), "RootT.x", (0, 0), (DriftLength, 0.12f));
            Curve(c, "", typeof(Animator), "RootT.y", (0, 1), (0.5f, 0.95f), (DriftLength, 1.06f));
            Curve(c, "", typeof(Animator), "RootT.z", (0, 0), (DriftLength, 0.4f));
            Quaternion At(float f) => Quaternion.AngleAxis(degrees * f, axis);
            Rotation(c, "RootQ", 1, (0, At(0)), (0.5f, At(0.5f)), (DriftLength, At(1)));
            foreach (var g in Goals)
            {
                var p = GoalPlaces[g];
                var rest = Quaternion.Euler(GoalTurns[g] * 0.25f);
                for (int i = 0; i < 3; i++) Curve(c, "", typeof(Animator), $"{g}T.{"xyz"[i]}", (0, p[i]), (0.5f, p[i] + 0.05f), (DriftLength, p[i]));
                Rotation(c, g + "Q", 1, (0, At(0) * rest), (0.5f, At(0.5f) * rest), (DriftLength, At(1) * rest));
            }
        }

        static void LoopDriftLayers(VRCAvatarDescriptor d)
        {
            AssetDatabase.CreateFolder(Dir, "Clips");
            var k = new Clips(Dir + "/Clips");
            var rest = k.Make("Rest", c => Muscles(c));

            var parameters = ScriptableObject.CreateInstance<VRCExpressionParameters>();
            const VRCExpressionParameters.ValueType B = VRCExpressionParameters.ValueType.Bool, I = VRCExpressionParameters.ValueType.Int, F = VRCExpressionParameters.ValueType.Float;
            parameters.parameters = new[]
            {
                P(CaseParameter, I, 0, saved: false), P(MirrorParameter, B, 0, saved: false),
                P(BlendX, F, 0, saved: false, synced: false), P(BlendY, F, 0, saved: false, synced: false), P(BlendU, F, 0, saved: false, synced: false),
            };
            AssetDatabase.CreateAsset(parameters, $"{Dir}/Parameters.asset");

            Ctl Controller(string name) => new Ctl(name, (CaseParameter, AnimatorControllerParameterType.Int), (MirrorParameter, AnimatorControllerParameterType.Bool),
                (BlendX, AnimatorControllerParameterType.Float), (BlendY, AnimatorControllerParameterType.Float), (BlendU, AnimatorControllerParameterType.Float));
            AnimatorState Off(AnimatorStateMachine sm, Motion m)
            {
                var s = sm.AddState("Off"); s.writeDefaultValues = false; s.motion = m;
                sm.defaultState = s;
                return s;
            }

            var baseCtl = Controller("DriftBase");
            var body = baseCtl.Layer("Body").stateMachine;
            var bodyOff = Off(body, rest);
            var actionCtl = Controller("DriftAction");
            var action = actionCtl.Layer("Action").stateMachine;
            var actionOff = Off(action, null);
            PlayableControl(actionOff, VRCPlayableLayerControl.BlendableLayer.Action, 0, 0);

            var clips = new List<AnimationClip>();
            foreach (var (axes, axis) in DriftAxes)
                foreach (var degrees in DriftDegrees)
                    foreach (var (setting, set) in DriftSettings)
                        clips.Add(k.Make($"drift {axes} {degrees} {setting}", c => DriftingWalk(c, axis, degrees), loop: true,
                            settings: s => { Original(s); s.loopBlend = true; set(s); }));

            int next = 1;
            foreach (var context in DriftContexts)
                foreach (var clip in clips)
                {
                    int id = next++;
                    var (sm, off) = context == "base_full" ? (body, bodyOff) : (action, actionOff);
                    var s = CaseState(sm, off, context + " " + clip.name, clip, id, writeDefaults: false);
                    s.mirrorParameterActive = true; s.mirrorParameter = MirrorParameter;
                    if (context == "action_partial") PlayableControl(s, VRCPlayableLayerControl.BlendableLayer.Action, 0.5f, 0);
                }

            d.customExpressions = true;
            d.expressionParameters = parameters;
            d.customizeAnimationLayers = true;
            d.baseAnimationLayers = new[]
            {
                Custom(VRCAvatarDescriptor.AnimLayerType.Base, baseCtl.C),
                new VRCAvatarDescriptor.CustomAnimLayer { type = VRCAvatarDescriptor.AnimLayerType.Additive, isDefault = true, isEnabled = true },
                new VRCAvatarDescriptor.CustomAnimLayer { type = VRCAvatarDescriptor.AnimLayerType.Gesture, isDefault = true, isEnabled = true },
                Custom(VRCAvatarDescriptor.AnimLayerType.Action, actionCtl.C),
                new VRCAvatarDescriptor.CustomAnimLayer { type = VRCAvatarDescriptor.AnimLayerType.FX, isDefault = true, isEnabled = true },
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
