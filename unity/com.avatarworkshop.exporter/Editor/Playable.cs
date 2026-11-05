using System;
using System.Collections.Generic;
using System.Linq;
using UnityEditor;
using UnityEditor.Animations;
using UnityEngine;
using VRC.SDK3.Avatars.Components;
using LayerType = VRC.SDK3.Avatars.Components.VRCAvatarDescriptor.AnimLayerType;

namespace AvatarWorkshop.Exporter
{
    // The controller and mask each playable layer runs: the avatar's own, or for a layer left at the default the
    // SDK's sample controller of that layer with the mask imposed on it, as Av3Emulator substitutes them.
    public static class Playable
    {
        static readonly Dictionary<LayerType, (string guid, string name)> Defaults = new Dictionary<LayerType, (string, string)>
        {
            { LayerType.Base, ("4e4e1a372a526074884b7311d6fc686b", "vrc_AvatarV3LocomotionLayer") },
            { LayerType.Additive, ("573a1373059632b4d820876efe2d277f", "vrc_AvatarV3IdleLayer") },
            { LayerType.Gesture, ("404d228aeae421f4590305bc4cdaba16", "vrc_AvatarV3HandsLayer") },
            { LayerType.Action, ("3e479eeb9db24704a828bffb15406520", "vrc_AvatarV3ActionLayer") },
            { LayerType.FX, ("d40be620cf6c698439a2f0a5144919fe", "vrc_AvatarV3FaceLayer") },
            { LayerType.Sitting, ("1268460c14f873240981bf15aa88b21a", "vrc_AvatarV3SittingLayer") },
            { LayerType.TPose, ("00121b5812372b74f9012473856d8acf", "vrc_AvatarV3UtilityTPose") },
            { LayerType.IKPose, ("a9b90a833b3486e4b82834c9d1f7c4ee", "vrc_AvatarV3UtilityIKPose") },
        };

        public static IEnumerable<VRCAvatarDescriptor.CustomAnimLayer> Layers(VRCAvatarDescriptor d) =>
            (d.baseAnimationLayers ?? new VRCAvatarDescriptor.CustomAnimLayer[0]).Concat(d.specialAnimationLayers ?? new VRCAvatarDescriptor.CustomAnimLayer[0]);

        public static RuntimeAnimatorController Controller(VRCAvatarDescriptor.CustomAnimLayer l)
        {
            if (!l.isDefault) return l.animatorController ? l.animatorController : null;
            if (!Defaults.TryGetValue(l.type, out var sdk)) return null;
            var controller = AssetDatabase.LoadAssetAtPath<AnimatorController>(AssetDatabase.GUIDToAssetPath(sdk.guid))
                ?? AssetDatabase.FindAssets(sdk.name + " t:AnimatorController").Select(AssetDatabase.GUIDToAssetPath)
                    .Where(p => p.EndsWith("/" + sdk.name + ".controller")).Select(AssetDatabase.LoadAssetAtPath<AnimatorController>).FirstOrDefault();
            if (controller == null) throw new Exception($"the VRChat SDK's default {l.type} controller {sdk.name} is not in the project");
            return controller;
        }

        public static AnimatorController Resolved(RuntimeAnimatorController c) =>
            c as AnimatorController ?? (c as AnimatorOverrideController)?.runtimeAnimatorController as AnimatorController;

        public static AvatarMask Mask(VRCAvatarDescriptor.CustomAnimLayer l)
        {
            if (!l.isDefault && !(l.mask == null && l.type == LayerType.FX)) return l.mask ? l.mask : null;
            switch (l.type)
            {
                case LayerType.FX: return Parts();
                case LayerType.Gesture: return Parts(AvatarMaskBodyPart.LeftFingers, AvatarMaskBodyPart.RightFingers);
                case LayerType.TPose:
                case LayerType.IKPose:
                    return Parts(Enum.GetValues(typeof(AvatarMaskBodyPart)).Cast<AvatarMaskBodyPart>().Where(p => p != AvatarMaskBodyPart.LastBodyPart
                        && p != AvatarMaskBodyPart.LeftFootIK && p != AvatarMaskBodyPart.RightFootIK && p != AvatarMaskBodyPart.LeftHandIK && p != AvatarMaskBodyPart.RightHandIK).ToArray());
                default: return null;
            }
        }

        static AvatarMask Parts(params AvatarMaskBodyPart[] active)
        {
            var m = new AvatarMask();
            for (var p = AvatarMaskBodyPart.Root; p < AvatarMaskBodyPart.LastBodyPart; p++) m.SetHumanoidBodyPartActive(p, active.Contains(p));
            return m;
        }
    }
}
