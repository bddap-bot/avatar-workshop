using System.Collections.Generic;
using System.Linq;
using UnityEditor;
using UnityEditor.Animations;
using UnityEngine;
using VRC.SDK3.Avatars.Components;
using VRC.SDK3.Avatars.ScriptableObjects;

namespace AvatarWorkshop.Reference
{
    // The generic-rig playable probe. The rule that the Base and Additive playables write no generic property was
    // fitted on a humanoid avatar, whose playables also carry a human pose; nothing records it on an avatar without one.
    public static partial class Fixture
    {
        public static readonly VRCAvatarDescriptor.AnimLayerType[] RigPlayables =
        {
            VRCAvatarDescriptor.AnimLayerType.Base, VRCAvatarDescriptor.AnimLayerType.Additive, VRCAvatarDescriptor.AnimLayerType.Gesture,
            VRCAvatarDescriptor.AnimLayerType.Action, VRCAvatarDescriptor.AnimLayerType.FX,
        };
        public static readonly string[] RigProperties = { "blendshape", "position", "parameter" };
        public static readonly string[] RigContexts = { "alone", "under_override", "under_additive" };
        // A layer's own weight, and its playable's: VRChat's playable-layer control reaches every playable but Base.
        public static string[] RigWeights(VRCAvatarDescriptor.AnimLayerType playable) =>
            RigBlendable(playable).HasValue ? new[] { "full", "partial_layer", "partial_playable" } : new[] { "full", "partial_layer" };
        public static VRCPlayableLayerControl.BlendableLayer? RigBlendable(VRCAvatarDescriptor.AnimLayerType playable) =>
            playable == VRCAvatarDescriptor.AnimLayerType.Additive ? VRCPlayableLayerControl.BlendableLayer.Additive
            : playable == VRCAvatarDescriptor.AnimLayerType.Gesture ? VRCPlayableLayerControl.BlendableLayer.Gesture
            : playable == VRCAvatarDescriptor.AnimLayerType.Action ? VRCPlayableLayerControl.BlendableLayer.Action
            : playable == VRCAvatarDescriptor.AnimLayerType.FX ? VRCPlayableLayerControl.BlendableLayer.FX
            : (VRCPlayableLayerControl.BlendableLayer?)null;
        public static string RigKey(VRCAvatarDescriptor.AnimLayerType playable, string property, string context, string weight) =>
            $"rig.{GenericPlayable(playable)}.{property}.{context}.{weight}";

        public const float RigPartialWeight = 0.5f, RigParameterDefault = 0.25f;
        public static string RigShape(int id) => "Rig" + id;
        public static string RigNode(int id) => "RigNode" + id;
        public static string RigParameter(int id) => "RigParam" + id;
        static readonly Vector3 RigNodeRest = new Vector3(0.1f, 1.3f, 0.2f);

        static Avatar GenericAvatar(GameObject root)
        {
            var avatar = AvatarBuilder.BuildGenericAvatar(root, "");
            if (!avatar.isValid || avatar.isHuman) throw new System.Exception("generic-rig avatar is invalid");
            avatar.name = "AWGenericRigAvatar";
            AssetDatabase.CreateAsset(avatar, $"{Dir}/Avatar.asset");
            return avatar;
        }

        static void RigSet(AnimationClip c, int id, string property, params (float t, float v)[] keys)
        {
            switch (property)
            {
                case "blendshape": Curve(c, "Body", typeof(SkinnedMeshRenderer), "blendShape." + RigShape(id), keys); break;
                case "position": Curve(c, RigNode(id), typeof(Transform), "m_LocalPosition.y", keys.Select(k => (k.t, RigNodeRest.y + k.v)).ToArray()); break;
                default: Curve(c, "", typeof(Animator), RigParameter(id), keys); break;
            }
        }

        static void RigLayers(VRCAvatarDescriptor d)
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

