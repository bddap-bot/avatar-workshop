using System;
using System.Collections.Generic;
using System.Linq;
using UnityEditor;
using UnityEditor.Animations;
using UnityEngine;
using VRC.SDK3.Avatars.Components;
using VRC.SDK3.Avatars.ScriptableObjects;
using VRC.SDKBase;

namespace AvatarWorkshop.Reference
{
    // The humanoid composition probe: the fixture's body, whose controllers play one composition of humanoid clips
    // at a time, chosen by the int parameter Case (0 plays the rest pose), so each way Unity's human stream composes
    // the body and the IK goals has a reference trace of its own. Every state that plays a clip mirrors on Mirror;
    // blend trees and crossfades read BlendX, BlendY and BlendU, which the harness ramps through each case.
    public static partial class Fixture
    {
        public const string CaseParameter = "Case";
        public const string MirrorParameter = "Mirror";

        static readonly string[] Goals = { "LeftFoot", "RightFoot", "LeftHand", "RightHand" };

        static void HumanLayers(VRCAvatarDescriptor d)
        {
            var clips = new Clips(Dir + "/Clips");
            AssetDatabase.CreateFolder(Dir, "Clips");
            var k = HumanClips(clips);
            var parameters = ScriptableObject.CreateInstance<VRCExpressionParameters>();
            const VRCExpressionParameters.ValueType B = VRCExpressionParameters.ValueType.Bool, I = VRCExpressionParameters.ValueType.Int, F = VRCExpressionParameters.ValueType.Float;
            parameters.parameters = new[]
            {
                P(CaseParameter, I, 0, saved: false), P(MirrorParameter, B, 0, saved: false),
                P("BlendX", F, 0, saved: false, synced: false), P("BlendY", F, 0, saved: false, synced: false), P("BlendU", F, 0, saved: false, synced: false),
            };
            AssetDatabase.CreateAsset(parameters, $"{Dir}/Parameters.asset");

            var built = new Probe(k);
            d.customExpressions = true;
            d.expressionParameters = parameters;
            d.customizeAnimationLayers = true;
            d.baseAnimationLayers = new[]
            {
                Custom(VRCAvatarDescriptor.AnimLayerType.Base, built.Base.C),
                Custom(VRCAvatarDescriptor.AnimLayerType.Additive, built.Additive.C),
                new VRCAvatarDescriptor.CustomAnimLayer { type = VRCAvatarDescriptor.AnimLayerType.Gesture, isDefault = true, isEnabled = true },
                Custom(VRCAvatarDescriptor.AnimLayerType.Action, built.Action.C),
                new VRCAvatarDescriptor.CustomAnimLayer { type = VRCAvatarDescriptor.AnimLayerType.FX, isDefault = true, isEnabled = true },
            };
            d.specialAnimationLayers = new[]
            {
                Custom(VRCAvatarDescriptor.AnimLayerType.Sitting, built.Sitting.C),
                new VRCAvatarDescriptor.CustomAnimLayer { type = VRCAvatarDescriptor.AnimLayerType.TPose, isDefault = true, isEnabled = true },
                new VRCAvatarDescriptor.CustomAnimLayer { type = VRCAvatarDescriptor.AnimLayerType.IKPose, isDefault = true, isEnabled = true },
            };
        }

        sealed class HumanClipSet
        {
            public AnimationClip Rest, Goals, GoalsB, Plain, PlainB, RootY, RootYB, Bob;
            public readonly List<(string name, AnimationClip goals, AnimationClip plain)> Settings = new List<(string, AnimationClip, AnimationClip)>();
        }

        // Every root-motion setting starts from keeping the original rotation, height and XZ, nothing baked and no
        // offset or mirror; each variant changes one of them.
        static void Original(AnimationClipSettings s)
        {
            s.loopBlend = false; s.mirror = false;
            s.loopBlendOrientation = false; s.keepOriginalOrientation = true; s.orientationOffsetY = 0;
            s.loopBlendPositionY = false; s.keepOriginalPositionY = true; s.heightFromFeet = false; s.level = 0;
            s.loopBlendPositionXZ = false; s.keepOriginalPositionXZ = true;
        }

