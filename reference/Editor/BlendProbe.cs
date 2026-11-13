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
        // Controller parameters the expression parameters do not list keep their defaults, so the asset fixes the
        // weight sum of each direct tree that takes its weights from these.
        static readonly (string name, float value)[] Weights = { ("W0", 0), ("W25", 0.25f), ("W50", 0.5f), ("W100", 1) };

        static void BlendLayers(VRCAvatarDescriptor d)
        {
            AssetDatabase.CreateFolder(Dir, "Clips");
            var k = new Clips(Dir + "/Clips");
            AnimationClip Held(string name, string shape) => k.Make(name, c => AnimationUtility.SetEditorCurve(c,
                EditorCurveBinding.FloatCurve("Body", typeof(SkinnedMeshRenderer), "blendShape." + shape), new AnimationCurve(new Keyframe(0, 100))));
            AnimationClip Long(string name, string shape, float seconds) =>
                k.Make(name, c => Curve(c, "Body", typeof(SkinnedMeshRenderer), "blendShape." + shape, (0, 0), (seconds, 100)), loop: true);
            var z = Enumerable.Range(0, 10).Select(i => Held("Zero" + i, Faces[i])).ToArray();
            var one = Long("One", Faces[10], 1);
            var two = Long("Two", Faces[11], 2);
            var brief = Long("Brief", Faces[12], 0.4f);
            // A clip without curves reports a length of one second.
            var empty = k.Make("Empty", c => { });

            var parameters = ScriptableObject.CreateInstance<VRCExpressionParameters>();
            const VRCExpressionParameters.ValueType I = VRCExpressionParameters.ValueType.Int, F = VRCExpressionParameters.ValueType.Float;
            parameters.parameters = new[]
            {
                P(CaseParameter, I, 0, saved: false),
                P(BlendX, F, 0, saved: false, synced: false), P(BlendY, F, 0, saved: false, synced: false), P(BlendU, F, 0, saved: false, synced: false),
            };
            AssetDatabase.CreateAsset(parameters, $"{Dir}/Parameters.asset");

            var fx = new Ctl("BlendFX", (CaseParameter, AnimatorControllerParameterType.Int),
                (BlendX, AnimatorControllerParameterType.Float), (BlendY, AnimatorControllerParameterType.Float), (BlendU, AnimatorControllerParameterType.Float));
            foreach (var (name, value) in Weights)
                fx.C.AddParameter(new AnimatorControllerParameter { name = name, type = AnimatorControllerParameterType.Float, defaultFloat = value });
            const string O = "W0", Q = "W25", H = "W50", W = "W100";

            (AnimatorStateMachine sm, AnimatorState off) Layer(string name, bool writeDefaults)
            {
                var sm = fx.Layer(name).stateMachine;
                var off = sm.AddState("Off"); off.writeDefaultValues = writeDefaults;
                sm.defaultState = off;
                return (sm, off);
            }
            var on = Layer("Tree", true);
            var bare = Layer("TreeNoDefaults", false);
            int next = 1;
            void Case(string name, Motion m, (AnimatorStateMachine sm, AnimatorState off)? layer = null)
            {
                var (sm, off) = layer ?? on;
                CaseState(sm, off, name, m, next++, off.writeDefaultValues);
            }
            BlendTree D(params (Motion m, string weight)[] children) => Direct(fx, children);
            BlendTree N(params (Motion m, string weight)[] children)
            {
                var t = D(children);
                var o = new SerializedObject(t);
                o.FindProperty("m_NormalizedBlendValues").boolValue = true;
                o.ApplyModifiedPropertiesWithoutUndo();
                return t;
            }
            var many = z.Select(c => ((Motion)c, W)).Append((one, W)).Append((two, W)).ToArray();

            Case("below one", D((one, Q), (two, Q)));
            Case("below one, single", D((one, H)));
            Case("to one", D((one, H), (two, H)));
            Case("to one, single", D((one, W)));
            Case("above one", D((one, W), (two, W)));
            Case("above one, three", D((one, W), (two, W), (brief, W)));
            Case("above one, uneven", D((one, W), (two, Q)));
            Case("ramped", D((one, BlendX), (two, BlendY)));

            Case("mixed below one", D((z[0], Q), (one, Q)));
            Case("mixed to one", D((z[0], H), (one, H)));
            Case("mixed above one", D((z[0], W), (one, W)));
            Case("mixed, zero-length heavier", D((z[0], W), (one, Q)));
            Case("mixed, zero-length lighter", D((z[0], Q), (one, W)));
            Case("mixed, zero-length unweighted", D((z[0], O), (one, W)));
            Case("mixed, length unweighted", D((z[0], W), (one, O)));
            Case("mixed, three zero-length", D((z[0], W), (z[1], W), (z[2], W), (one, W)));
            Case("mixed, many", D(many));
            Case("mixed ramped", D((z[0], BlendX), (one, BlendY)));
            Case("zero-length only", D((z[0], W), (z[1], W)));
            Case("zero-length only, below one", D((z[0], Q), (z[1], Q)));

            Case("nested", D((D((one, W), (two, W)), W), (brief, W)));
            Case("nested, inner below one", D((D((one, Q), (two, Q)), W)));
            Case("nested, outer below one", D((D((one, W), (two, W)), Q)));
            Case("nested, inner mixed", D((D((z[0], W), (one, W)), W), (D((z[1], W), (two, W)), W)));
            Case("nested, inner zero-length only", D((D((z[0], W), (z[1], W)), W), (one, W)));
            Case("nested, zero-length beside inner", D((D((one, H), (two, H)), H), (z[0], W)));
            Case("nested twice", D((D((D((z[0], W), (one, W)), W), (two, W)), W), (z[1], W)));

            Case("empty only", D((empty, W), (empty, W)));
            Case("empty and length", D((empty, W), (brief, W)));
            Case("empty and length, below one", D((empty, Q), (brief, Q)));
            Case("empty and zero-length", D((empty, W), (z[0], W)));
            Case("empty, zero-length and length", D((empty, W), (z[0], W), (brief, W)));
            Case("normalized empty and length", N((empty, W), (brief, W)));
            Case("empty clip", empty);
            Case("zero-length clip", z[0]);

            Case("normalized below one", N((one, Q), (two, Q)));
            Case("normalized to one", N((one, H), (two, H)));
            Case("normalized above one", N((one, W), (two, W)));
            Case("normalized above one, uneven", N((one, W), (two, Q)));
            Case("normalized ramped", N((one, BlendX), (two, BlendY)));
            Case("normalized mixed below one", N((z[0], Q), (one, Q)));
            Case("normalized mixed to one", N((z[0], H), (one, H)));
            Case("normalized mixed above one", N((z[0], W), (one, W)));
            Case("normalized mixed, zero-length heavier", N((z[0], W), (one, Q)));
            Case("normalized mixed, zero-length lighter", N((z[0], Q), (one, W)));
            Case("normalized mixed, zero-length unweighted", N((z[0], O), (one, W)));
            Case("normalized mixed, length unweighted", N((z[0], W), (one, O)));
            Case("normalized mixed, many", N(many));
            Case("normalized mixed ramped", N((z[0], BlendX), (one, BlendY)));
            Case("normalized zero-length only", N((z[0], W), (z[1], W)));
            Case("normalized nested", N((N((z[0], W), (one, W)), W), (N((z[1], W), (two, W)), W)));
            Case("normalized nested in plain", D((N((z[0], W), (one, W)), W), (two, W)));
            Case("plain nested in normalized", N((D((z[0], W), (one, W)), W), (two, W)));

            Case("1D zero-length only", Tree1D(fx, z[0], z[1]));
            Case("1D mixed", Tree1D(fx, z[0], one));
            Case("1D of 1D zero-length only and length", Tree1D(fx, Tree1D(fx, z[0], z[1]), one));
            Case("1D of 1D mixed and zero-length", Tree1D(fx, Tree1D(fx, z[0], one), z[1]));
            Case("direct of 1D zero-length only", D((Tree1D(fx, z[0], z[1]), W), (Tree1D(fx, z[2], z[3]), W)));
            Case("direct of 1D zero-length only and length", D((Tree1D(fx, z[0], z[1]), W), (one, W)));
            Case("direct of 1D zero-length only and length, below one", D((Tree1D(fx, z[0], z[1]), Q), (one, Q)));
            Case("direct of three 1D zero-length only and length", D((Tree1D(fx, z[0], z[1]), W), (Tree1D(fx, z[2], z[3]), W), (Tree1D(fx, z[4], z[5]), W), (one, W)));
            Case("direct of many 1D zero-length only and length", D(Enumerable.Range(0, 8).Select(i => ((Motion)Tree1D(fx, z[i], z[i + 1]), W)).Append((one, W)).ToArray()));
            Case("direct of 1D mixed", D((Tree1D(fx, z[0], one), W), (Tree1D(fx, z[1], two), W)));
            Case("direct of 1D mixed and 1D zero-length only", D((Tree1D(fx, z[0], one), W), (Tree1D(fx, z[1], z[2]), W), (Tree1D(fx, z[3], z[4]), W)));
            Case("direct of 2D zero-length only and length", D((Tree2D(fx, BlendTreeType.FreeformCartesian2D, z[0], z[1], z[2], z[3], z[4]), W), (one, W)));
            Case("normalized of 1D zero-length only and length", N((Tree1D(fx, z[0], z[1]), W), (one, W)));
            Case("1D empty and length", Tree1D(fx, empty, two));
            Case("direct of 1D empty and length, 1D zero-length only and length", D((Tree1D(fx, empty, two), W), (Tree1D(fx, z[0], z[1]), W), (brief, W)));
            foreach (var type in new[] { BlendTreeType.SimpleDirectional2D, BlendTreeType.FreeformDirectional2D, BlendTreeType.FreeformCartesian2D })
            {
                Case(type + " zero-length only", Tree2D(fx, type, z[0], z[1], z[2], z[3], z[4]));
                Case(type + " mixed", Tree2D(fx, type, z[0], one, z[1], two, z[2]));
            }

            // Without write defaults a direct tree feeds each value back scaled by its weight sum: at a sum of two
            // the values doubled every frame to infinity within a case. These stay at or below one.
            Case("mixed below one", D((z[0], Q), (one, Q)), bare);
            Case("mixed to one", D((z[0], H), (one, H)), bare);
            Case("zero-length only", D((z[0], H), (z[1], H)), bare);
            Case("normalized mixed, many", N(many), bare);

            // The shape that runs fast on avatars in the wild: a layer whose default state, playing from the first
            // frame, is a write-defaults direct tree of toggles at weight one with one clip that has length. A build
            // tool that merges toggles into one tree normalizes its blend values.
            int shape = 0;
            foreach (var normalized in new[] { false, true })
            {
                var always = fx.Layer(normalized ? "AlwaysNormalized" : "Always").stateMachine;
                var held = always.AddState("Toggles"); held.writeDefaultValues = true;
                var children = Enumerable.Range(0, 6).Select(i => ((Motion)Held("Toggle" + shape, Visemes[shape++]), W)).ToList();
                children.Add((Long("Pulse" + shape, Visemes[shape++], 0.4f), W));
                held.motion = normalized ? N(children.ToArray()) : D(children.ToArray());
                always.defaultState = held;
            }

            d.customExpressions = true;
            d.expressionParameters = parameters;
            d.customizeAnimationLayers = true;
            d.baseAnimationLayers = new[] { VRCAvatarDescriptor.AnimLayerType.Base, VRCAvatarDescriptor.AnimLayerType.Additive, VRCAvatarDescriptor.AnimLayerType.Gesture, VRCAvatarDescriptor.AnimLayerType.Action }
                .Select(type => new VRCAvatarDescriptor.CustomAnimLayer { type = type, isDefault = true, isEnabled = true })
                .Append(Custom(VRCAvatarDescriptor.AnimLayerType.FX, fx.C)).ToArray();
            d.specialAnimationLayers = new[] { VRCAvatarDescriptor.AnimLayerType.Sitting, VRCAvatarDescriptor.AnimLayerType.TPose, VRCAvatarDescriptor.AnimLayerType.IKPose }
                .Select(type => new VRCAvatarDescriptor.CustomAnimLayer { type = type, isDefault = true, isEnabled = true }).ToArray();
        }
    }
}