            var controllers = new List<VRCAvatarDescriptor.CustomAnimLayer>();
            int next = 1;
            foreach (var type in RigPlayables)
            {
                var ctl = new Ctl("Rig" + type, (CaseParameter, AnimatorControllerParameterType.Int), (BlendX, AnimatorControllerParameterType.Float),
                    (BlendY, AnimatorControllerParameterType.Float), (BlendU, AnimatorControllerParameterType.Float));
                var full = ctl.Layer("Full").stateMachine;
                var partial = ctl.Layer("Partial", weight: RigPartialWeight).stateMachine;
                var over = ctl.Layer("Override", weight: RigPartialWeight).stateMachine;
                var add = ctl.Layer("Additive", AnimatorLayerBlendingMode.Additive).stateMachine;
                AnimatorState Off(AnimatorStateMachine sm)
                {
                    var s = sm.AddState("Off"); s.writeDefaultValues = false;
                    sm.defaultState = s;
                    return s;
                }
                var offs = new Dictionary<AnimatorStateMachine, AnimatorState> { [full] = Off(full), [partial] = Off(partial), [over] = Off(over), [add] = Off(add) };
                var blendable = RigBlendable(type);
                bool action = type == VRCAvatarDescriptor.AnimLayerType.Action;
                // Each case leaves through a reset to Case 0, so every layer's rest state is entered between cases.
                if (blendable is VRCPlayableLayerControl.BlendableLayer b)
                {
                    PlayableControl(offs[full], b, action ? 0 : 1, 0);
                    if (action) PlayableControl(offs[partial], b, 0, 0);
                }

                foreach (var weight in RigWeights(type))
                    foreach (var context in RigContexts)
                    {
                        int id = next++;
                        mesh.AddBlendShapeFrame(RigShape(id), 100, delta, null, null);
                        smr.SetBlendShapeWeight(mesh.GetBlendShapeIndex(RigShape(id)), GenericShapeDefault);
                        Node(root, RigNode(id), RigNodeRest);
                        ctl.C.AddParameter(new AnimatorControllerParameter { name = RigParameter(id), type = AnimatorControllerParameterType.Float, defaultFloat = RigParameterDefault });
                        string label = $"{GenericPlayable(type)} {context} {weight}";

                        var played = k.Make(label, c =>
                        {
                            RigSet(c, id, "blendshape", (0, 10), (1, 80), (2, 10));
                            RigSet(c, id, "position", (0, 0.05f), (1, 0.2f), (2, 0.05f));
                            RigSet(c, id, "parameter", (0, 0.1f), (1, 0.9f), (2, 0.1f));
                        }, loop: true);
                        var layer = weight == "partial_layer" ? partial : full;
                        var s = CaseState(layer, offs[layer], label, played, id, writeDefaults: false);
                        if (weight == "partial_playable") PlayableControl(s, blendable.Value, RigPartialWeight, 0);
                        else if (action) PlayableControl(s, blendable.Value, 1, 0);

                        if (context == "under_override")
                            CaseState(over, offs[over], label + " override", k.Make(label + " override", c =>
                            {
                                RigSet(c, id, "blendshape", (0, 60), (1 / 60f, 60));
                                RigSet(c, id, "position", (0, 0.3f), (1 / 60f, 0.3f));
                                RigSet(c, id, "parameter", (0, 0.6f), (1 / 60f, 0.6f));
                            }), id, writeDefaults: false);
                        if (context == "under_additive")
                            CaseState(add, offs[add], label + " additive", k.Make(label + " additive", c =>
                            {
                                RigSet(c, id, "blendshape", (0, 0), (0.75f, 30), (1.5f, 0));
                                RigSet(c, id, "position", (0, 0), (0.75f, 0.1f), (1.5f, 0));
                                RigSet(c, id, "parameter", (0, 0), (0.75f, 0.3f), (1.5f, 0));
                            }, loop: true), id, writeDefaults: false);
                    }
                controllers.Add(Custom(type, ctl.C));
            }
            EditorUtility.SetDirty(mesh);

            d.customExpressions = true;
            d.expressionParameters = parameters;
            d.customizeAnimationLayers = true;
            d.baseAnimationLayers = controllers.ToArray();
            d.specialAnimationLayers = new[]
            {
                new VRCAvatarDescriptor.CustomAnimLayer { type = VRCAvatarDescriptor.AnimLayerType.Sitting, isDefault = true, isEnabled = true },
                new VRCAvatarDescriptor.CustomAnimLayer { type = VRCAvatarDescriptor.AnimLayerType.TPose, isDefault = true, isEnabled = true },
                new VRCAvatarDescriptor.CustomAnimLayer { type = VRCAvatarDescriptor.AnimLayerType.IKPose, isDefault = true, isEnabled = true },
            };
        }
    }
}