        static readonly (string name, Action<AnimationClipSettings> set)[] RootSettings =
        {
            ("original", s => { }),
            ("loop pose", s => s.loopBlend = true),
            ("mirror", s => s.mirror = true),
            ("rotation baked", s => s.loopBlendOrientation = true),
            ("rotation body", s => s.keepOriginalOrientation = false),
            ("rotation offset", s => s.orientationOffsetY = 25),
            ("height baked", s => s.loopBlendPositionY = true),
            ("height centre of mass", s => s.keepOriginalPositionY = false),
            ("height feet", s => { s.keepOriginalPositionY = false; s.heightFromFeet = true; }),
            ("height offset", s => s.level = 0.08f),
            ("xz baked", s => s.loopBlendPositionXZ = true),
            ("xz centre of mass", s => s.keepOriginalPositionXZ = false),
        };

        // A walking body: it travels, turns, bobs and swings its legs over two seconds; `phase` offsets the B clips.
        static void Moving(AnimationClip c, float phase, bool goals)
        {
            Muscles(c);
            float a = 0.3f + phase;
            Curve(c, "", typeof(Animator), "Left Upper Leg Front-Back", (0, a), (1, -a), (2, a));
            Curve(c, "", typeof(Animator), "Right Upper Leg Front-Back", (0, -a), (1, a), (2, -a));
            Curve(c, "", typeof(Animator), "Spine Twist Left-Right", (0, phase), (2, 0.2f - phase));
            Curve(c, "", typeof(Animator), "RootT.x", (0, 0), (2, 0.15f - phase));
            Curve(c, "", typeof(Animator), "RootT.y", (0, 1), (0.5f, 0.93f + phase * 0.1f), (1, 1.02f), (1.5f, 0.95f), (2, 1));
            Curve(c, "", typeof(Animator), "RootT.z", (0, 0), (2, 0.5f + phase));
            float yaw = (20 + phase * 40) * Mathf.Deg2Rad / 2;
            Curve(c, "", typeof(Animator), "RootQ.y", (0, 0), (2, Mathf.Sin(yaw)));
            Curve(c, "", typeof(Animator), "RootQ.w", (0, 1), (2, Mathf.Cos(yaw)));
            if (!goals) return;
            // Feet planted and hands reaching, so no goal follows the body by any fixed offset.
            var at = new Dictionary<string, (Vector3 from, Vector3 to, float yaw)>
            {
                ["LeftFoot"] = (new Vector3(-0.1f, 0.08f, 0.05f), new Vector3(-0.12f, 0.15f, 0.35f + phase), 10),
                ["RightFoot"] = (new Vector3(0.1f, 0.08f, -0.05f), new Vector3(0.09f, 0.08f, 0.1f), -15),
                ["LeftHand"] = (new Vector3(-0.55f, 1.3f, 0.1f), new Vector3(-0.3f, 1.5f + phase, 0.4f), 30),
                ["RightHand"] = (new Vector3(0.6f, 1.35f, 0), new Vector3(0.4f, 1.1f, -0.2f), -40),
            };
            foreach (var g in Goals)
            {
                var (from, to, deg) = at[g];
                for (int i = 0; i < 3; i++) Curve(c, "", typeof(Animator), $"{g}T.{"xyz"[i]}", (0, from[i]), (1, (from[i] + to[i]) / 2 + 0.05f), (2, to[i]));
                float h = deg * Mathf.Deg2Rad / 2;
                Curve(c, "", typeof(Animator), $"{g}Q.x", (0, 0), (2, 0));
                Curve(c, "", typeof(Animator), $"{g}Q.y", (0, 0), (2, Mathf.Sin(h)));
                Curve(c, "", typeof(Animator), $"{g}Q.z", (0, 0), (2, 0));
                Curve(c, "", typeof(Animator), $"{g}Q.w", (0, 1), (2, Mathf.Cos(h)));
            }
        }

