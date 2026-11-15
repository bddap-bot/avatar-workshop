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
    // The crossfade probe: an additive humanoid layer's crossfades and a mirrored state scrubbed by a time parameter
    // came from corpus recordings alone, which cannot be committed.
    public static partial class Fixture
    {
        public static readonly string[] CrossfadeHemispheres = { "same", "other", "negative_alike", "negative_pair" };
        public const float CrossfadeSeconds = 0.5f, CrossfadeNormalized = 0.4f;
        // Longer than PulseOff - PulseOn, so the fade Pulse starts is still running when Pulse clears.
        public const float CrossfadeInterrupted = 2, CrossfadeInterruptedNormalized = 1;
        public static readonly (string hemisphere, bool fixedDuration)[] CrossfadeInterruptedCases = { ("same", true), ("other", true), ("other", false) };
        public static string CrossfadeKey(string hemisphere, bool fixedDuration, bool interrupted) =>
            $"crossfade.{hemisphere}.{(fixedDuration ? "fixed" : "normalized")}{(interrupted ? ".interrupted" : "")}";

        // A walk's time: its own clock (no parameter), a parameter held at its default, or BlendX, which the harness ramps.
        public static readonly float[] ScrubValues = { 0, 0.25f, 0.6f, 1 };
        public static string ScrubParameter(float v) => "Scrub" + Mathf.RoundToInt(v * 100).ToString(CultureInfo.InvariantCulture);
        public static string ScrubTime(float v) => "time_" + Mathf.RoundToInt(v * 100).ToString(CultureInfo.InvariantCulture);
        static (string key, string parameter)[] ScrubTimes(params float[] values) =>
            new[] { ("own_clock", (string)null) }.Concat(values.Select(v => (ScrubTime(v), ScrubParameter(v)))).Append(("time_ramp", BlendX)).ToArray();
        public static readonly (bool mirrored, bool loop, (string key, string parameter)[] times)[] ScrubCases =
        {
            (true, true, ScrubTimes(ScrubValues)),
            (true, false, ScrubTimes(ScrubValues)),
            (false, true, ScrubTimes(0, 0.25f)),
        };
        public static string ScrubKey(bool mirrored, bool loop, string time) => $"scrub.{(mirrored ? "mirrored" : "unmirrored")}.{(loop ? "loop" : "once")}.{time}";

        static readonly Quaternion SourceTurn = Quaternion.Euler(0, 20, 0);
        static readonly Quaternion DestinationTurn = Quaternion.Euler(40, 25, -20);
        static readonly Quaternion ReferenceTurn = Quaternion.Euler(-15, 10, 5);
        static readonly Quaternion ScrubTurn = Quaternion.Euler(0, 30, 10);

        static Quaternion Negated(Quaternion q) => new Quaternion(-q.x, -q.y, -q.z, -q.w);

        static void HeldRotation(AnimationClip c, string prop, Quaternion q)
        {
            for (int i = 0; i < 4; i++) Const(c, "", typeof(Animator), $"{prop}.{"xyzw"[i]}", q[i]);
        }

        static void ReferencePose(AnimationClip c, Quaternion q)
        {
            Muscles(c);
            HeldRotation(c, "RootQ", q);
        }

        static void CrossfadeSource(AnimationClip c, bool smile)
        {
            Muscles(c);
            Curve(c, "", typeof(Animator), "Spine Front-Back", (0, 0), (1, 0.3f), (2, 0));
            Curve(c, "", typeof(Animator), "Chest Left-Right", (0, 0.1f), (1, -0.2f), (2, 0.1f));
            HeldRotation(c, "RootQ", SourceTurn);
            if (smile) Curve(c, "Body", typeof(SkinnedMeshRenderer), "blendShape.Smile", (0, 10), (1, 80), (2, 10));
        }

        static void CrossfadeDestination(AnimationClip c, Quaternion q, bool smile)
        {
            Muscles(c, ("Spine Front-Back", -0.5f), ("Right Arm Down-Up", -0.3f));
            Curve(c, "", typeof(Animator), "Left Upper Leg Front-Back", (0, 0), (2, 0.6f));
            Const(c, "", typeof(Animator), "RootT.y", 0.9f);
            HeldRotation(c, "RootQ", q);
            if (smile) Curve(c, "Body", typeof(SkinnedMeshRenderer), "blendShape.Smile", (0, 60), (2, 20));
        }

        // An asymmetric walk, so a mirrored state differs from the unmirrored one: a looping one, or one that only
        // moves forward through its two seconds.
        static void ScrubWalk(AnimationClip c, bool loop)
        {
            Muscles(c);
            if (loop)
            {
                Curve(c, "", typeof(Animator), "Left Upper Leg Front-Back", (0, 0.5f), (1, -0.2f), (2, 0.5f));
                Curve(c, "", typeof(Animator), "Spine Left-Right", (0, 0.2f), (1, -0.1f), (2, 0.2f));
            }
            else
            {
                Curve(c, "", typeof(Animator), "Left Upper Leg Front-Back", (0, 0), (2, 0.8f));
                Curve(c, "", typeof(Animator), "Spine Left-Right", (0, -0.2f), (2, 0.3f));
            }
            Curve(c, "", typeof(Animator), "RootT.x", (0, 0), (2, 0.1f));
            HeldRotation(c, "RootQ", ScrubTurn);
        }

        static void CrossfadeLayers(VRCAvatarDescriptor d)
        {
            AssetDatabase.CreateFolder(Dir, "Clips");
            var k = new Clips(Dir + "/Clips");
            var rest = k.Make("Rest", c => Muscles(c));
            var restReference = k.Make("Reference rest", c => ReferencePose(c, Quaternion.identity));
            var turnedReference = k.Make("Reference negative", c => ReferencePose(c, Negated(ReferenceTurn)));
            var alikeReference = k.Make("Reference negative alike", c => ReferencePose(c, Negated(DestinationTurn)));
            Action<AnimationClipSettings> Against(AnimationClip reference) => s =>
            {
                s.hasAdditiveReferencePose = true; s.additiveReferencePoseClip = reference; s.additiveReferencePoseTime = 0;
            };
            var source = k.Make("Source", c => CrossfadeSource(c, false), loop: true, settings: Against(restReference));
            var smiling = k.Make("Source smiling", c => CrossfadeSource(c, true), loop: true, settings: Against(restReference));
            var destinations = new Dictionary<string, AnimationClip>
            {
                ["same"] = k.Make("Destination same", c => CrossfadeDestination(c, DestinationTurn, false), settings: Against(restReference)),
                ["other"] = k.Make("Destination other", c => CrossfadeDestination(c, Negated(DestinationTurn), false), settings: Against(restReference)),
                ["negative_alike"] = k.Make("Destination negative alike", c => CrossfadeDestination(c, Negated(DestinationTurn), false), settings: Against(alikeReference)),
                ["negative_pair"] = k.Make("Destination negative pair", c => CrossfadeDestination(c, Negated(DestinationTurn), false), settings: Against(turnedReference)),
            };
            var smilingDestination = k.Make("Destination same smiling", c => CrossfadeDestination(c, DestinationTurn, true), settings: Against(restReference));
            var walks = new Dictionary<bool, AnimationClip>
            {
                [true] = k.Make("Walk loop", c => ScrubWalk(c, true), loop: true),
                [false] = k.Make("Walk once", c => ScrubWalk(c, false)),
            };

            var parameters = ScriptableObject.CreateInstance<VRCExpressionParameters>();
            const VRCExpressionParameters.ValueType B = VRCExpressionParameters.ValueType.Bool, I = VRCExpressionParameters.ValueType.Int, F = VRCExpressionParameters.ValueType.Float;
            parameters.parameters = new[]
                {
                    P(CaseParameter, I, 0, saved: false), P(PulseParameter, B, 0, saved: false),
                    P(BlendX, F, 0, saved: false, synced: false), P(BlendY, F, 0, saved: false, synced: false), P(BlendU, F, 0, saved: false, synced: false),
                }
                .Concat(ScrubValues.Select(v => P(ScrubParameter(v), F, v, saved: false, synced: false))).ToArray();
            AssetDatabase.CreateAsset(parameters, $"{Dir}/Parameters.asset");

            var baseCtl = new Ctl("CrossfadeBase", new[]
                {
                    (CaseParameter, AnimatorControllerParameterType.Int), (PulseParameter, AnimatorControllerParameterType.Bool),
                    (BlendX, AnimatorControllerParameterType.Float), (BlendY, AnimatorControllerParameterType.Float), (BlendU, AnimatorControllerParameterType.Float),
                }
                .Concat(ScrubValues.Select(v => (ScrubParameter(v), AnimatorControllerParameterType.Float))).ToArray());
            var defaults = baseCtl.C.parameters;
            foreach (var v in ScrubValues) defaults.Single(p => p.name == ScrubParameter(v)).defaultFloat = v;
            baseCtl.C.parameters = defaults;

            var body = baseCtl.Layer("Body").stateMachine;
            var bodyOff = body.AddState("Off"); bodyOff.writeDefaultValues = false; bodyOff.motion = rest;
            body.defaultState = bodyOff;
            var additive = baseCtl.Layer("Additive", AnimatorLayerBlendingMode.Additive).stateMachine;
            var additiveOff = additive.AddState("Off"); additiveOff.writeDefaultValues = false;
            additive.defaultState = additiveOff;

            int next = 1;
            AnimatorState Into(AnimatorStateMachine sm, AnimatorState off, int id, string name, Motion m)
            {
                var s = sm.AddState($"{id} {name}");
                s.writeDefaultValues = false; s.motion = m;
                Go(s, off, 0).AddCondition(AnimatorConditionMode.NotEqual, id, CaseParameter);
                return s;
            }
            void Crossfade(string label, AnimationClip from, AnimationClip to, float duration, bool fixedDuration, AnimationClip interruptTo = null)
            {
                int id = next++;
                label = label.Replace('.', ' ');
                var a = CaseState(additive, additiveOff, label + " source", from, id, writeDefaults: false);
                var b = Into(additive, additiveOff, id, label + " destination", to);
                var fade = Go(a, b, duration, fixedDuration);
                fade.AddCondition(AnimatorConditionMode.If, 0, PulseParameter);
                var back = interruptTo == null ? a : Into(additive, additiveOff, id, label + " interrupting", interruptTo);
                if (interruptTo != null) fade.interruptionSource = TransitionInterruptionSource.Destination;
                Go(b, back, CrossfadeSeconds).AddCondition(AnimatorConditionMode.IfNot, 0, PulseParameter);
            }

            // The walks play first: an additive layer idle after a case holds what it last played, which Unity and
            // the core hold apart, and a walk under it would show that instead of its own time.
            foreach (var (mirrored, loop, times) in ScrubCases)
                foreach (var (key, parameter) in times)
                {
                    var s = CaseState(body, bodyOff, ScrubKey(mirrored, loop, key).Replace('.', ' '), walks[loop], next++, writeDefaults: false);
                    s.mirror = mirrored;
                    s.timeParameterActive = parameter != null;
                    if (parameter != null) s.timeParameter = parameter;
                }

            foreach (var fixedDuration in new[] { true, false })
                foreach (var h in CrossfadeHemispheres)
                    Crossfade(CrossfadeKey(h, fixedDuration, false), source, destinations[h], fixedDuration ? CrossfadeSeconds : CrossfadeNormalized, fixedDuration);
            foreach (var (h, fixedDuration) in CrossfadeInterruptedCases)
                Crossfade(CrossfadeKey(h, fixedDuration, true), source, destinations[h], fixedDuration ? CrossfadeInterrupted : CrossfadeInterruptedNormalized, fixedDuration,
                    interruptTo: destinations[h == "same" ? "other" : "same"]);
            Crossfade("generic source only", smiling, destinations["same"], CrossfadeSeconds, true);
            Crossfade("generic destination only", source, smilingDestination, CrossfadeSeconds, true);

            d.customExpressions = true;
            d.expressionParameters = parameters;
            d.customizeAnimationLayers = true;
            d.baseAnimationLayers = new[]
            {
                Custom(VRCAvatarDescriptor.AnimLayerType.Base, baseCtl.C),
                new VRCAvatarDescriptor.CustomAnimLayer { type = VRCAvatarDescriptor.AnimLayerType.Additive, isDefault = true, isEnabled = true },
                new VRCAvatarDescriptor.CustomAnimLayer { type = VRCAvatarDescriptor.AnimLayerType.Gesture, isDefault = true, isEnabled = true },
                new VRCAvatarDescriptor.CustomAnimLayer { type = VRCAvatarDescriptor.AnimLayerType.Action, isDefault = true, isEnabled = true },
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
