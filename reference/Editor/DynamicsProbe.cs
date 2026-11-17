using System;
using System.Collections.Generic;
using System.Linq;
using UnityEditor;
using UnityEditor.Animations;
using UnityEngine;
using VRC.Dynamics;
using VRC.SDK3.Avatars.Components;
using VRC.SDK3.Avatars.ScriptableObjects;
using VRC.SDK3.Dynamics.Constraint.Components;
using VRC.SDK3.Dynamics.Contact.Components;
using VRC.SDK3.Dynamics.PhysBone.Components;

namespace AvatarWorkshop.Reference
{
    // The dynamics probe: the fixture's body with one constraint or contact receiver per setting value, each on a
    // transform of its own, so each has a reference trace of its own; and senders outside the avatar, which the
    // harness moves through the receivers, standing for another avatar's part and a hand.
    public static partial class Fixture
    {
        public const string DynamicsTag = "AWProbe";
        public static readonly string[] HandTags = { "Hand", "HandL", "Finger", "FingerL" };
        // The contact player id of the senders outside the avatar; the avatar's contacts keep the SDK's default.
        public const int OtherPlayer = 1000;
        public const string SelfPass = "SelfPass";
        public const float DefaultMinVelocity = 0.05f;

        public static readonly (string name, ContactBase.ShapeType shape, float radius, float height, string[] tags)[] OutsideSenders =
        {
            ("OtherPart", ContactBase.ShapeType.Sphere, 0.05f, 0, new[] { DynamicsTag }),
            ("OtherHand", ContactBase.ShapeType.Capsule, 0.04f, 0.15f, HandTags),
        };

        static readonly string[] AllTags = new[] { DynamicsTag }.Concat(HandTags).ToArray();

        public static readonly (string name, ContactReceiver.ReceiverType type, ContactBase.ShapeType shape, bool self, bool others, bool local, string[] tags, float minVelocity)[] Receivers =
        {
            ("Constant Sphere", ContactReceiver.ReceiverType.Constant, ContactBase.ShapeType.Sphere, true, true, false, AllTags, DefaultMinVelocity),
            ("Constant Capsule", ContactReceiver.ReceiverType.Constant, ContactBase.ShapeType.Capsule, true, true, false, AllTags, DefaultMinVelocity),
            ("OnEnter Sphere", ContactReceiver.ReceiverType.OnEnter, ContactBase.ShapeType.Sphere, true, true, false, AllTags, DefaultMinVelocity),
            ("OnEnter Capsule", ContactReceiver.ReceiverType.OnEnter, ContactBase.ShapeType.Capsule, true, true, false, AllTags, DefaultMinVelocity),
            ("Proximity Sphere", ContactReceiver.ReceiverType.Proximity, ContactBase.ShapeType.Sphere, true, true, false, AllTags, DefaultMinVelocity),
            ("Proximity Capsule", ContactReceiver.ReceiverType.Proximity, ContactBase.ShapeType.Capsule, true, true, false, AllTags, DefaultMinVelocity),
            ("Self Only", ContactReceiver.ReceiverType.Proximity, ContactBase.ShapeType.Sphere, true, false, false, AllTags, DefaultMinVelocity),
            ("Others Only", ContactReceiver.ReceiverType.Proximity, ContactBase.ShapeType.Sphere, false, true, false, AllTags, DefaultMinVelocity),
            ("Nobody", ContactReceiver.ReceiverType.Proximity, ContactBase.ShapeType.Sphere, false, false, false, AllTags, DefaultMinVelocity),
            ("Local Only", ContactReceiver.ReceiverType.Proximity, ContactBase.ShapeType.Sphere, true, true, true, AllTags, DefaultMinVelocity),
            ("Part Tag", ContactReceiver.ReceiverType.Proximity, ContactBase.ShapeType.Sphere, true, true, false, new[] { DynamicsTag }, DefaultMinVelocity),
            ("Hand Tags", ContactReceiver.ReceiverType.Proximity, ContactBase.ShapeType.Sphere, true, true, false, HandTags, DefaultMinVelocity),
            ("Unmatched Tag", ContactReceiver.ReceiverType.Proximity, ContactBase.ShapeType.Sphere, true, true, false, new[] { "AWNobody" }, DefaultMinVelocity),
            ("Min Velocity", ContactReceiver.ReceiverType.OnEnter, ContactBase.ShapeType.Sphere, true, true, false, AllTags, 1),
        };