        static HumanClipSet HumanClips(Clips k)
        {
            var s = new HumanClipSet();
            AnimationClip Walk(string name, float phase, bool goals, Action<AnimationClipSettings> set = null) =>
                k.Make(name, c => Moving(c, phase, goals), loop: true, settings: x => { Original(x); set?.Invoke(x); });
            s.Rest = k.Make("Rest", c => Muscles(c));
            s.Goals = Walk("Goals", 0, true);
            s.GoalsB = Walk("GoalsB", 0.15f, true);
            s.Plain = Walk("Plain", 0, false);
            s.PlainB = Walk("PlainB", 0.15f, false);
            foreach (var (name, set) in RootSettings)
                s.Settings.Add((name, Walk("Goals " + name, 0, true, set), Walk("Plain " + name, 0, false, set)));
            s.RootY = k.Make("RootY", c => Curve(c, "", typeof(Animator), "RootT.y", (0, 1), (1, 0.7f), (2, 1)), loop: true, settings: Original);
            s.RootYB = k.Make("RootYB", c => Curve(c, "", typeof(Animator), "RootT.y", (0, 0.2f), (1, 0.4f), (2, 0.2f)), loop: true, settings: Original);
            s.Bob = k.Make("Bob", c =>
            {
                Curve(c, "", typeof(Animator), "Spine Front-Back", (0, 0), (1, 0.3f), (2, 0));
                Curve(c, "", typeof(Animator), "Left Arm Down-Up", (0, 0), (1, 0.4f), (2, 0));
                Curve(c, "", typeof(Animator), "RootT.y", (0, 1), (1, 1.05f), (2, 1));
            }, loop: true, settings: Original);
            return s;
        }

        // The probe's four controllers and the cases they play. A case enters a state in each layer it uses from
        // that layer's Off state and leaves it when Case changes. Base holds one layer, so a clip there plays alone:
        // an idle override layer with a mask pins the parts it masks in. Layered cases play in the Action
        // controller at full weight, whose override and additive layers rest at weight 0 until a case raises them;
        // each playable but Base rests at weight 0.
        sealed class Probe
        {
            public readonly Ctl Base, Additive, Action, Sitting;
            readonly AnimatorStateMachine body, action, over, add, additive, sitting, sittingHalf;
            readonly AnimatorState bodyOff, actionOff, overOff, addOff, additiveOff, sittingOff, sittingHalfOff;
            int next = 1;

            static Ctl Controller(string name) => new Ctl(name, (CaseParameter, AnimatorControllerParameterType.Int), (MirrorParameter, AnimatorControllerParameterType.Bool),
                ("BlendX", AnimatorControllerParameterType.Float), ("BlendY", AnimatorControllerParameterType.Float), ("BlendU", AnimatorControllerParameterType.Float));

            static AnimatorState Off(AnimatorStateMachine sm, Motion rest)
            {
                var s = sm.AddState("Off"); s.writeDefaultValues = false; s.motion = rest;
                sm.defaultState = s;
                return s;
            }

            static void Weights(Ctl c, params float[] weights)
            {
                var layers = c.C.layers;
                for (int i = 0; i < weights.Length; i++) layers[i].defaultWeight = weights[i];
                c.C.layers = layers;
            }

