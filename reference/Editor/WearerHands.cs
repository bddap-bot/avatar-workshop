using System.Collections.Generic;
using System.Linq;
using UnityEngine;
using VRC.Dynamics;
using VRC.SDK3.Avatars.Components;
using VRC.SDK3.Dynamics.PhysBone.Components;

namespace AvatarWorkshop.Reference
{
    // The emulator puts no PhysBone collider on the descriptor's hands, so global ones stand where avatar-core puts the
    // client's (README, "Every recording stands in for the wearer's hand colliders").
    public static class WearerHands
    {
        const string StandIn = "HandColliderStandIn";

        // The PhysBone colliders on the descriptor's hand and finger collider bones that the build did not hold.
        public static int EmulatorColliders(VRCAvatarDescriptor d, Transform root, HashSet<VRCPhysBoneColliderBase> built) =>
            Slots(d, root).Where(s => s.config.transform != null)
                .Sum(s => (s.slot.StartsWith("finger") ? s.config.transform.parent : s.config.transform)?.GetComponents<VRCPhysBoneColliderBase>().Count(c => !built.Contains(c)) ?? 0);

        public static List<(string slot, VRCPhysBoneCollider collider)> Stand(VRCAvatarDescriptor d, Transform root)
        {
            var animator = root.GetComponent<Animator>();
            Skeleton skeleton = animator != null && animator.isHuman ? new Skeleton(animator, root) : null;
            var stood = new List<(string, VRCPhysBoneCollider)>();
            foreach (var (slot, config) in Slots(d, root))
            {
                var finger = slot.StartsWith("finger");
                var c = config.state == VRCAvatarDescriptor.ColliderConfig.State.Automatic && skeleton != null
                    ? skeleton.Automatic(slot)
                    : (config.transform, config.radius, config.height, config.position, config.rotation);
                if (c == null || c.Value.bone == null) continue;
                var (bone, radius, height, position, rotation) = c.Value;
                if (finger)
                {
                    if (bone.parent == null) continue;
                    (radius, height, position, rotation) = FingerCapsule(radius, height, bone);
                    bone = bone.parent;
                }
                var go = new GameObject($"{StandIn} {slot}");
                go.transform.SetParent(bone, false);
                var x = go.AddComponent<VRCPhysBoneCollider>();
                x.rootTransform = bone;
                x.shapeType = VRCPhysBoneColliderBase.ShapeType.Capsule;
                x.radius = radius; x.height = height; x.position = position; x.rotation = rotation;
                x.globalCollision = VRCPhysBoneBase.AdvancedBool.True;
                stood.Add((slot, x));
            }
            return stood;
        }

        static IEnumerable<(string slot, VRCAvatarDescriptor.ColliderConfig config)> Slots(VRCAvatarDescriptor d, Transform root) =>
            AvatarWorkshop.Exporter.Exporter.DescriptorColliders(d).Where(s => (s.slot.StartsWith("hand") || s.slot.StartsWith("finger"))
                && s.config.state != VRCAvatarDescriptor.ColliderConfig.State.Disabled
                && (s.config.state == VRCAvatarDescriptor.ColliderConfig.State.Automatic || (s.config.transform != null && s.config.transform.IsChildOf(root))));

        // A finger's capsule in its bone's parent's space, from the parent's origin through the bone, run on in the bone's
        // own space to the collider's height (avatar-core's `finger_capsule`).
        public static (float radius, float height, Vector3 position, Quaternion rotation) FingerCapsule(float radius, float height, Transform bone)
        {
            var t = bone.localPosition;
            var reach = t.normalized * Mathf.Max(height - t.magnitude, 0);
            var end = t + bone.localRotation * Vector3.Scale(bone.localScale, reach);
            var s = bone.localScale;
            var r = radius * Mathf.Max(Mathf.Abs(s.x), Mathf.Abs(s.y), Mathf.Abs(s.z));
            return (r, end.magnitude + 2 * r, end * 0.5f, Quaternion.FromToRotation(Vector3.up, end.sqrMagnitude > 0 ? end.normalized : Vector3.up));
        }

        // The humanoid's bones in the collider pose, every other node as it stands, composed as avatar-core composes
        // poses: an automatic hand or finger collider as the client computes it (avatar-core's `Skeleton::automatic`).
        sealed class Skeleton
        {
            struct Pose
            {
                public Vector3 t; public Quaternion r; public Vector3 s;
                public Pose Mul(Pose c) => new Pose { t = t + r * Vector3.Scale(s, c.t), r = (r * c.r).normalized, s = Vector3.Scale(s, c.s) };
            }

