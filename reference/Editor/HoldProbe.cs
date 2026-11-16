using System.Collections.Generic;
using System.Linq;
using UnityEditor;
using UnityEditor.Animations;
using UnityEngine;
using VRC.SDK3.Avatars.Components;
using VRC.SDK3.Avatars.ScriptableObjects;

namespace AvatarWorkshop.Reference
{
    // The generic-rig hold probe. Every generic-rig playable probe controller holds two override layers at 0.5 beside an
    // additive layer, so that recording cannot tell which layer holds what, and it records playable weight at 0.5 only.
    // Every layer here but the first rests at weight 0 and is raised only by its own case, so each case's layers are
    // the only ones that compose.
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
            t == VRCAvatarDescriptor.AnimLayerType.Additive ? VRCAnimatorLayerControl.BlendableLayer.Additive
            : t == VRCAvatarDescriptor.AnimLayerType.Gesture ? VRCAnimatorLayerControl.BlendableLayer.Gesture
            : t == VRCAvatarDescriptor.AnimLayerType.Action ? VRCAnimatorLayerControl.BlendableLayer.Action
            : VRCAnimatorLayerControl.BlendableLayer.FX;

        static void HoldLayers(VRCAvatarDescriptor d)
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
            var aboves = new Dictionary<VRCAvatarDescriptor.AnimLayerType, (AnimatorStateMachine sm, AnimatorState off)>();
            AnimatorState Rest(AnimatorStateMachine sm)
            {
                var s = sm.AddState("Off"); s.writeDefaultValues = false;
                sm.defaultState = s;
                return s;
            }
            foreach (var type in HoldPlayables)
            {
                var ctl = ctls[type];
                var full = ctl.Layer("Full").stateMachine;
                fulls[type] = (full, Rest(full));
                PlayableControl(fulls[type].off, RigBlendable(type).Value, type == VRCAvatarDescriptor.AnimLayerType.Action ? 0 : 1, 0);
                var above = ctl.Layer("Above", weight: 0).stateMachine;
                aboves[type] = (above, Rest(above));
                LayerControl(aboves[type].off, HoldLayerControl(type), 1, 0, 0);
            }

            // The hold cases, in the FX controller: each its own layer, at weight 0 until its case raises it.
            var fx = VRCAvatarDescriptor.AnimLayerType.FX;
            var holds = new List<(int id, float weight, string history, string context)>();
            foreach (var context in HoldContexts)
                foreach (var history in HoldHistories)
                    foreach (var weight in HoldWeights)
                        holds.Add((Fresh(ctls[fx]), weight, history, context));
            var holdLayers = holds.Select(h => ctls[fx].Layer($"Hold {HoldWeight(h.weight)} {h.history} {h.context}", weight: 0).stateMachine).ToList();
            int additiveIndex = ctls[fx].C.layers.Length;
            var add = ctls[fx].Layer("Additive", AnimatorLayerBlendingMode.Additive, weight: 0).stateMachine;
            var addOff = Rest(add);
            LayerControl(addOff, HoldLayerControl(fx), additiveIndex, 0, 0);
            for (int i = 0; i < holds.Count; i++)
            {
                var (id, weight, history, context) = holds[i];
                int index = 2 + i;
                string label = $"hold {HoldWeight(weight)} {history} {context}";
                CaseState(fulls[fx].sm, fulls[fx].off, label, Below(id, label), id, writeDefaults: false);

                var sm = holdLayers[i];
                var off = Rest(sm);
                LayerControl(off, HoldLayerControl(fx), index, 0, 0);
                AnimatorState entered;
                if (history == "before") entered = CaseState(sm, off, label + " held", null, id, writeDefaults: false);
                else
                {
                    entered = CaseState(sm, off, label + " played", k.Make(label + " played", c =>
                        HoldClip(c, id, (0, 40, 0.3f, 90, 2, 0.5f), (0.5f, 95, 0.45f, 120, 2.5f, 0.7f))), id, writeDefaults: false);
                    Then(entered, sm, off, label + " held", null, id);
                }
                LayerControl(entered, HoldLayerControl(fx), index, weight, 0);

                if (context == "under_additive")
                {
                    var played = CaseState(add, addOff, label + " additive", k.Make(label + " additive", c =>
                        HoldClip(c, id, (0, 0, 0, 0, 1, 0), (0.75f, 30, 0.1f, 20, 1.3f, 0.3f))), id, writeDefaults: false);
                    LayerControl(played, HoldLayerControl(fx), additiveIndex, 1, 0);
                    Then(played, add, addOff, label + " additive held", null, id);
                }
            }

            // The playable-weight cases: each played in a controller's first layer with its playable lowered as it enters.
            foreach (var type in HoldPlayables)
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
                            var over = CaseState(aboves[type].sm, aboves[type].off, label + " above", k.Make(label + " above", c =>
                                HoldClip(c, id, (0, 60, 0.3f, 45, 0.7f, 0.6f), (1 / 60f, 60, 0.3f, 45, 0.7f, 0.6f))), id, writeDefaults: false);
                            LayerControl(over, HoldLayerControl(type), 1, HoldAboveWeight, 0);
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

        // A state the case moves on to once `from` has played through, held until the case ends.
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
