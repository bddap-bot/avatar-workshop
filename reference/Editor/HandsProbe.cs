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
    // the client's hand colliders (`WearerHands` and the harness's outside hand). Behind it, chains cross the wearer's
    // right hand and fingers (`Reaches`).
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
            d.collider_head = d.collider_torso = d.collider_footL = d.collider_footR = off;
            d.collider_fingerMiddleL = d.collider_fingerRingL = d.collider_fingerLittleL = off;
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
            HandsReaches(d, root);
        }

        // `up` counts from the bone the collider stands on (-1 its child); a finger collider stands on its transform's
        // parent.
        public static readonly (string name, string slot, int up)[] Reaches =
        {
            ("FingerOwn", "fingerIndexR", 0),
            ("FingerParent", "fingerMiddleR", 1),
            ("FingerAncestor", "fingerRingR", 2),
            ("FingerChild", "fingerLittleR", -1),
            ("HandOwn", "handR", 0),
        };

        // Offsets from the collider's middle: Across is pushed from the start; the others' roots are inside the
        // collider, or (Rim) outside it but within the chain's radius.
        public static readonly (string suffix, Vector3 root, Vector3 bone)[] ReachChains =
        {
            ("Across", new Vector3(-0.06f, 0.01f, 0), new Vector3(0.04f, 0, 0)),
            ("Inside", new Vector3(0, 0.01f, -0.015f), new Vector3(0.04f, 0, 0)),
            ("Mid", new Vector3(0, 0.015f, 0.015f), new Vector3(0.04f, 0, 0)),
            ("Rim", new Vector3(0, 0.025f, -0.03f), new Vector3(0.04f, 0, 0)),
        };

        public const float ReachBone = 0.05f, ReachRadius = 0.02f, ReachHeight = 0.08f;

        public static readonly (string suffix, System.Action<VRCPhysBone> set, Vector3? bone)[] ReachVariants =
        {
            ("PullLow", pb => pb.pull = 0.05f, null),
            ("PullHigh", pb => pb.pull = 0.6f, null),
            ("SpringNone", pb => pb.spring = 0, null),
            ("SpringHigh", pb => pb.spring = 0.9f, null),
            ("Advanced", pb => pb.integrationType = VRCPhysBoneBase.IntegrationType.Advanced, null),
            ("AdvancedStiff", pb => { pb.integrationType = VRCPhysBoneBase.IntegrationType.Advanced; pb.stiffness = 0.6f; }, null),
            ("Immobile", pb => { pb.immobileType = VRCPhysBoneBase.ImmobileType.World; pb.immobile = 1; }, null),
            ("RadiusSmall", pb => pb.radius = 0.005f, null),
            ("RadiusLarge", pb => pb.radius = 0.015f, null),
            ("Up", _ => { }, new Vector3(0, 0.04f, 0)),
            ("Diagonal", _ => { }, new Vector3(0.02828f, 0.02828f, 0)),
            ("Down", _ => { }, new Vector3(0.02828f, -0.02828f, 0)),
            ("Along", _ => { }, new Vector3(0, 0, 0.04f)),
            ("Like", pb =>
            {
                pb.pull = 0.05f; pb.spring = 0.8f; pb.radius = 0.005f;
                pb.integrationType = VRCPhysBoneBase.IntegrationType.Advanced;
                pb.immobileType = VRCPhysBoneBase.ImmobileType.World; pb.immobile = 1;
            }, null),
        };

        public static readonly string[] ReachVaried = { "FingerOwn", "HandOwn" };

        // One-bone chains with neither pull nor spring around the collider on the chains' own bone, so each step moves a
        // bone only by the push: roots at heights above the axis up to its reach, leading out at angles from level.
        public static readonly float[] MapHeights = { 0.005f, 0.01f, 0.015f, 0.02f, 0.025f, 0.029f };
        public static readonly int[] MapAngles = { -45, -20, 0, 10, 20, 30, 45, 60 };

        // Bones crossing the axis at an angle: where the collider's point nearest a bone's end is farther from the bone
        // than the axis is, a push found from that point alone and one at the segments' nearest points part.
        public static readonly float[] CrossHeights = { 0.005f, 0.015f, 0.025f, 0.029f };
        public static readonly int[] CrossAngles = { 15, 30, 45, 60, 75 };
        public static readonly float[] CrossAts = { 0.1f, 0.3f, 0.7f };

        public static readonly float[] AlongHeights = { 0.01f, 0.025f };
        public static readonly float[] AlongOffsets = { -0.05f, -0.03f, -0.01f, 0.01f };
        public static readonly (string name, Vector3 bone)[] AlongWays =
        {
            ("Forward", new Vector3(0, 0, 0.04f)),
            ("Back", new Vector3(0, 0, -0.04f)),
            ("Rising", 0.04f * new Vector3(0, Mathf.Sin(20 * Mathf.Deg2Rad), Mathf.Cos(20 * Mathf.Deg2Rad))),
        };

        static IEnumerable<(string name, Vector3 root, Vector3 bone, System.Action<VRCPhysBone> set, int bones)> ReachChainsOf(string relation)
        {
            System.Action<VRCPhysBone> none = _ => { };
            var chains = ReachChains.Select(c => (relation + c.suffix, c.root, c.bone, none, 3));
            if (!ReachVaried.Contains(relation)) return chains;
            var varied = ReachChains.Where(c => c.suffix != "Across").SelectMany(c => ReachVariants.Select(v => (relation + c.suffix + v.suffix, c.root, v.bone ?? c.bone, v.set, 3)));
            var map = relation != ReachVaried[0] ? Enumerable.Empty<(string, Vector3, Vector3, System.Action<VRCPhysBone>, int)>() : MapHeights.SelectMany(h => MapAngles.Select(a => (
                $"{relation}MapH{Mathf.RoundToInt(h * 1000)}A{(a < 0 ? "m" : "")}{Mathf.Abs(a)}",
                new Vector3(0, h, 0),
                0.04f * new Vector3(Mathf.Cos(a * Mathf.Deg2Rad), Mathf.Sin(a * Mathf.Deg2Rad), 0),
                (System.Action<VRCPhysBone>)(pb => { pb.pull = 0; pb.spring = 0; }),
                1)));
            var cross = relation != ReachVaried[0] ? Enumerable.Empty<(string, Vector3, Vector3, System.Action<VRCPhysBone>, int)>() : CrossHeights.SelectMany(h => CrossAngles.SelectMany(a => CrossAts.Select(f =>
            {
                var bone = 0.04f * new Vector3(Mathf.Cos(a * Mathf.Deg2Rad), 0, Mathf.Sin(a * Mathf.Deg2Rad));
                return ($"{relation}CrossH{Mathf.RoundToInt(h * 1000)}A{a}F{Mathf.RoundToInt(f * 10)}", new Vector3(0, h, 0) - f * bone, bone,
                    (System.Action<VRCPhysBone>)(pb => { pb.pull = 0; pb.spring = 0; }), 1);
            })));
            var along = relation != ReachVaried[1] ? Enumerable.Empty<(string, Vector3, Vector3, System.Action<VRCPhysBone>, int)>() : AlongHeights.SelectMany(h => AlongOffsets.SelectMany(z => AlongWays.Select(w => (
                $"{relation}AlongH{Mathf.RoundToInt(h * 1000)}Z{(z < 0 ? "m" : "")}{Mathf.RoundToInt(Mathf.Abs(z) * 1000)}{w.name}",
                new Vector3(0, h, z),
                w.bone,
                (System.Action<VRCPhysBone>)(pb => { pb.pull = 0; pb.spring = 0; }),
                1))));
            return chains.Concat(varied).Concat(map).Concat(cross).Concat(along);
        }

        // The runs hang from the avatar's root, which the body's first pose leaves where it is.
        static void HandsReaches(VRCAvatarDescriptor d, Transform root)
        {
            var custom = VRCAvatarDescriptor.ColliderConfig.State.Custom;
            var along = Quaternion.Euler(90, 0, 0);
            for (int i = 0; i < Reaches.Length; i++)
            {
                var (name, slot, up) = Reaches[i];
                var run = new Transform[4];
                for (int k = 0; k < 4; k++)
                {
                    run[k] = new GameObject($"{name}Run.{k}").transform;
                    run[k].SetParent(k == 0 ? root : run[k - 1], false);
                    run[k].localPosition = k == 0 ? new Vector3(-0.5f + i * 0.25f, 1.1f, -0.3f) : new Vector3(0, 0, -ReachBone);
                }
                bool finger = slot.StartsWith("finger");
                var stands = run[2];
                var config = finger
                    ? new VRCAvatarDescriptor.ColliderConfig { state = custom, transform = run[3], radius = ReachRadius, height = ReachHeight, position = Vector3.zero, rotation = Quaternion.identity }
                    : new VRCAvatarDescriptor.ColliderConfig { state = custom, transform = stands, radius = ReachRadius, height = ReachHeight, position = new Vector3(0, 0, -ReachHeight / 2 + ReachRadius), rotation = along };
                switch (slot)
                {
                    case "fingerIndexR": d.collider_fingerIndexR = config; break;
                    case "fingerMiddleR": d.collider_fingerMiddleR = config; break;
                    case "fingerRingR": d.collider_fingerRingR = config; break;
                    case "fingerLittleR": d.collider_fingerLittleR = config; break;
                    case "handR": d.collider_handR = config; break;
                    default: throw new System.ArgumentException(slot);
                }
                var middle = stands.TransformPoint(new Vector3(0, 0, finger ? -ReachHeight / 2 : -ReachHeight / 2 + ReachRadius));
                foreach (var (chainName, at, bone, set, bones) in ReachChainsOf(name))
                {
                    var p = run[2 - up];
                    GameObject chain = null;
                    for (int k = 0; k <= bones; k++)
                    {
                        var go = new GameObject(k == 0 ? chainName : $"{chainName}.{k}");
                        go.transform.SetParent(p, false);
                        if (k == 0) go.transform.position = middle + at;
                        else go.transform.localPosition = bone;
                        chain = chain ?? go;
                        p = go.transform;
                    }
                    var pb = chain.AddComponent<VRCPhysBone>();
                    pb.pull = 0.2f; pb.spring = 0.3f; pb.gravity = 0; pb.radius = 0.01f;
                    pb.allowCollision = VRCPhysBoneBase.AdvancedBool.True;
                    pb.allowGrabbing = VRCPhysBoneBase.AdvancedBool.False;
                    pb.allowPosing = VRCPhysBoneBase.AdvancedBool.False;
                    set(pb);
                }
            }
        }

        public static readonly string[] HandSettings =
            HandFilters.Select(f => "chain." + f.name).Concat(Reaches.SelectMany(r => ReachChainsOf(r.name).Select(c => "reach." + c.name))).Concat(new[] { "hand.wearer.hand", "hand.wearer.finger", "hand.other" }).ToArray();

        public static SortedDictionary<string, int> CountHands(GameObject root, IEnumerable<(string slot, VRCPhysBoneCollider collider)> wearer, IEnumerable<VRCPhysBoneColliderBase> outside)
        {
            var c = new SortedDictionary<string, int>(System.StringComparer.Ordinal);
            foreach (var k in HandSettings) c[k] = 0;
            foreach (var pb in root.GetComponentsInChildren<VRCPhysBoneBase>(true))
            {
                var spec = HandFilters.FirstOrDefault(f => f.name == pb.name);
                if (spec.name != null && pb.allowCollision == spec.allow && pb.collisionFilter.allowSelf == spec.self && pb.collisionFilter.allowOthers == spec.others && pb.radius > 0)
                    c["chain." + spec.name]++;
                if (c.ContainsKey("reach." + pb.name) && pb.radius > 0) c["reach." + pb.name]++;
            }
            foreach (var (slot, _) in wearer) c["hand.wearer." + (slot.StartsWith("finger") ? "finger" : "hand")]++;
            foreach (var x in outside)
                if (x.globalCollision == VRCPhysBoneBase.AdvancedBool.True) c["hand.other"]++;
            return c;
        }
    }
}
