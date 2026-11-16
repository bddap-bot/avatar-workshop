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
    // The frame grid, write-defaults fill and additive hold probe. The generic probe bounds the frame-grid tolerance
    // only between 0.1 and 0.37 of a frame and cannot tell whether one off-grid curve keeps a clip's others off the
    // frames by curve kind or by how many of its keys are off; and no recording shows an override state's unanimated
    // channel beside an animated one, an additive layer idling after a motion, leaving a held empty state through a
    // fade, or adding a body offset over a body turned beneath it.
    public static partial class Fixture
    {
        public static readonly float[] GridOffsets = { 0.15f, 0.2f, 0.25f, 0.3f, 0.37f };
        public static string GridOffset(float o) => "offset_" + o.ToString("0.00", CultureInfo.InvariantCulture).Replace('.', '_');
        public static readonly string[] GridShapes = { "alone", "beside_on_grid", "one_key", "two_keys", "stepped", "muscle" };
        public static string GridKey(float offset, string shape) => $"grid.{GridOffset(offset)}.{shape}";
        public static readonly string[] FillStates = { "write_defaults", "no_write_defaults" };
        public static readonly string[] FillAbove = { "alone", "additive_above" };
        public static string FillKey(string state, string above) => $"fill.{state}.{above}";
        public const string HoldIdleKey = "hold.idle.write_defaults_then_without", HoldFadeKey = "hold.fade_out";
        public static readonly string[] BodyBeneath = { "turned", "turning" };
        public static string BodyKey(string beneath) => $"body.additive_over.{beneath}";

        public const string GridOff = "GridOff", GridOn = "GridOn", GridOnOther = "GridOnOther", GridMuscle = "Chest Left-Right";
        public static string FillNode(int id) => "Fill" + id;
        public static string MoverNode(int id) => "Mover" + id;
        public static string HoldShape(int id) => "Hold" + id;
        public static string HoldNode(int id) => "Holder" + id;

        static readonly Vector3 FillRest = new Vector3(0.2f, 1.0f, 0.3f), FillScale = new Vector3(1.2f, 0.8f, 1.1f);
        static readonly Vector3 HoldRest = new Vector3(-0.2f, 1.2f, 0.25f);
        static readonly Quaternion GridBodyTurn = Quaternion.Euler(10, 60, -15), GridBodyOffset = Quaternion.Euler(20, -10, 15);

        static AnimationCurve GridCurve(int seed, float centre, float scale, float offset, Func<int, bool> off)
        {
            var random = new System.Random(seed);
            int frames = Mathf.RoundToInt(CurveLength * CurveRate), inner = frames / 3 - 1;
            var keys = Enumerable.Range(0, frames / 3 + 1).Select(i =>
            {
                int f = 3 * i;
                bool moved = i > 0 && i <= inner && off(i - 1);
                return new Keyframe((moved ? f + offset : f) / CurveRate, centre + scale * (float)(2 * random.NextDouble() - 1));
            }).ToArray();
            var curve = new AnimationCurve(keys);
            Smooth(curve);
            return curve;
        }

        static void GridClip(AnimationClip c, float offset, string shape)
        {
            c.frameRate = CurveRate;
            var serialized = new SerializedObject(c);
            serialized.FindProperty("m_UseHighQualityCurve").boolValue = false;
            serialized.ApplyModifiedPropertiesWithoutUndo();
            const string body = "Body";
            var smr = typeof(SkinnedMeshRenderer);
            Func<int, bool> all = _ => true;
            Func<int, bool> pick = shape == "one_key" ? i => i == 9 : shape == "two_keys" ? i => i == 9 || i == 19 : all;
            var offCurve = GridCurve(100, 50, 40, offset, pick);
            if (shape == "stepped")
                for (int i = 0; i < offCurve.length; i++)
                {
                    AnimationUtility.SetKeyBroken(offCurve, i, true);
                    AnimationUtility.SetKeyLeftTangentMode(offCurve, i, AnimationUtility.TangentMode.Constant);
                    AnimationUtility.SetKeyRightTangentMode(offCurve, i, AnimationUtility.TangentMode.Constant);
                }
            if (shape == "muscle")
                Set(c, "", typeof(Animator), GridMuscle, GridCurve(101, 0, 0.5f, offset, all));
            else
                Set(c, body, smr, "blendShape." + GridOff, offCurve);
            if (shape != "alone") Set(c, body, smr, "blendShape." + GridOn, GridCurve(102, 50, 40, 0, all));
            if (shape == "stepped" || shape == "muscle") Set(c, body, smr, "blendShape." + GridOnOther, GridCurve(103, 50, 40, 0, all));
        }

        static void GridHoldLayers(VRCAvatarDescriptor d)
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
            foreach (var s in new[] { GridOff, GridOn, GridOnOther }) Shaped(s);

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
            AnimatorState Into(AnimatorStateMachine sm, AnimatorState off, int id, string name, Motion m, bool writeDefaults)
            {
                var s = sm.AddState($"{id} {name}");
                s.writeDefaultValues = writeDefaults; s.motion = m;
                Go(s, off, 0).AddCondition(AnimatorConditionMode.NotEqual, id, CaseParameter);
                return s;
            }
            Action<AnimationClipSettings> Against(AnimationClip r) => s =>
            {
                s.hasAdditiveReferencePose = true; s.additiveReferencePoseClip = r; s.additiveReferencePoseTime = 0;
            };

            // An FX additive layer playing a motion adds their rest value to generic properties no state writes, so the
            // cases without an additive motion come first.
            var fx = Controller("GridHoldFX");
            var grid = fx.Layer("Grid").stateMachine;
            var gridOff = Off(grid, null);
            var fill = fx.Layer("Fill").stateMachine;
            var fillOff = Off(fill, null);
            var above = fx.Layer("Above", AnimatorLayerBlendingMode.Additive).stateMachine;
            var aboveOff = Off(above, null);
            var holds = fx.Layer("Holds", AnimatorLayerBlendingMode.Additive).stateMachine;
            var holdsOff = Off(holds, null);

            int next = 1;
            var baseCtl = Controller("GridHoldBase");
            var bodySm = baseCtl.Layer("Body").stateMachine;
            var bodyOff = Off(bodySm, rest);
            var bodyAdditive = baseCtl.Layer("Additive", AnimatorLayerBlendingMode.Additive).stateMachine;
            var bodyAdditiveOff = Off(bodyAdditive, null);
            var restReference = k.Make("body reference", c => ReferencePose(c, Quaternion.identity));
            var offsetClip = k.Make("body offset", c =>
            {
                Muscles(c, ("Spine Front-Back", 0.2f));
                HeldRotation(c, "RootQ", GridBodyOffset);
            }, settings: Against(restReference));
            foreach (var beneath in BodyBeneath)
            {
                int id = next++;
                string label = $"body {beneath}";
                var turn = k.Make(label, c =>
                {
                    Muscles(c);
                    if (beneath == "turned") HeldRotation(c, "RootQ", GridBodyTurn);
                    else
                    {
                        var half = Quaternion.Slerp(Quaternion.identity, GridBodyTurn, 0.5f);
                        foreach (var a in "xyzw")
                        {
                            int i = "xyzw".IndexOf(a);
                            Curve(c, "", typeof(Animator), $"RootQ.{a}", (0, Quaternion.identity[i]), (1, half[i]), (2, GridBodyTurn[i]));
                        }
                    }
                });
                CaseState(bodySm, bodyOff, label, turn, id, writeDefaults: false);
                CaseState(bodyAdditive, bodyAdditiveOff, label + " offset", offsetClip, id, writeDefaults: false);
            }

            foreach (var shape in GridShapes)
                foreach (var offset in GridOffsets)
                {
                    var clip = k.Make($"grid {GridOffset(offset)} {shape}", c => GridClip(c, offset, shape));
                    CaseState(grid, gridOff, clip.name, clip, next++, writeDefaults: true);
                }

            void FillCase(string state, bool withAbove)
            {
                int id = next++;
                string node = FillNode(id), label = $"fill {state} {(withAbove ? "additive_above" : "alone")}";
                var t = Node(root, node, FillRest);
                t.localScale = FillScale;
                var clip = k.Make(label, c =>
                {
                    Curve(c, node, typeof(Transform), "m_LocalPosition.x", (0, FillRest.x), (1, FillRest.x + 0.2f), (2, FillRest.x));
                    Curve(c, node, typeof(Transform), "m_LocalScale.y", (0, FillScale.y), (1, FillScale.y + 0.3f), (2, FillScale.y));
                }, loop: true);
                CaseState(fill, fillOff, label, clip, id, writeDefaults: state == "write_defaults");
                if (!withAbove) return;
                string mover = MoverNode(id);
                var moverRest = new Vector3(0.4f, 1.1f, 0.2f);
                Node(root, mover, moverRest);
                var reference = k.Make($"{label} reference", c => Const(c, mover, typeof(Transform), "m_LocalPosition.y", moverRest.y));
                var moving = k.Make($"{label} above", c => Curve(c, mover, typeof(Transform), "m_LocalPosition.y", (0, moverRest.y), (1, moverRest.y + 0.2f), (2, moverRest.y)),
                    loop: true, settings: Against(reference));
                CaseState(above, aboveOff, label + " above", moving, id, writeDefaults: false);
            }

            foreach (var state in FillStates) FillCase(state, false);

            AnimationClip HoldReference(int id) => k.Make($"hold {id} reference", c =>
            {
                Shape(c, HoldShape(id), 25);
                Const(c, HoldNode(id), typeof(Transform), "m_LocalPosition.y", HoldRest.y);
            });
            AnimationClip Holding(int id, string name, bool rising, AnimationClip reference) => k.Make($"hold {id} {name}", c =>
            {
                Curve(c, "Body", typeof(SkinnedMeshRenderer), "blendShape." + HoldShape(id), rising ? new[] { (0f, 10f), (1f, 80f), (2f, 10f) } : new[] { (0f, 60f), (2f, 20f) });
                Curve(c, HoldNode(id), typeof(Transform), "m_LocalPosition.y", rising
                    ? new[] { (0f, HoldRest.y + 0.05f), (1f, HoldRest.y + 0.2f), (2f, HoldRest.y + 0.05f) } : new[] { (0f, HoldRest.y + 0.3f), (2f, HoldRest.y + 0.1f) });
            }, loop: rising, settings: Against(reference));
            void HoldProps(int id)
            {
                Shaped(HoldShape(id));
                Node(root, HoldNode(id), HoldRest);
            }
            {
                int id = next++;
                HoldProps(id);
                string label = "hold idle";
                var motion = CaseState(holds, holdsOff, label + " motion", Holding(id, "motion", true, HoldReference(id)), id, writeDefaults: false);
                var idleWd = Into(holds, holdsOff, id, label + " write_defaults", null, writeDefaults: true);
                var idleKept = Into(holds, holdsOff, id, label + " without", null, writeDefaults: false);
                Go(motion, idleWd, 0).AddCondition(AnimatorConditionMode.If, 0, PulseParameter);
                Go(idleWd, idleKept, 0).AddCondition(AnimatorConditionMode.IfNot, 0, PulseParameter);
            }
            {
                int id = next++;
                HoldProps(id);
                string label = "hold fade_out";
                var reference = HoldReference(id);
                var motion = CaseState(holds, holdsOff, label + " motion", Holding(id, "motion", true, reference), id, writeDefaults: false);
                var held = Into(holds, holdsOff, id, label + " held", null, writeDefaults: false);
                var after = Into(holds, holdsOff, id, label + " after", Holding(id, "after", false, reference), writeDefaults: false);
                Go(motion, held, 0).AddCondition(AnimatorConditionMode.If, 0, PulseParameter);
                Go(held, after, CrossfadeSeconds).AddCondition(AnimatorConditionMode.IfNot, 0, PulseParameter);
            }

            // A state without write defaults beneath an additive layer playing a motion makes some unwritten generic
            // properties grow each frame, so that case comes last.
            foreach (var state in FillStates) FillCase(state, true);

            EditorUtility.SetDirty(mesh);

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
