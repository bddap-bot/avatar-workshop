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
    // The rotation probe: the composition probe only yaws, so a goal's pitch and roll never meet a turned body.
    public static partial class Fixture
    {
        static void Emote(AnimationClipSettings s)
        {
            s.loopBlendOrientation = true; s.keepOriginalOrientation = false;
            s.loopBlendPositionY = true; s.level = -0.02f;
            s.loopBlendPositionXZ = true;
        }

        // A property: RootSettings is initialised in another file of this partial class, in an order C# leaves unspecified.
        internal static (string name, Action<AnimationClipSettings> set)[] RotationSettings => RootSettings.Append(("emote", (Action<AnimationClipSettings>)Emote)).ToArray();

        public static readonly string[] RotationKinds = { "moving", "pose" };
        public static readonly string[] RotationAxes = { "x", "z", "xyz" };
        public static readonly string[] RotationPaired = { "original", "emote" };
        public static readonly string[] RotationContexts = { "base_full", "base_partial", "action_full", "action_partial" };
        public static readonly string[] RotationUnnormalizedContexts = { "base_full", "action_full" };
        // The SDK's proxy_die holds its body and goal rotations at about this length.
        public const float RotationUnnormalized = 0.7f;

        static readonly Vector3 BodyTurn = new Vector3(93, 20, 15);
        static readonly Dictionary<string, Vector3> GoalTurns = new Dictionary<string, Vector3>
        {
            ["LeftFoot"] = new Vector3(30, -20, 40), ["RightFoot"] = new Vector3(-45, 25, -30),
            ["LeftHand"] = new Vector3(60, 35, -50), ["RightHand"] = new Vector3(-35, -60, 45),
        };
        static readonly Dictionary<string, Vector3> GoalPlaces = new Dictionary<string, Vector3>
        {
            ["LeftFoot"] = new Vector3(-0.1f, 0.08f, 0.05f), ["RightFoot"] = new Vector3(0.1f, 0.1f, -0.05f),
            ["LeftHand"] = new Vector3(-0.55f, 1.3f, 0.1f), ["RightHand"] = new Vector3(0.6f, 1.35f, 0),
        };

        static Quaternion Turn(Vector3 deg, string axes, float f) =>
            Quaternion.Euler(axes.Contains('x') ? deg.x * f : 0, axes.Contains('y') ? deg.y * f : 0, axes.Contains('z') ? deg.z * f : 0);

        static void Rotation(AnimationClip c, string prop, float scale, params (float t, Quaternion q)[] keys)
        {
            for (int i = 0; i < 4; i++) Curve(c, "", typeof(Animator), $"{prop}.{"xyzw"[i]}", keys.Select(k => (k.t, k.q[i] * scale)).ToArray());
        }

        static void TurningWalk(AnimationClip c, string axes)
        {
            Muscles(c);
            Curve(c, "", typeof(Animator), "Left Upper Leg Front-Back", (0, 0.3f), (1, -0.3f), (2, 0.3f));
            Curve(c, "", typeof(Animator), "Right Upper Leg Front-Back", (0, -0.3f), (1, 0.3f), (2, -0.3f));
            Curve(c, "", typeof(Animator), "RootT.x", (0, 0), (2, 0.15f));
            Curve(c, "", typeof(Animator), "RootT.y", (0, 1), (0.5f, 0.93f), (1, 1.02f), (1.5f, 0.95f), (2, 1));
            Curve(c, "", typeof(Animator), "RootT.z", (0, 0), (2, 0.5f));
            Rotation(c, "RootQ", 1, (0, Quaternion.identity), (1, Turn(BodyTurn, axes, 0.5f)), (2, Turn(BodyTurn, axes, 1)));
            foreach (var g in Goals)
            {
                var p = GoalPlaces[g];
                for (int i = 0; i < 3; i++) Curve(c, "", typeof(Animator), $"{g}T.{"xyz"[i]}", (0, p[i]), (1, p[i] + 0.1f), (2, p[i] + 0.05f * (i + 1)));
                Rotation(c, g + "Q", 1, (0, Quaternion.identity), (1, Turn(GoalTurns[g], axes, 0.5f)), (2, Turn(GoalTurns[g], axes, 1)));
            }
        }

        // Every curve holds one key, at time zero, as an emote pack's zero-length pose does.
        static void TurnedPose(AnimationClip c, string axes, float scale)
        {
            var bend = new Dictionary<string, float> { ["Spine Front-Back"] = 0.4f, ["Left Upper Leg Front-Back"] = 0.5f, ["Right Arm Down-Up"] = -0.3f };
            foreach (var m in bend.Keys) if (Array.IndexOf(HumanTrait.MuscleName, m) < 0) throw new Exception("no humanoid muscle " + m);
            for (int i = 0; i < HumanTrait.MuscleCount; i++)
            {
                var m = HumanTrait.MuscleName[i];
                Curve(c, "", typeof(Animator), Binding(m), (0, bend.TryGetValue(m, out var v) ? v : RestMuscles[i]));
            }
            var at = new Vector3(0.05f, 0.3f, -0.1f);
            for (int i = 0; i < 3; i++) Curve(c, "", typeof(Animator), $"RootT.{"xyz"[i]}", (0, at[i]));
            Rotation(c, "RootQ", scale, (0, Turn(BodyTurn, axes, 1)));
            foreach (var g in Goals)
            {
                for (int i = 0; i < 3; i++) Curve(c, "", typeof(Animator), $"{g}T.{"xyz"[i]}", (0, GoalPlaces[g][i] + 0.1f));
                Rotation(c, g + "Q", scale, (0, Turn(GoalTurns[g], axes, 1)));
            }
        }

        static void RotationLayers(VRCAvatarDescriptor d)
        {
            AssetDatabase.CreateFolder(Dir, "Clips");
            var k = new Clips(Dir + "/Clips");
            var rest = k.Make("Rest", c => Muscles(c));
            var empty = k.Make("Empty", c => { });
            var settings = RotationSettings;
            var made = new Dictionary<string, AnimationClip>();
            AnimationClip Make(string name, Action<AnimationClip> fill, bool loop, string setting)
            {
                if (made.TryGetValue(name, out var c)) return c;
                var set = settings.First(x => x.name == setting).set;
                return made[name] = k.Make(name, fill, loop: loop, settings: s => { Original(s); set(s); });
            }
            AnimationClip Moving(string axes, string setting) => Make($"moving {axes} {setting}", c => TurningWalk(c, axes), true, setting);
            AnimationClip Pose(string axes, string setting, float scale = 1) =>
                Make($"pose {axes} {setting}{(scale != 1 ? " unnormalized" : "")}", c => TurnedPose(c, axes, scale), false, setting);
            AnimationClip Clip(string kind, string axes, string setting) => kind == "moving" ? Moving(axes, setting) : Pose(axes, setting);

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

            // VRChat holds the Base playable at weight one and no layer control reaches its layers, so Base plays a
            // clip at partial weight from a second layer at 0.5. It idles on an empty clip, which gives no human pose.
            var baseCtl = Controller("RotationBase");
            var body = baseCtl.Layer("Body").stateMachine;
            var half = baseCtl.Layer("Half", weight: 0.5f).stateMachine;
            var bodyOff = Off(body, rest);
            var halfOff = Off(half, empty);
            var actionCtl = Controller("RotationAction");
            var action = actionCtl.Layer("Action").stateMachine;
            var actionOff = Off(action, null);
            PlayableControl(actionOff, VRCPlayableLayerControl.BlendableLayer.Action, 0, 0);

            int next = 1;
            void Case(string context, AnimationClip clip)
            {
                int id = next++;
                var (sm, off) = context == "base_full" ? (body, bodyOff) : context == "base_partial" ? (half, halfOff) : (action, actionOff);
                var s = CaseState(sm, off, context + " " + clip.name, clip, id, writeDefaults: false);
                s.mirrorParameterActive = true; s.mirrorParameter = MirrorParameter;
                if (context == "action_full" || context == "action_partial")
                    PlayableControl(s, VRCPlayableLayerControl.BlendableLayer.Action, context == "action_full" ? 1 : 0.5f, 0);
            }

            foreach (var kind in RotationKinds)
            {
                foreach (var (setting, _) in settings) Case("base_full", Clip(kind, "xyz", setting));
                foreach (var axes in RotationAxes.Where(a => a != "xyz"))
                    foreach (var setting in RotationPaired)
                        Case("base_full", Clip(kind, axes, setting));
                foreach (var context in RotationContexts.Where(x => x != "base_full"))
                    foreach (var setting in RotationPaired)
                        Case(context, Clip(kind, "xyz", setting));
            }
            foreach (var context in RotationUnnormalizedContexts) Case(context, Pose("xyz", "emote", RotationUnnormalized));

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
