using System.Collections.Generic;
using System.Linq;
using UnityEditor;
using UnityEditor.Animations;
using UnityEngine;
using VRC.SDK3.Avatars.Components;
using VRC.SDK3.Avatars.ScriptableObjects;

namespace AvatarWorkshop.Reference
{
    // The generic-rig hold probe. A rest state without write defaults holds every property its controller binds, so a
    // layer resting at a partial weight would hold every other case's properties too: every layer but the first rests at
    // weight 0 and is raised only by its own case. The playable-weight probe is its playable-weight cases of the
    // Additive, Gesture and Action controllers alone, beside an FX controller that never plays a motion.
    public static partial class Fixture
    {
        public static readonly string[] HoldProperties = { "position", "rotation", "scale", "blendshape", "parameter" };
        public static readonly float[] HoldWeights = { 0.25f, 0.5f, 0.75f };
        public static string HoldWeight(float w) => "w" + Mathf.RoundToInt(w * 100);
        public static readonly string[] HoldHistories = { "before", "after" };
        public static readonly string[] HoldContexts = { "alone", "under_additive" };
        public static readonly VRCAvatarDescriptor.AnimLayerType[] HoldPlayables =
        {
            VRCAvatarDescriptor.AnimLayerType.Additive, VRCAvatarDescriptor.AnimLayerType.Gesture,
            VRCAvatarDescriptor.AnimLayerType.Action, VRCAvatarDescriptor.AnimLayerType.FX,
        };
        public static readonly VRCAvatarDescriptor.AnimLayerType[] WeightPlayables = HoldPlayables.Where(t => t != VRCAvatarDescriptor.AnimLayerType.FX).ToArray();
        public static readonly string[] HoldAbove = { "alone", "under_partial" };
        public static string HoldKey(string property, float weight, string history, string context) =>
            $"hold.{property}.{HoldWeight(weight)}.{history}.{context}";
        public static string HoldPlayableKey(VRCAvatarDescriptor.AnimLayerType playable, string property, float weight, string above) =>
            $"playable.{GenericPlayable(playable)}.{property}.{HoldWeight(weight)}.{above}";

        public const float HoldAboveWeight = 0.5f;
        public static string HeldShape(int id) => "Held" + id;
        public static string HeldNode(int id) => "HeldNode" + id;
        public static string HeldParameter(int id) => "HeldParam" + id;
        static readonly Vector3 HoldNodeRest = new Vector3(-0.1f, 1.3f, 0.2f);
        public static readonly string[] HoldEuler = { "localEulerAnglesRaw.x", "localEulerAnglesRaw.y", "localEulerAnglesRaw.z" };

        static void HoldSet(AnimationClip c, int id, string property, params (float t, float v)[] keys)
        {
            switch (property)
            {
                case "blendshape": Curve(c, "Body", typeof(SkinnedMeshRenderer), "blendShape." + HeldShape(id), keys); break;
                case "position": Curve(c, HeldNode(id), typeof(Transform), "m_LocalPosition.y", keys.Select(k => (k.t, HoldNodeRest.y + k.v)).ToArray()); break;
                case "rotation":
                    Curve(c, HeldNode(id), typeof(Transform), HoldEuler[0], keys.Select(k => (k.t, 0f)).ToArray());
                    Curve(c, HeldNode(id), typeof(Transform), HoldEuler[1], keys);
                    Curve(c, HeldNode(id), typeof(Transform), HoldEuler[2], keys.Select(k => (k.t, 0f)).ToArray());
                    break;
                case "scale": Curve(c, HeldNode(id), typeof(Transform), "m_LocalScale.y", keys); break;
                default: Curve(c, "", typeof(Animator), HeldParameter(id), keys); break;
            }
        }

        // Each property's keys for one role: (blendshape, position offset, rotation degrees, scale, parameter).
        static void HoldClip(AnimationClip c, int id, params (float t, float shape, float pos, float rot, float scale, float param)[] keys)
        {
            HoldSet(c, id, "blendshape", keys.Select(k => (k.t, k.shape)).ToArray());
            HoldSet(c, id, "position", keys.Select(k => (k.t, k.pos)).ToArray());
            HoldSet(c, id, "rotation", keys.Select(k => (k.t, k.rot)).ToArray());
            HoldSet(c, id, "scale", keys.Select(k => (k.t, k.scale)).ToArray());
            HoldSet(c, id, "parameter", keys.Select(k => (k.t, k.param)).ToArray());
        }