            public Probe(HumanClipSet k)
            {
                Base = Controller("HumanBase");
                body = Base.Layer("Body").stateMachine;
                bodyOff = Off(body, k.Rest); bodyOff.writeDefaultValues = true;
                Action = Controller("HumanAction");
                action = Action.Layer("Action").stateMachine;
                over = Action.Layer("Override", mask: Mask("HumanUpper", new[] { AvatarMaskBodyPart.Root, AvatarMaskBodyPart.Body, AvatarMaskBodyPart.Head, AvatarMaskBodyPart.LeftArm, AvatarMaskBodyPart.LeftFingers })).stateMachine;
                add = Action.Layer("Additive", AnimatorLayerBlendingMode.Additive, Mask("HumanTorso", new[] { AvatarMaskBodyPart.Root, AvatarMaskBodyPart.Body, AvatarMaskBodyPart.LeftArm })).stateMachine;
                Weights(Action, 1, 0, 0);
                actionOff = Off(action, null); overOff = Off(over, null); addOff = Off(add, null);
                PlayableControl(actionOff, VRCPlayableLayerControl.BlendableLayer.Action, 0, 0);
                LayerControl(actionOff, VRCAnimatorLayerControl.BlendableLayer.Action, 1, 0, 0);
                LayerControl(actionOff, VRCAnimatorLayerControl.BlendableLayer.Action, 2, 0, 0);
                Additive = Controller("HumanAdditive");
                additive = Additive.Layer("Additive").stateMachine;
                additiveOff = Off(additive, null);
                PlayableControl(additiveOff, VRCPlayableLayerControl.BlendableLayer.Additive, 0, 0);
                Sitting = Controller("HumanSitting");
                sitting = Sitting.Layer("Sit").stateMachine;
                sittingOff = Off(sitting, null);
                sittingHalf = Sitting.Layer("Half").stateMachine;
                sittingHalfOff = Off(sittingHalf, null);
                Weights(Sitting, 1, 0.5f);

                foreach (var (name, goals, plain) in k.Settings)
                {
                    Case((body, bodyOff, "goals " + name, goals));
                    Case((body, bodyOff, "plain " + name, plain));
                }
                Case((body, bodyOff, "RootT.y only", k.RootY));

                Crossfade("goals to goals", k.Goals, k.GoalsB);
                Crossfade("plain to plain", k.Plain, k.PlainB);
                Crossfade("plain to goals", k.Plain, k.Goals);
                Crossfade("goals to RootT.y only", k.Goals, k.RootY);

                Case((body, bodyOff, "1D plain", Tree1D(Base, k.Plain, k.PlainB)));
                Case((body, bodyOff, "1D goals", Tree1D(Base, k.Goals, k.GoalsB)));
                Case((body, bodyOff, "1D plain and goals", Tree1D(Base, k.Plain, k.Goals)));
                foreach (var type in new[] { BlendTreeType.SimpleDirectional2D, BlendTreeType.FreeformDirectional2D, BlendTreeType.FreeformCartesian2D })
                    Case((body, bodyOff, type.ToString(), Tree2D(type, k)));
                Case((body, bodyOff, "direct plain", Direct(Base, k.Plain, k.PlainB)));
                Case((body, bodyOff, "direct goals", Direct(Base, k.Goals, k.GoalsB)));
                Case((body, bodyOff, "direct RootT.y only", Direct(Base, k.RootY, k.RootYB)));

                Layered(k.Plain);
                Layered(k.Plain, overMotion: ("goals over plain", k.GoalsB));
                Layered(k.Goals, overMotion: ("plain over goals", k.PlainB));
                Layered(k.Goals, overMotion: ("RootT.y only over goals", k.RootY));
                Layered(k.Plain, addMotion: ("bob on plain", k.Bob));
                Layered(k.Plain, addMotion: ("goals on plain", k.GoalsB));
                Layered(k.Goals, addMotion: ("bob on goals", k.Bob));
                // A locomotion system's play-space state: RootT.y-only clips in a direct tree under an override
                // layer, with an additive layer above it.
                Layered(k.Rest, overMotion: ("direct RootT.y only", Direct(Action, k.RootY, k.RootYB)), addMotion: ("bob", k.Bob));
                Layered(k.Goals, overMotion: ("direct RootT.y only", Direct(Action, k.RootY, k.RootYB)), addMotion: ("bob", k.Bob));

                var a = Case((body, bodyOff, "plain", k.Plain), (action, actionOff, "goals half", k.Goals));
                PlayableControl(a[1], VRCPlayableLayerControl.BlendableLayer.Action, 0.5f, 0);
                a = Case((body, bodyOff, "goals", k.Goals), (action, actionOff, "plain half", k.PlainB));
                PlayableControl(a[1], VRCPlayableLayerControl.BlendableLayer.Action, 0.5f, 0);
                a = Case((body, bodyOff, "plain", k.Plain), (additive, additiveOff, "bob half", k.Bob));
                PlayableControl(a[1], VRCPlayableLayerControl.BlendableLayer.Additive, 0.5f, 0);
                a = Case((body, bodyOff, "plain", k.Plain), (additive, additiveOff, "goals half", k.GoalsB));
                PlayableControl(a[1], VRCPlayableLayerControl.BlendableLayer.Additive, 0.5f, 0);
                Case((body, bodyOff, "plain", k.Plain), (sitting, sittingOff, "goals", k.Goals), (sittingHalf, sittingHalfOff, "plain half", k.PlainB));
            }

