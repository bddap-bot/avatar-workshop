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
        public const string PulseParameter = "Pulse";
        public const int PulseOn = 30, PulseOff = 90;

        // Each layer animates blendshapes no other layer binds, and no tree feeds an earlier case's value back into a
        // shape it writes: Unity plays a shape's last value, from any earlier case, back into a state without write
        // defaults.
        static void LayeringLayers(VRCAvatarDescriptor d)
        {
            AssetDatabase.CreateFolder(Dir, "Clips");
            var k = new Clips(Dir + "/Clips");
            int clip = 0;
            AnimationClip Held(params (string shape, float v)[] sets) => k.Make("Held" + clip++, c =>
            {
                foreach (var (shape, v) in sets)
                    AnimationUtility.SetEditorCurve(c, EditorCurveBinding.FloatCurve("Body", typeof(SkinnedMeshRenderer), "blendShape." + shape), new AnimationCurve(new Keyframe(0, v)));
            });
            AnimationClip Long(string shape, float seconds) =>
                k.Make("Long" + clip++, c => Curve(c, "Body", typeof(SkinnedMeshRenderer), "blendShape." + shape, (0, 0), (seconds, 100)), loop: true);

            var parameters = ScriptableObject.CreateInstance<VRCExpressionParameters>();
            const VRCExpressionParameters.ValueType B = VRCExpressionParameters.ValueType.Bool, I = VRCExpressionParameters.ValueType.Int, F = VRCExpressionParameters.ValueType.Float;
            parameters.parameters = new[]
            {
                P(CaseParameter, I, 0, saved: false), P(PulseParameter, B, 0, saved: false),
                P(BlendX, F, 0, saved: false, synced: false), P(BlendY, F, 0, saved: false, synced: false), P(BlendU, F, 0, saved: false, synced: false),
            };
            AssetDatabase.CreateAsset(parameters, $"{Dir}/Parameters.asset");

            // One controller: FX resets what a lower playable animates on targets it does not bind, so a toggle in
            // Gesture showed nothing.
            var fx = new Ctl("LayeringFX", (CaseParameter, AnimatorControllerParameterType.Int), (PulseParameter, AnimatorControllerParameterType.Bool),
                (BlendX, AnimatorControllerParameterType.Float), (BlendY, AnimatorControllerParameterType.Float), (BlendU, AnimatorControllerParameterType.Float));
            AddWeights(fx);
            const string O = "W0", Q = "W25", H = "W50", W = "W100";
            int next = 1;

            AnimatorState Rest(AnimatorStateMachine sm, string name, bool writeDefaults)
            {
                var s = sm.AddState(name); s.writeDefaultValues = writeDefaults;
                if (sm.defaultState == null) sm.defaultState = s;
                return s;
            }
            AnimatorState State(AnimatorStateMachine sm, int id, string name, bool writeDefaults, Motion m)
            {
                var s = sm.AddState($"{id} {name}"); s.writeDefaultValues = writeDefaults; s.motion = m;
                return s;
            }
            void OnCase(AnimatorStateTransition t, int id, bool pulse)
            {
                t.AddCondition(AnimatorConditionMode.Equals, id, CaseParameter);
                if (pulse) t.AddCondition(AnimatorConditionMode.If, 0, PulseParameter);
            }

            // A write-defaults toggle: from a state with no motion into one holding a shape at 100 while Pulse is
            // set, and back when it clears. Each case holds its own shape, so each starts from the shape's default.
            void Toggle(string name, params (int id, string shape)[] cases)
            {
                var sm = fx.Layer(name).stateMachine;
                var off = Rest(sm, "Off", true);
                foreach (var (id, shape) in cases)
                {
                    var s = State(sm, id, "On", true, Held((shape, 100)));
                    OnCase(Go(off, s, 0), id, pulse: true);
                    Go(s, off, 0).AddCondition(AnimatorConditionMode.IfNot, 0, PulseParameter);
                }
            }
            // A layer without write defaults for the length of a case, in a state that holds its shape or in one
            // with no motion.
            void Context(string name, string shape, params (int id, bool motion)[] cases)
            {
                var sm = fx.Layer(name).stateMachine;
                var idle = Rest(sm, "Idle", true);
                var held = Held((shape, 100));
                foreach (var (id, motion) in cases)
                {
                    var s = State(sm, id, motion ? "Held" : "Empty", false, motion ? held : null);
                    OnCase(Go(idle, s, 0), id, pulse: false);
                    Go(s, idle, 0).AddCondition(AnimatorConditionMode.NotEqual, id, CaseParameter);
                }
            }

            int firstHeld = next++, firstEmpty = next++;
            Toggle("ToggleFirst", (firstHeld, Faces[1]), (firstEmpty, Faces[2]));
            Context("UnderFirst", Faces[3], (firstHeld, true), (firstEmpty, false));
            Toggle("ToggleAlone", (next++, Faces[0]));
            int aboveHeld = next++, aboveEmpty = next++;
            Context("BeneathToggle", Faces[4], (aboveHeld, true), (aboveEmpty, false));
            Toggle("ToggleAbove", (aboveHeld, Faces[5]), (aboveEmpty, Faces[6]));
            int belowHeld = next++, belowEmpty = next++;
            Toggle("ToggleBelow", (belowHeld, Faces[7]), (belowEmpty, Faces[8]));
            Context("OverToggle", Faces[9], (belowHeld, true), (belowEmpty, false));

            {
                var sm = fx.Layer("EmptyAfterMotion").stateMachine;
                var idle = Rest(sm, "Idle", true);
                var id = next++;
                var s = State(sm, id, "Held", false, Held((Faces[10], 100)));
                var empty = State(sm, id, "Empty", false, null);
                OnCase(Go(idle, s, 0), id, pulse: true);
                Go(s, empty, 0).AddCondition(AnimatorConditionMode.IfNot, 0, PulseParameter);
                Go(empty, idle, 0).AddCondition(AnimatorConditionMode.NotEqual, id, CaseParameter);
            }

            {
                var sm = fx.Layer("TreeToEmpty").stateMachine;
                var off = Rest(sm, "Off", true);
                var bare = sm.AddState("OffNoDefaults"); bare.writeDefaultValues = false;
                var shape = Faces[11];
                AnimationClip At(float v) => Held((shape, v));
                void Leave(string name, Motion tree, AnimatorState into)
                {
                    var id = next++;
                    var s = State(sm, id, name, true, tree);
                    OnCase(Go(off, s, 0), id, pulse: true);
                    Go(s, into, 0).AddCondition(AnimatorConditionMode.IfNot, 0, PulseParameter);
                    if (into != off) Go(into, off, 0).AddCondition(AnimatorConditionMode.NotEqual, id, CaseParameter);
                }
                Leave("Direct", Direct(fx, (At(40), H), (At(80), H)), off);
                Leave("1D", Tree1D(fx, At(20), At(80)), off);
                Leave("Direct of 1D", Direct(fx, (Tree1D(fx, At(20), At(80)), W)), off);
                Leave("2D", Tree2D(fx, BlendTreeType.FreeformCartesian2D, At(10), At(30), At(50), At(70), At(90)), off);
                Leave("Direct, into no defaults", Direct(fx, (At(40), H), (At(80), H)), bare);
            }

            // Without write defaults a direct tree feeds back each target its leaves leave unwritten, scaled by those
            // leaves' weights; above one that grows each frame, so its trees play only while Pulse is set, between
            // states that set the layer's other targets.
            var sums = new[] { ("below one", Q, Q), ("one", H, H), ("above one", W, Q) };
            foreach (var (bound, a, b, cs) in new[]
            {
                ("Some", Faces[12], Faces[13], new[] { Faces[14], Visemes[7], Visemes[8] }),
                ("All", Visemes[0], Visemes[1], new string[3]),
                ("None", Visemes[2], Visemes[3], new[] { Visemes[4], Visemes[4], Visemes[4] }),
            })
            {
                var sm = fx.Layer("DirectNoDefaults" + bound).stateMachine;
                var off = Rest(sm, "Off", true);
                var set = Held((a, 60), (b, 60));
                foreach (var ((sum, x, y), c) in sums.Zip(cs, (w, c) => (w, c)))
                {
                    (Motion, Motion) leaves = bound == "Some" ? (Held((a, 100)), Held((c, 100)))
                        : bound == "All" ? (Held((a, 100), (b, 20)), Held((a, 30), (b, 100)))
                        : (Held((c, 100)), Held((c, 40)));
                    var id = next++;
                    var s = State(sm, id, "Set", false, set);
                    var t = State(sm, id, $"{bound}, {sum}", false, Direct(fx, (leaves.Item1, x), (leaves.Item2, y)));
                    OnCase(Go(off, s, 0), id, pulse: false);
                    Go(s, t, 0).AddCondition(AnimatorConditionMode.If, 0, PulseParameter);
                    Go(t, s, 0).AddCondition(AnimatorConditionMode.IfNot, 0, PulseParameter);
                    Go(s, off, 0).AddCondition(AnimatorConditionMode.NotEqual, id, CaseParameter);
                }
            }

            {
                var sm = fx.Layer("NormalizedBelowOne").stateMachine;
                var off = Rest(sm, "Off", true);
                BlendTree N(params (Motion m, string weight)[] children) => Normalized(fx, children);
                string x = Visemes[5], y = Visemes[6];
                var lx = Long(x, 1); var ly = Long(y, 2);
                var hx = Held((x, 100)); var hy = Held((y, 100));
                foreach (var (name, wd, tree) in new[]
                {
                    ("length", true, N((lx, Q), (ly, Q))),
                    ("length, uneven", true, N((lx, Q), (ly, H))),
                    ("length, single", true, N((lx, H))),
                    ("zero-length", true, N((hx, Q), (hy, Q))),
                    ("zero-length, one unweighted", true, N((hx, O), (hy, H))),
                    ("mixed", true, N((hx, Q), (ly, Q))),
                    ("length, no defaults", false, N((Long(Visemes[11], 1), Q), (Long(Visemes[12], 2), Q))),
                })
                    CaseState(sm, off, name, tree, next++, wd);
            }

            InstallFx(d, parameters, fx);
        }
    }
}