        public const float RowSpacing = 0.3f, RowHeight = 1, RowDepth = 0.7f;
        public static float RowHalfLength => (Receivers.Length - 1) * RowSpacing / 2 + 0.4f;
        public static Vector3 OnRow(float x) => new Vector3(x, RowHeight, RowDepth);
        public static readonly Vector3 Parked = new Vector3(0, -10, 0);
        // The avatar's own sender crosses the row and back in this many seconds, with the clip's auto tangents.
        const float SelfPassSeconds = 9;

        public static readonly Dictionary<string, Type> ConstraintKinds = new Dictionary<string, Type>
        {
            ["Position"] = typeof(VRCPositionConstraint), ["Rotation"] = typeof(VRCRotationConstraint), ["Scale"] = typeof(VRCScaleConstraint),
            ["Parent"] = typeof(VRCParentConstraint), ["Aim"] = typeof(VRCAimConstraint), ["LookAt"] = typeof(VRCLookAtConstraint),
        };

        public static readonly Dictionary<string, string[]> MaskFields = new Dictionary<string, string[]>
        {
            ["Position"] = new[] { "AffectsPositionX", "AffectsPositionY", "AffectsPositionZ" },
            ["Rotation"] = new[] { "AffectsRotationX", "AffectsRotationY", "AffectsRotationZ" },
            ["Scale"] = new[] { "AffectsScaleX", "AffectsScaleY", "AffectsScaleZ" },
            ["Parent"] = new[] { "AffectsPositionX", "AffectsPositionY", "AffectsPositionZ", "AffectsRotationX", "AffectsRotationY", "AffectsRotationZ" },
            ["Aim"] = new[] { "AffectsRotationX", "AffectsRotationY", "AffectsRotationZ" },
            ["LookAt"] = new string[0],
        };

