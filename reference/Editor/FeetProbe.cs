using System;
using System.Collections.Generic;
using System.Linq;
using UnityEditor;
using UnityEngine;
using VRC.SDK3.Avatars.Components;

namespace AvatarWorkshop.Reference
{
    // The Feet height basis probe: the rotation probe's only clips with height based upon Feet and left unbaked turn
    // about all three axes at once, by one size, over 1.25 cycles, so its recording cannot separate the turn, the
    // cycle and the foot. Each clip here plays beside the same clip with its height baked.
    public static partial class Fixture
    {
        // A property: DriftAxes is initialised in another file of this partial class, in an order C# leaves unspecified.
        internal static (string name, Vector3 axis)[] FeetAxes => DriftAxes.Where(a => a.name != "xy").ToArray();
        internal static readonly string[] FeetKinds = { "moving", "pose" };
        internal static readonly bool[] FeetLoopPose = { false, true };
        internal static readonly bool[] FeetHeightBaked = { false, true };
        const float FeetLength = CaseFrames / 5f / Harness.Rate;

        internal static void FeetSettings(AnimationClipSettings s, bool loopPose, bool heightBaked)
        {
            Original(s);
            s.keepOriginalPositionY = false; s.heightFromFeet = true;
            s.loopTime = true; s.loopBlend = loopPose; s.loopBlendPositionY = heightBaked;
        }

        static void FeetLayers(VRCAvatarDescriptor d) => CaseLayers(d, "Feet", k =>
        {
            var clips = new List<AnimationClip>();
            foreach (var kind in FeetKinds)
                foreach (var (axes, axis) in FeetAxes)
                    foreach (var degrees in DriftDegrees)
                        foreach (var loopPose in FeetLoopPose)
                            foreach (var baked in FeetHeightBaked)
                            {
                                var name = $"feet {kind} {axes} {degrees}{(loopPose ? " loop pose" : "")} {(baked ? "baked" : "unbaked")}";
                                var turn = Quaternion.AngleAxis(degrees, axis);
                                Action<AnimationClip> fill = kind == "moving"
                                    ? c => DriftingWalk(c, axis, degrees, FeetLength)
                                    : c => HeldPose(c, turn, g => turn * Quaternion.Euler(GoalTurns[g] * 0.25f), 1);
                                clips.Add(k.Make(name, fill, settings: s => FeetSettings(s, loopPose, baked)));
                            }
            return clips;
        });
    }
}
