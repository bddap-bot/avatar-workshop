using System.Collections.Generic;
using System.Linq;
using UnityEngine;
using VRC.Dynamics;
using VRC.SDK3.Avatars.Components;
using VRC.SDK3.Dynamics.PhysBone.Components;

namespace AvatarWorkshop.Reference
{
    // The hand collider probe: the fixture's body with one chain per collision filter setting hanging in a row before it,
    // which the wearer's left hand and another player's hand sweep through. The SDK's editor simulation puts no PhysBone
    // collider on the descriptor's hands (the emulator extracts its senders only), so global PhysBone colliders, which
    // collide with every chain whose filter admits their player as the client's hand colliders do, stand on the wearer's
    // hand and index finger where the client puts their colliders, and on the other player's hand.
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

        public const string StandIn = "HandColliderStandIn";
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

            var hand = d.collider_handL;
            Global(t["LeftHand"], "hand", hand.radius, hand.height, hand.position, hand.rotation, root);
            var (radius, height, position, rotation) = FingerCapsule(d.collider_fingerIndexL, t["LeftHandTip"]);
            Global(t["LeftHand"], "finger", radius, height, position, rotation, root);

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

        // A global collider on its own child of `bone`, so the descriptor's bones carry only what the emulator puts there.
        static void Global(Transform bone, string part, float radius, float height, Vector3 position, Quaternion rotation, Transform root)
        {
            var go = new GameObject($"{StandIn} {part}");
            go.transform.SetParent(bone, false);
            var c = go.AddComponent<VRCPhysBoneCollider>();
            c.rootTransform = bone;
            c.shapeType = VRCPhysBoneColliderBase.ShapeType.Capsule;
            c.radius = radius; c.height = height; c.position = position; c.rotation = rotation;
            c.globalCollision = VRCPhysBoneBase.AdvancedBool.True;
        }

        // Where the client puts a finger's collider, in its bone's parent's space: from the parent's origin through the
        // bone, run on in the bone's own space to the collider's height (avatar-core's `finger_capsule`).
        public static (float radius, float height, Vector3 position, Quaternion rotation) FingerCapsule(VRCAvatarDescriptor.ColliderConfig c, Transform bone)
        {
            var t = bone.localPosition;
            var reach = t.normalized * Mathf.Max(c.height - t.magnitude, 0);
            var end = t + bone.localRotation * Vector3.Scale(bone.localScale, reach);
            var s = bone.localScale;
            var radius = c.radius * Mathf.Max(Mathf.Abs(s.x), Mathf.Abs(s.y), Mathf.Abs(s.z));
            return (radius, end.magnitude + 2 * radius, end * 0.5f, Quaternion.FromToRotation(Vector3.up, end.sqrMagnitude > 0 ? end.normalized : Vector3.up));
        }

        public static readonly string[] HandSettings =
            HandFilters.Select(f => "chain." + f.name).Concat(new[] { "hand.wearer.hand", "hand.wearer.finger", "hand.other" }).ToArray();

        public static SortedDictionary<string, int> CountHands(GameObject root, IEnumerable<VRCPhysBoneColliderBase> outside)
        {
            var c = new SortedDictionary<string, int>(System.StringComparer.Ordinal);
            foreach (var k in HandSettings) c[k] = 0;
            foreach (var pb in root.GetComponentsInChildren<VRCPhysBoneBase>(true))
            {
                var spec = HandFilters.FirstOrDefault(f => f.name == pb.name);
                if (spec.name != null && pb.allowCollision == spec.allow && pb.collisionFilter.allowSelf == spec.self && pb.collisionFilter.allowOthers == spec.others && pb.radius > 0)
                    c["chain." + spec.name]++;
            }
            foreach (var x in root.GetComponentsInChildren<VRCPhysBoneColliderBase>(true))
                if (x.globalCollision == VRCPhysBoneBase.AdvancedBool.True && x.name.StartsWith(StandIn))
                    c["hand.wearer." + x.name.Substring(StandIn.Length + 1)]++;
            foreach (var x in outside)
                if (x.globalCollision == VRCPhysBoneBase.AdvancedBool.True) c["hand.other"]++;
            return c;
        }
    }
}