            // A case in the Action controller at full weight: `under` in its first layer, and the override and
            // additive layers raised to full weight, each playing its motion if given and idle if not.
            void Layered(Motion under, (string name, Motion m)? overMotion = null, (string name, Motion m)? addMotion = null)
            {
                var parts = new List<(AnimatorStateMachine, AnimatorState, string, Motion)> { (action, actionOff, overMotion == null && addMotion == null ? "under an idle override" : "under", under) };
                if (overMotion.HasValue) parts.Add((over, overOff, overMotion.Value.name, overMotion.Value.m));
                if (addMotion.HasValue) parts.Add((add, addOff, addMotion.Value.name, addMotion.Value.m));
                var first = Case(parts.ToArray())[0];
                PlayableControl(first, VRCPlayableLayerControl.BlendableLayer.Action, 1, 0);
                LayerControl(first, VRCAnimatorLayerControl.BlendableLayer.Action, 1, 1, 0);
                LayerControl(first, VRCAnimatorLayerControl.BlendableLayer.Action, 2, 1, 0);
            }

            AnimatorState State(AnimatorStateMachine sm, AnimatorState off, string name, Motion m, int id)
            {
                var s = sm.AddState($"{id} {name}");
                s.writeDefaultValues = false; s.motion = m;
                s.mirrorParameterActive = true; s.mirrorParameter = MirrorParameter;
                Go(off, s, 0).AddCondition(AnimatorConditionMode.Equals, id, CaseParameter);
                Go(s, off, 0).AddCondition(AnimatorConditionMode.NotEqual, id, CaseParameter);
                return s;
            }

            AnimatorState[] Case(params (AnimatorStateMachine sm, AnimatorState off, string name, Motion m)[] parts)
            {
                int id = next++;
                return parts.Select(p => State(p.sm, p.off, p.name, p.m, id)).ToArray();
            }

            void Crossfade(string name, Motion from, Motion to)
            {
                var a = Case((body, bodyOff, name, from))[0];
                var b = body.AddState(a.name + " (to)");
                b.writeDefaultValues = false; b.motion = to;
                b.mirrorParameterActive = true; b.mirrorParameter = MirrorParameter;
                Go(a, b, 0.5f).AddCondition(AnimatorConditionMode.Greater, 0.5f, "BlendX");
                Go(b, bodyOff, 0).AddCondition(AnimatorConditionMode.NotEqual, next - 1, CaseParameter);
            }

            static BlendTree Tree1D(Ctl c, Motion a, Motion b)
            {
                var t = c.Tree("1D", BlendTreeType.Simple1D, "BlendX");
                t.AddChild(a, 0); t.AddChild(b, 1);
                return t;
            }

            BlendTree Tree2D(BlendTreeType type, HumanClipSet k)
            {
                var t = Base.Tree(type.ToString(), type, "BlendU", "BlendY");
                t.AddChild(k.Plain, new Vector2(-1, 0)); t.AddChild(k.Goals, new Vector2(1, 0));
                t.AddChild(k.PlainB, new Vector2(0, 1)); t.AddChild(k.GoalsB, new Vector2(0, -1));
                if (type != BlendTreeType.SimpleDirectional2D) t.AddChild(k.Rest, Vector2.zero);
                return t;
            }

            static BlendTree Direct(Ctl c, Motion a, Motion b)
            {
                var t = c.Tree("Direct", BlendTreeType.Direct, "BlendX");
                t.children = new[]
                {
                    new ChildMotion { motion = a, timeScale = 1, directBlendParameter = "BlendX" },
                    new ChildMotion { motion = b, timeScale = 1, directBlendParameter = "BlendY" },
                };
                return t;
            }
        }
    }
}
