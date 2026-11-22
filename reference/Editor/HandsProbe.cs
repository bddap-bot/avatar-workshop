using System.Collections.Generic;
using System.Linq;
using UnityEngine;
using VRC.Dynamics;
using VRC.SDK3.Avatars.Components;
using VRC.SDK3.Dynamics.PhysBone.Components;

namespace AvatarWorkshop.Reference
{
    // The hand collider probe: the fixture's body with one chain per collision filter setting hanging in a row before it,
    // which the wearer's left hand and another player's hand sweep through, each a global PhysBone collider standing for
    // the client's hand colliders (`WearerHands` and the harness's outside hand).
    public static partial class Fixture
    {
        public static readonly (string name, VRCPhysBoneBase.AdvancedBool allow, bool self, bool others)[] HandFilters =
        {
            ("CollideAll", VRCPhysBoneBase.AdvancedBool.True, false, false),
            ("CollideNone", VRCPhysBoneBase.AdvancedBool.False, true, true),
            ("SelfOnly", VRCPhysBoneBase.AdvancedBool.Other, true, false),
            ("OthersOnly", VRCPhysBoneBase.AdvancedBool.Other, false, true),
            ("SelfAndOthers", VRCPhysBoneBase.AdvancedBool.Other, true, true),
            ("Nobody", VRCPhysBoneBase.AdvancedBool.Other, false, false),
        };

        public const float HandRowSpacing = 0.25f, HandRowTop = 1.1f, HandRowDepth = 0.35f, HandBone = 0.1f;
        public static float HandRowHalf => (HandFilters.Length - 1) * HandRowSpacing / 2 + 0.3f;
        // The sweeps cross the chains' third bones.
        public static Vector3 OnHandRow(float x) => new Vector3(x, HandRowTop - 2.5f * HandBone, HandRowDepth);
        public const float HandSweepSpeed = 0.5f;
        public static readonly (string name, float radius, float height) OtherHand = ("OtherHand", 0.05f, 0.15f);

        static void HandsLayers(VRCAvatarDescriptor d, Dictionary<string, Transform> t, Transform root)
        {
            var custom = VRCAvatarDescriptor.ColliderConfig.State.Custom;
            var off = new VRCAvatarDescriptor.ColliderConfig { state = VRCAvatarDescriptor.ColliderConfig.State.Disabled };
            d.collider_head = d.collider_torso = d.collider_handR = d.collider_footL = d.collider_footR = off;
            d.collider_fingerMiddleL = d.collider_fingerRingL = d.collider_fingerLittleL = off;
            d.collider_fingerIndexR = d.collider_fingerMiddleR = d.collider_fingerRingR = d.collider_fingerLittleR = off;
            d.collider_handL = new VRCAvatarDescriptor.ColliderConfig { state = custom, transform = t["LeftHand"], radius = 0.05f, height = 0.15f, position = new Vector3(-0.05f, 0, 0), rotation = Quaternion.Euler(0, 0, 90) };
            d.collider_fingerIndexL = new VRCAvatarDescriptor.ColliderConfig { state = custom, transform = t["LeftHandTip"], radius = 0.015f, height = 0.1f, position = Vector3.zero, rotation = Quaternion.identity };

            var row = new GameObject("HandRow").transform;
            row.SetParent(root, false);
            for (int i = 0; i < HandFilters.Length; i++)
            {
                var spec = HandFilters[i];
                var p = row;
                for (int k = 0; k < 4; k++)
                {
                    var go = new GameObject(k == 0 ? spec.name : $"{spec.name}.{k}");
                    go.transform.SetParent(p, false);
                    go.transform.localPosition = k == 0 ? new Vector3(-HandRowHalf + 0.3f + i * HandRowSpacing, HandRowTop, HandRowDepth) : new Vector3(0, -HandBone, 0);
                    p = go.transform;
                }
                var pb = row.Find(spec.name).gameObject.AddComponent<VRCPhysBone>();
                pb.pull = 0.2f; pb.spring = 0.3f; pb.gravity = 0; pb.radius = 0.03f;
                pb.allowCollision = spec.allow;
                pb.collisionFilter = new VRCPhysBoneBase.PermissionFilter { allowSelf = spec.self, allowOthers = spec.others };
                pb.allowGrabbing = VRCPhysBoneBase.AdvancedBool.False;
                pb.allowPosing = VRCPhysBoneBase.AdvancedBool.False;
            }
        }

        public static readonly string[] HandSettings =
            HandFilters.Select(f => "chain." + f.name).Concat(new[] { "hand.wearer.hand", "hand.wearer.finger", "hand.other" }).ToArray();

        public static SortedDictionary<string, int> CountHands(GameObject root, IEnumerable<(string slot, VRCPhysBoneCollider collider)> wearer, IEnumerable<VRCPhysBoneColliderBase> outside)
        {
            var c = new SortedDictionary<string, int>(System.StringComparer.Ordinal);
            foreach (var k in HandSettings) c[k] = 0;
            foreach (var pb in root.GetComponentsInChildren<VRCPhysBoneBase>(true))
            {
                var spec = HandFilters.FirstOrDefault(f => f.name == pb.name);
                if (spec.name != null && pb.allowCollision == spec.allow && pb.collisionFilter.allowSelf == spec.self && pb.collisionFilter.allowOthers == spec.others && pb.radius > 0)
                    c["chain." + spec.name]++;
            }
            foreach (var (slot, _) in wearer) c["hand.wearer." + (slot.StartsWith("finger") ? "finger" : "hand")]++;
            foreach (var x in outside)
                if (x.globalCollision == VRCPhysBoneBase.AdvancedBool.True) c["hand.other"]++;
            return c;
        }
    }
}