        static void DynamicsLayers(VRCAvatarDescriptor d, Dictionary<string, Transform> t, Transform root)
        {
            var group = new GameObject("Constraints").transform;
            group.SetParent(root, false);
            var local = new GameObject("LocalConstraints").transform;
            local.SetParent(t["Chest"], false);
            int placedCount = 0;
            Transform Target(Transform parent, string name)
            {
                var go = new GameObject(name);
                go.transform.SetParent(parent, false);
                int i = placedCount++;
                go.transform.localPosition = new Vector3(-1.2f + 0.3f * (i % 9), 0.4f + 0.25f * (i / 9), -0.5f);
                return go.transform;
            }

            var scaleA = new GameObject("ScaleA").transform;
            scaleA.SetParent(t["LeftHand"], false);
            var scaleB = new GameObject("ScaleB").transform;
            scaleB.SetParent(t["RightHand"], false);
            scaleB.localScale = new Vector3(0.5f, 1.5f, 1);
            (Transform, float)[] One(string kind) => kind == "Scale" ? new[] { (scaleA, 1f) } : new[] { (t["LeftHand"], 1f) };
            (Transform, float)[] Two(string kind) => kind == "Scale" ? new[] { (scaleA, 0.3f), (scaleB, 0.7f) } : new[] { (t["LeftHand"], 0.3f), (t["RightHand"], 0.7f) };

            VRCConstraintBase Add(Transform target, string kind, (Transform src, float weight)[] sources, params (string name, object value)[] fields)
            {
                var c = (VRCConstraintBase)target.gameObject.AddComponent(ConstraintKinds[kind]);
                foreach (var (src, weight) in sources) c.Sources.Add(new VRCConstraintSource { SourceTransform = src, Weight = weight });
                c.IsActive = true; c.Locked = true;
                Flags(c, fields);
                return c;
            }
            VRCConstraintBase Make(string kind, string variant, (Transform, float)[] sources, params (string, object)[] fields) =>
                Add(Target(group, $"{kind} {variant}"), kind, sources, fields);

            foreach (var kind in ConstraintKinds.Keys)
            {
                Make(kind, "one source", One(kind));
                Make(kind, "two sources", Two(kind));
                foreach (var mask in MaskFields[kind]) Make(kind, "no " + mask.Substring("Affects".Length), Two(kind), (mask, false));
                Add(Target(local, $"{kind} local"), kind, Two(kind), ("SolveInLocalSpace", true));
            }

            Make("Position", "at rest", new[] { (t["LeftHand"], 0.4f) }, ("GlobalWeight", 0.6f), ("PositionAtRest", new Vector3(0.1f, 1.2f, 0.3f)));
            Make("Rotation", "at rest", new[] { (t["LeftHand"], 0.4f) }, ("GlobalWeight", 0.6f), ("RotationAtRest", new Vector3(20, 40, -30)));
            Make("Scale", "at rest", new[] { (scaleA, 0.4f) }, ("GlobalWeight", 0.6f), ("ScaleAtRest", new Vector3(0.5f, 2, 1.5f)));
            Make("Parent", "at rest", new[] { (t["LeftHand"], 0.4f) }, ("GlobalWeight", 0.6f), ("PositionAtRest", new Vector3(0.1f, 1.2f, 0.3f)), ("RotationAtRest", new Vector3(20, 40, -30)));
            Make("Aim", "at rest", new[] { (t["LeftHand"], 0.4f) }, ("GlobalWeight", 0.6f), ("RotationAtRest", new Vector3(20, 40, -30)));
            Make("LookAt", "at rest", new[] { (t["LeftHand"], 0.4f) }, ("GlobalWeight", 0.6f), ("RotationAtRest", new Vector3(20, 40, -30)));

            Make("Position", "offset", One("Position"), ("PositionOffset", new Vector3(0.05f, -0.1f, 0.2f)));
            Make("Rotation", "offset", One("Rotation"), ("RotationOffset", new Vector3(30, -20, 10)));
            Make("Scale", "offset", One("Scale"), ("ScaleOffset", new Vector3(1.5f, 0.5f, 2)));
            var parentOffset = Make("Parent", "offset", new (Transform, float)[0]);
            parentOffset.Sources.Add(new VRCConstraintSource { SourceTransform = t["LeftHand"], Weight = 1, ParentPositionOffset = new Vector3(0.05f, -0.1f, 0.2f), ParentRotationOffset = new Vector3(30, -20, 10) });
            Make("Aim", "offset", One("Aim"), ("RotationOffset", new Vector3(30, -20, 10)));
            Make("LookAt", "offset", One("LookAt"), ("RotationOffset", new Vector3(30, -20, 10)));

            Make("Aim", "up SceneUp", One("Aim"), ("WorldUp", VRCConstraintBase.WorldUpType.SceneUp));
            Make("Aim", "up ObjectUp", One("Aim"), ("WorldUp", VRCConstraintBase.WorldUpType.ObjectUp), ("WorldUpTransform", t["Chest"]));
            Make("Aim", "up ObjectRotationUp", One("Aim"), ("WorldUp", VRCConstraintBase.WorldUpType.ObjectRotationUp), ("WorldUpTransform", t["RightHand"]), ("WorldUpVector", new Vector3(1, 0, 0)));
            Make("Aim", "up Vector", One("Aim"), ("WorldUp", VRCConstraintBase.WorldUpType.Vector), ("WorldUpVector", new Vector3(0.3f, 1, 0.2f)));
            Make("Aim", "up None", One("Aim"), ("WorldUp", VRCConstraintBase.WorldUpType.None));
            Make("Aim", "axes", One("Aim"), ("AimAxis", Vector3.right), ("UpAxis", Vector3.forward));
            Make("LookAt", "roll", One("LookAt"), ("Roll", 30f));
            Make("LookAt", "up object", One("LookAt"), ("UseUpTransform", true), ("WorldUpTransform", t["Chest"]));

            var frozen = new List<string>();
            foreach (var kind in new[] { "Position", "Parent" })
                foreach (var rebake in new[] { true, false })
                {
                    var name = $"{kind} freeze {(rebake ? "rebake" : "keep")}";
                    Make(kind, $"freeze {(rebake ? "rebake" : "keep")}", One(kind), ("RebakeOffsetsWhenUnfrozen", rebake));
                    frozen.Add(name);
                }

            Transform Chain(Transform parent, string name)
            {
                var p = parent;
                for (int k = 0; k < 4; k++)
                {
                    var go = new GameObject(k == 0 ? name : $"{name}.{k}");
                    go.transform.SetParent(p, false);
                    go.transform.localPosition = k == 0 ? new Vector3(0, 0, 0.1f) : new Vector3(0, -0.1f, 0.02f);
                    p = go.transform;
                }
                var pb = parent.Find(name).gameObject.AddComponent<VRCPhysBone>();
                pb.pull = 0.2f; pb.spring = 0.6f; pb.gravity = 0.3f;
                pb.allowGrabbing = VRCPhysBoneBase.AdvancedBool.False;
                pb.allowPosing = VRCPhysBoneBase.AdvancedBool.False;
                return p;
            }
            var swung = Chain(t["Head"], "Swung");
            Make("Parent", "PhysBone source", new[] { (swung, 1f) });
            var carrier = Make("Parent", "PhysBone carrier", One("Parent"));
            Chain(carrier.transform, "Carried");
            var a = Make("Position", "chain first", One("Position"));
            var b = Make("Rotation", "chain second", new[] { (a.transform, 1f) });
            Make("Parent", "chain third", new[] { (b.transform, 1f) });

            var receivers = new GameObject("Receivers").transform;
            receivers.SetParent(root, false);
            for (int i = 0; i < Receivers.Length; i++)
            {
                var spec = Receivers[i];
                var go = new GameObject(spec.name);
                go.transform.SetParent(receivers, false);
                go.transform.localPosition = OnRow(-RowHalfLength + 0.4f + i * RowSpacing);
                var r = go.AddComponent<VRCContactReceiver>();
                r.shapeType = spec.shape;
                r.radius = spec.shape == ContactBase.ShapeType.Capsule ? 0.08f : 0.1f;
                r.height = spec.shape == ContactBase.ShapeType.Capsule ? 0.3f : 0;
                r.receiverType = spec.type; r.allowSelf = spec.self; r.allowOthers = spec.others; r.localOnly = spec.local;
                r.collisionTags = spec.tags.ToList(); r.minVelocity = spec.minVelocity;
                r.parameter = ReceiverParameter(spec.name);
            }
            var self = new GameObject("SelfSender");
            self.transform.SetParent(root, false);
            self.transform.localPosition = Parked;
            var sender = self.AddComponent<VRCContactSender>();
            sender.shapeType = ContactBase.ShapeType.Sphere; sender.radius = 0.05f; sender.collisionTags = new List<string> { DynamicsTag };

            AssetDatabase.CreateFolder(Dir, "Clips");
            var k = new Clips(Dir + "/Clips");
            var parameters = ScriptableObject.CreateInstance<VRCExpressionParameters>();
            parameters.parameters = new[] { P(SelfPass, VRCExpressionParameters.ValueType.Bool, 0, saved: false) };
            AssetDatabase.CreateAsset(parameters, $"{Dir}/Parameters.asset");
            var fx = new Ctl("DynamicsFX", (SelfPass, AnimatorControllerParameterType.Bool));
            foreach (var spec in Receivers)
                fx.C.AddParameter(ReceiverParameter(spec.name), spec.type == ContactReceiver.ReceiverType.Proximity ? AnimatorControllerParameterType.Float : AnimatorControllerParameterType.Bool);

            var freeze = k.Make("Freeze", c =>
            {
                foreach (var name in frozen)
                    Stepped(c, "Constraints/" + name, ConstraintKinds[name.Split(' ')[0]], "FreezeToWorld", (0, 0), (1, 1), (2.5f, 0), (3, 0));
                foreach (var axis in "xyz")
                    Curve(c, AnimationUtility.CalculateTransformPath(scaleA, root), typeof(Transform), "m_LocalScale." + axis, (0, 1), (2, axis == 'y' ? 0.6f : 1.6f), (4, 1));
            }, loop: true);
            var freezeState = fx.Layer("Freeze").stateMachine.AddState("Play");
            freezeState.writeDefaultValues = false; freezeState.motion = freeze;

            float half = RowHalfLength;
            var pass = k.Make("Self pass", c =>
            {
                Curve(c, "SelfSender", typeof(Transform), "m_LocalPosition.x", (0, -half), (SelfPassSeconds / 2, half), (SelfPassSeconds, -half));
                Const(c, "SelfSender", typeof(Transform), "m_LocalPosition.y", RowHeight);
                Const(c, "SelfSender", typeof(Transform), "m_LocalPosition.z", RowDepth);
            }, loop: true);
            var park = k.Make("Self parked", c => Pos(c, "SelfSender", Parked));
            var sm = fx.Layer("Self").stateMachine;
            var parkState = sm.AddState("Parked"); parkState.writeDefaultValues = false; parkState.motion = park;
            var passState = sm.AddState("Pass"); passState.writeDefaultValues = false; passState.motion = pass;
            sm.defaultState = parkState;
            Go(parkState, passState, 0).AddCondition(AnimatorConditionMode.If, 0, SelfPass);
            Go(passState, parkState, 0).AddCondition(AnimatorConditionMode.IfNot, 0, SelfPass);

            InstallFx(d, parameters, fx);
        }

        public static string ReceiverParameter(string receiver) => "Contact " + receiver;

        public static float SelfPassFrames => SelfPassSeconds * 2 * Harness.Rate;
    }
}
