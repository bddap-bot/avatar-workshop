using System.Globalization;
using System.Linq;
using UnityEditor;
using UnityEditor.Animations;
using UnityEngine;
using VRC.SDK3.Avatars.Components;
using VRC.SDK3.Avatars.ScriptableObjects;

namespace AvatarWorkshop.Reference
{
    public static partial class Fixture
    {
        public static readonly int[] SampleTimeExponents = { 0, 1, 2, 3, 4, 5, 6, 7 };
        // Null is the speed parameter.
        public static readonly float?[] SampleTimeSpeeds = { 1, 0.5f, 2, null };
        public static readonly string[] SampleTimeContainers = { "state", "simple_1d", "direct" };
        public static readonly string[] SampleTimeEntries = { "first_frame", "later" };
        // Not an expression parameter, so it keeps its default and the harness's ramps leave it alone.
        public const string SampleSpeed = "SampleSpeed";
        public const float SampleSpeedValue = 0.75f;
        // Frames after the case that enters every later state: Unity's sample times leave n·L from the first seconds on.
        public const int SampleTimeHold = 3000;

        public static string SpeedKey(float? speed) =>
            speed is float s ? "speed_" + s.ToString(CultureInfo.InvariantCulture).Replace('.', '_') : "speed_parameter";

        // Every composition has its own layer and parameter, so all play at once without touching one another, and
        // every later one enters on the same frame, so its entry time is no function of the composition.
        static void SampleTimeLayers(VRCAvatarDescriptor d)
        {
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

            var fx = new Ctl("SampleTimeFX", (CaseParameter, AnimatorControllerParameterType.Int),
                (BlendX, AnimatorControllerParameterType.Float), (BlendY, AnimatorControllerParameterType.Float), (BlendU, AnimatorControllerParameterType.Float));
            AddWeights(fx);
            fx.C.AddParameter(new AnimatorControllerParameter { name = SampleSpeed, type = AnimatorControllerParameterType.Float, defaultFloat = SampleSpeedValue });

            foreach (var entry in SampleTimeEntries)
                foreach (var container in SampleTimeContainers)
                    foreach (var speed in SampleTimeSpeeds)
                        foreach (var e in SampleTimeExponents)
                        {
                            var name = $"{entry} {container} {SpeedKey(speed)} 1e{e}";
                            fx.C.AddParameter(name, AnimatorControllerParameterType.Float);
                            // Unit tangents make each cubic segment exactly t, which Auto tangents would not.
                            float length = Mathf.Pow(10, e);
                            var clip = k.Make(name, c => AnimationUtility.SetEditorCurve(c, EditorCurveBinding.FloatCurve("", typeof(Animator), name),
                                new AnimationCurve(new Keyframe(0, 0, 1, 1), new Keyframe(length, length, 1, 1))), loop: true);
                            // A 1D tree at its first threshold gives that leaf weight exactly one, so its value is unrounded.
                            var motion = container == "simple_1d" ? Tree1D(fx, clip, clip, "W0")
                                : container == "direct" ? Direct(fx, (clip, "W100"))
                                : (Motion)clip;

                            var sm = fx.Layer(name).stateMachine;
                            var s = sm.AddState("Play"); s.writeDefaultValues = true; s.motion = motion;
                            s.speed = speed ?? 1;
                            if (speed == null) { s.speedParameterActive = true; s.speedParameter = SampleSpeed; }
                            if (entry == "first_frame") { sm.defaultState = s; continue; }
                            var off = sm.AddState("Off"); off.writeDefaultValues = true;
                            sm.defaultState = off;
                            Go(off, s, 0).AddCondition(AnimatorConditionMode.Equals, 1, CaseParameter);
                        }

            InstallFx(d, parameters, fx);
        }
    }
}