            readonly Animator animator;
            readonly Dictionary<Transform, (Vector3 position, Quaternion rotation)> posed = new Dictionary<Transform, (Vector3, Quaternion)>();
            readonly Dictionary<Transform, Pose> world = new Dictionary<Transform, Pose>();
            readonly Transform root;

            public Skeleton(Animator animator, Transform root)
            {
                this.animator = animator;
                this.root = root;
                var pose = AvatarWorkshop.Exporter.Exporter.ColliderPose(animator, root.GetComponentsInChildren<Transform>(true));
                if (pose != null)
                    foreach (var kv in pose) posed[animator.GetBoneTransform(kv.Key)] = kv.Value;
            }

            Transform Bone(HumanBodyBones b) => animator.GetBoneTransform(b);

            Pose At(Transform t)
            {
                if (world.TryGetValue(t, out var w)) return w;
                var (p, q) = posed.TryGetValue(t, out var x) ? x : (t.localPosition, t.localRotation);
                var local = new Pose { t = p, r = q.normalized, s = t.localScale };
                w = t == root || t.parent == null ? local : At(t.parent).Mul(local);
                return world[t] = w;
            }

            float Scale(Transform t) { var s = At(t).s; return Mathf.Max(Mathf.Abs(s.x), Mathf.Abs(s.y), Mathf.Abs(s.z)); }

            Vector3 Local(Transform t, Vector3 p) { var w = At(t); var v = Quaternion.Inverse(w.r) * (p - w.t); return new Vector3(v.x / w.s.x, v.y / w.s.y, v.z / w.s.z); }

            Quaternion Along(Transform t, Vector3 d)
            {
                var v = Quaternion.Inverse(At(t).r) * d;
                return v.sqrMagnitude > 0 && !float.IsNaN(v.sqrMagnitude) && !float.IsInfinity(v.sqrMagnitude) ? Quaternion.FromToRotation(Vector3.up, v.normalized) : Quaternion.identity;
            }

            public (Transform bone, float radius, float height, Vector3 position, Quaternion rotation)? Automatic(string slot)
            {
                var left = slot.EndsWith("Left");
                if (slot.StartsWith("hand"))
                {
                    var hand = Bone(left ? HumanBodyBones.LeftHand : HumanBodyBones.RightHand);
                    if (hand == null) return null;
                    var at = At(hand).t;
                    var roots = (left
                            ? new[] { HumanBodyBones.LeftIndexProximal, HumanBodyBones.LeftMiddleProximal, HumanBodyBones.LeftRingProximal, HumanBodyBones.LeftLittleProximal }
                            : new[] { HumanBodyBones.RightIndexProximal, HumanBodyBones.RightMiddleProximal, HumanBodyBones.RightRingProximal, HumanBodyBones.RightLittleProximal })
                        .Select(Bone).Where(b => b != null).Select(b => At(b).t).ToList();
                    if (roots.Count == 0)
                    {
                        var lower = Bone(left ? HumanBodyBones.LeftLowerArm : HumanBodyBones.RightLowerArm);
                        if (lower == null) return null;
                        var l = At(lower).t;
                        var length = Vector3.Distance(at, l);
                        return (hand, length * 0.2f / Scale(hand), 0f, Local(hand, at + (at - l).normalized * length * 0.2f), Quaternion.identity);
                    }
                    var mid = roots.Aggregate(Vector3.zero, (a, b) => a + b) / roots.Count;
                    var reach = Vector3.Distance(at, mid);
                    var across = Vector3.Cross((mid - at).normalized, Vector3.up).normalized;
                    return (hand, reach * 0.25f / Scale(hand), reach / Scale(hand), Local(hand, mid) * 0.85f, Along(hand, across));
                }
                var digit = slot.Substring("finger".Length, slot.Length - "finger".Length - (left ? 4 : 5));
                var side = left ? "Left" : "Right";
                HumanBodyBones B(string joint) => (HumanBodyBones)System.Enum.Parse(typeof(HumanBodyBones), side + digit + joint);
                var (proximal, intermediate, distal) = (Bone(B("Proximal")), Bone(B("Intermediate")), Bone(B("Distal")));
                if (proximal == null || intermediate == null) return null;
                var (from, to) = distal != null ? (intermediate, distal) : (proximal, intermediate);
                var span = Vector3.Distance(At(from).t, At(to).t);
                return (to, span * 0.4f / Scale(to), span * 2f / Scale(to), Vector3.zero, Quaternion.identity);
            }
        }
    }
}