        static VRCAnimatorLayerControl.BlendableLayer HoldLayerControl(VRCAvatarDescriptor.AnimLayerType t) =>
            (VRCAnimatorLayerControl.BlendableLayer)System.Enum.Parse(typeof(VRCAnimatorLayerControl.BlendableLayer), RigBlendable(t).Value.ToString());

        static void HoldLayers(VRCAvatarDescriptor d, bool fxPlays)
        {
            var root = d.transform;
            var smr = root.Find("Body").GetComponent<SkinnedMeshRenderer>();
            var mesh = smr.sharedMesh;
            var delta = new Vector3[mesh.vertexCount];
            mesh.GetBlendShapeFrameVertices(mesh.GetBlendShapeIndex(SampledShape), 0, delta, null, null);

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

            int next = 1;
            int Fresh(Ctl ctl)
            {
                int id = next++;
                mesh.AddBlendShapeFrame(HeldShape(id), 100, delta, null, null);
                smr.SetBlendShapeWeight(mesh.GetBlendShapeIndex(HeldShape(id)), GenericShapeDefault);
                Node(root, HeldNode(id), HoldNodeRest);
                ctl.C.AddParameter(new AnimatorControllerParameter { name = HeldParameter(id), type = AnimatorControllerParameterType.Float, defaultFloat = RigParameterDefault });
                return id;
            }
            AnimationClip Below(int id, string label) => k.Make(label, c => HoldClip(c, id, (0, 10, 0.05f, 10, 1.1f, 0.1f), (1, 80, 0.2f, 60, 1.6f, 0.9f), (2, 10, 0.05f, 10, 1.1f, 0.1f)), loop: true);

            var ctls = HoldPlayables.ToDictionary(t => t, t => new Ctl("Hold" + t, (CaseParameter, AnimatorControllerParameterType.Int),
                (BlendX, AnimatorControllerParameterType.Float), (BlendY, AnimatorControllerParameterType.Float), (BlendU, AnimatorControllerParameterType.Float)));
            var fulls = new Dictionary<VRCAvatarDescriptor.AnimLayerType, (AnimatorStateMachine sm, AnimatorState off)>();
            AnimatorState Rest(AnimatorStateMachine sm)
            {
                var s = sm.AddState("Off"); s.writeDefaultValues = false;
                sm.defaultState = s;
                return s;
            }
            // A layer of the case's own, resting at weight 0, which the case's state `entered` raises to `weight`.
            (AnimatorStateMachine sm, AnimatorState off, int index) Own(VRCAvatarDescriptor.AnimLayerType type, string name, AnimatorLayerBlendingMode mode = AnimatorLayerBlendingMode.Override)
            {
                int index = ctls[type].C.layers.Length;
                var sm = ctls[type].Layer(name, mode, weight: 0).stateMachine;
                var off = Rest(sm);
                LayerControl(off, HoldLayerControl(type), index, 0, 0);
                return (sm, off, index);
            }
            foreach (var type in HoldPlayables)
            {
                var full = ctls[type].Layer("Full").stateMachine;
                fulls[type] = (full, Rest(full));
                PlayableControl(fulls[type].off, RigBlendable(type).Value, type == VRCAvatarDescriptor.AnimLayerType.Action ? 0 : 1, 0);
            }

            var fx = VRCAvatarDescriptor.AnimLayerType.FX;
            foreach (var context in fxPlays ? HoldContexts : new string[0])
                foreach (var history in HoldHistories)
                    foreach (var weight in HoldWeights)
                    {
                        int id = Fresh(ctls[fx]);
                        string label = $"hold {HoldWeight(weight)} {history} {context}";
                        CaseState(fulls[fx].sm, fulls[fx].off, label, Below(id, label), id, writeDefaults: false);

                        var hold = Own(fx, $"Hold {HoldWeight(weight)} {history} {context}");
                        var clip = k.Make(label + " played", c => HoldClip(c, id, (0, 40, 0.3f, 90, 2, 0.5f), (0.5f, 95, 0.45f, 120, 2.5f, 0.7f)));
                        AnimatorState entered;
                        if (history == "before")
                        {
                            // A layer binds only what its own motions animate; this state, which nothing enters, binds them.
                            var unplayed = hold.sm.AddState($"{label} unplayed");
                            unplayed.writeDefaultValues = false; unplayed.motion = clip;
                            entered = CaseState(hold.sm, hold.off, label + " held", null, id, writeDefaults: false);
                        }
                        else
                        {
                            entered = CaseState(hold.sm, hold.off, label + " played", clip, id, writeDefaults: false);
                            Then(entered, hold.sm, hold.off, label + " held", null, id);
                        }
                        LayerControl(entered, HoldLayerControl(fx), hold.index, weight, 0);

                        if (context == "under_additive")
                        {
                            var add = Own(fx, $"Additive {HoldWeight(weight)} {history}", AnimatorLayerBlendingMode.Additive);
                            var played = CaseState(add.sm, add.off, label + " additive", k.Make(label + " additive", c =>
                                HoldClip(c, id, (0, 0, 0, 0, 1, 0), (0.75f, 30, 0.1f, 20, 1.3f, 0.3f))), id, writeDefaults: false);
                            LayerControl(played, HoldLayerControl(fx), add.index, 1, 0);
                            Then(played, add.sm, add.off, label + " additive held", null, id);
                        }
                    }

            foreach (var type in fxPlays ? HoldPlayables : WeightPlayables)
                foreach (var above in HoldAbove)
                    foreach (var weight in HoldWeights)
                    {
                        var ctl = ctls[type];
                        int id = Fresh(ctl);
                        string label = $"playable {GenericPlayable(type)} {HoldWeight(weight)} {above}";
                        var s = CaseState(fulls[type].sm, fulls[type].off, label, Below(id, label), id, writeDefaults: false);
                        PlayableControl(s, RigBlendable(type).Value, weight, 0);
                        if (above == "under_partial")
                        {
                            var layer = Own(type, $"Above {HoldWeight(weight)}");
                            var over = CaseState(layer.sm, layer.off, label + " above", k.Make(label + " above", c =>
                                HoldClip(c, id, (0, 60, 0.3f, 45, 0.7f, 0.6f), (1 / 60f, 60, 0.3f, 45, 0.7f, 0.6f))), id, writeDefaults: false);
                            LayerControl(over, HoldLayerControl(type), layer.index, HoldAboveWeight, 0);
                        }
                    }
            EditorUtility.SetDirty(mesh);

            d.customExpressions = true;
            d.expressionParameters = parameters;
            d.customizeAnimationLayers = true;
            var baseCtl = new Ctl("HoldBase");
            Rest(baseCtl.Layer("Full").stateMachine);
            d.baseAnimationLayers = new[] { Custom(VRCAvatarDescriptor.AnimLayerType.Base, baseCtl.C) }.Concat(HoldPlayables.Select(t => Custom(t, ctls[t].C))).ToArray();
            d.specialAnimationLayers = new[]
            {
                new VRCAvatarDescriptor.CustomAnimLayer { type = VRCAvatarDescriptor.AnimLayerType.Sitting, isDefault = true, isEnabled = true },
                new VRCAvatarDescriptor.CustomAnimLayer { type = VRCAvatarDescriptor.AnimLayerType.TPose, isDefault = true, isEnabled = true },
                new VRCAvatarDescriptor.CustomAnimLayer { type = VRCAvatarDescriptor.AnimLayerType.IKPose, isDefault = true, isEnabled = true },
            };
        }

        static AnimatorState Then(AnimatorState from, AnimatorStateMachine sm, AnimatorState off, string name, Motion m, int id)
        {
            var s = sm.AddState($"{id} {name}");
            s.writeDefaultValues = false; s.motion = m;
            var t = Go(from, s, 0);
            t.hasExitTime = true; t.exitTime = 1;
            Go(s, off, 0).AddCondition(AnimatorConditionMode.NotEqual, id, CaseParameter);
            return s;
        }
    }
}
