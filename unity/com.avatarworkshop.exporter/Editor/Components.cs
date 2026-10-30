using System;
using System.Collections.Generic;
using System.Linq;
using UnityEngine;
using UnityEngine.Animations;
using VRC.Dynamics;
using VRC.Dynamics.ManagedTypes;
using VRC.SDK3.Avatars.Components;
using VRC.SDK3.Avatars.ScriptableObjects;
using VRC.SDKBase;

namespace AvatarWorkshop.Exporter
{
    sealed partial class Export
    {
        sealed class Item<T> { public T component; public Obj record; public bool sender; }

        readonly Dictionary<Component, int> physboneIndex = new Dictionary<Component, int>();
        readonly Dictionary<Component, int> constraintIndex = new Dictionary<Component, int>();
        readonly Dictionary<Component, int> senderIndex = new Dictionary<Component, int>();
        readonly Dictionary<Component, int> receiverIndex = new Dictionary<Component, int>();
        readonly Dictionary<VRCPhysBoneColliderBase, int> colliderIndex = new Dictionary<VRCPhysBoneColliderBase, int>();

        IEnumerable<T> All<T>() where T : Component => nodes.SelectMany(t => t.GetComponents<T>());

        Obj Humanoid()
        {
            var animator = avatar.GetComponent<Animator>();
            var bones = new Obj();
            var skeleton = new List<object>();
            var h = new Obj();
            if (animator == null || animator.avatar == null || !animator.avatar.isHuman)
            {
                Drop("non-humanoid avatar");
                h.Add("bones", bones);
                h.Add("skeleton", skeleton);
                foreach (var k in new[] { "armStretch", "legStretch", "upperArmTwist", "lowerArmTwist", "upperLegTwist", "lowerLegTwist", "feetSpacing" }) h.Add(k, 0f);
                h.Add("hasTranslationDoF", false);
                return h;
            }
            var desc = animator.avatar.humanDescription;
            var limits = desc.human.ToDictionary(b => b.humanName, b => b.limit);
            for (int i = 0; i < (int)HumanBodyBones.LastBone; i++)
            {
                var bone = (HumanBodyBones)i;
                if (!(Node(animator.GetBoneTransform(bone)) is int n)) continue;
                var record = new Obj { { "node", n } };
                if (limits.TryGetValue(HumanTrait.BoneName[i], out var limit) && !limit.useDefaultValues)
                    record.Add("limit", new Obj { { "min", Space.S(limit.min) }, { "max", Space.S(limit.max) }, { "center", Space.S(limit.center) }, { "axisLength", limit.axisLength } });
                bones.Add(bone.ToString(), record);
            }
            var byName = new Dictionary<string, Transform>();
            foreach (var t in nodes) if (!byName.ContainsKey(t.name)) byName[t.name] = t;
            int unmatched = 0;
            for (int i = 0; i < desc.skeleton.Length; i++)
            {
                var s = desc.skeleton[i];
                Transform t = i == 0 && !byName.ContainsKey(s.name) ? root : byName.TryGetValue(s.name, out var found) ? found : null;
                if (t == null) { unmatched++; continue; }
                skeleton.Add(new Obj { { "node", nodeIndex[t] }, { "position", Space.P(s.position) }, { "rotation", Space.Q(s.rotation) }, { "scale", Space.S(s.scale) } });
            }
            Drop("humanoid skeleton bone with no node", unmatched);
            h.Add("bones", bones);
            h.Add("skeleton", skeleton);
            h.Add("armStretch", desc.armStretch);
            h.Add("legStretch", desc.legStretch);
            h.Add("upperArmTwist", desc.upperArmTwist);
            h.Add("lowerArmTwist", desc.lowerArmTwist);
            h.Add("upperLegTwist", desc.upperLegTwist);
            h.Add("lowerLegTwist", desc.lowerLegTwist);
            h.Add("feetSpacing", desc.feetSpacing);
            h.Add("hasTranslationDoF", desc.hasTranslationDoF);
            return h;
        }

        static Obj Rotations(VRCAvatarDescriptor.CustomEyeLookSettings.EyeRotations r) =>
            new Obj { { "linked", r.linked }, { "left", Space.Q(r.left) }, { "right", Space.Q(r.right) } };

        static Obj Lids(VRCAvatarDescriptor.CustomEyeLookSettings.EyelidRotations r) =>
            new Obj { { "upper", Rotations(r.upper) }, { "lower", Rotations(r.lower) } };

        Obj Descriptor()
        {
            var d = descriptor;
            Obj lip;
            switch (d.lipSync)
            {
                case VRC_AvatarDescriptor.LipSyncStyle.JawFlapBone when Node(d.lipSyncJawBone) is int jaw:
                    lip = new Obj { { "mode", "jawFlapBone" }, { "bone", jaw }, { "closed", Space.Q(d.lipSyncJawClosed) }, { "open", Space.Q(d.lipSyncJawOpen) } };
                    break;
                case VRC_AvatarDescriptor.LipSyncStyle.JawFlapBlendShape when d.VisemeSkinnedMesh != null && Node(d.VisemeSkinnedMesh.transform) is int mesh:
                    lip = new Obj { { "mode", "jawFlapBlendShape" }, { "mesh", mesh }, { "blendShape", d.MouthOpenBlendShapeName ?? "" } };
                    break;
                case VRC_AvatarDescriptor.LipSyncStyle.VisemeBlendShape when d.VisemeSkinnedMesh != null && Node(d.VisemeSkinnedMesh.transform) is int mesh:
                    var names = Enum.GetNames(typeof(VRC_AvatarDescriptor.Viseme)).Take(15).ToArray();
                    var visemes = new Obj();
                    for (int i = 0; i < 15; i++) visemes.Add(names[i], d.VisemeBlendShapes != null && i < d.VisemeBlendShapes.Length ? d.VisemeBlendShapes[i] ?? "" : "");
                    lip = new Obj { { "mode", "visemeBlendShape" }, { "mesh", mesh }, { "visemes", visemes } };
                    break;
                case VRC_AvatarDescriptor.LipSyncStyle.VisemeParameterOnly:
                    lip = new Obj { { "mode", "visemeParameterOnly" } };
                    break;
                case VRC_AvatarDescriptor.LipSyncStyle.Default:
                    lip = new Obj { { "mode", "default" } };
                    break;
                default:
                    Drop("lip sync " + d.lipSync + " without its mesh or bone");
                    lip = new Obj { { "mode", "default" } };
                    break;
            }
            var e = d.customEyeLookSettings;
            var eyes = new Obj();
            if (d.enableEyeLook)
            {
                var look = new Obj();
                if (Node(e.leftEye) is int l) look.Add("leftEye", l);
                if (Node(e.rightEye) is int r) look.Add("rightEye", r);
                look.Add("straight", Rotations(e.eyesLookingStraight));
                look.Add("up", Rotations(e.eyesLookingUp));
                look.Add("down", Rotations(e.eyesLookingDown));
                look.Add("left", Rotations(e.eyesLookingLeft));
                look.Add("right", Rotations(e.eyesLookingRight));
                eyes.Add("look", look);
            }
            Obj lids = new Obj { { "type", "none" } };
            if (d.enableEyeLook && e.eyelidType == VRCAvatarDescriptor.EyelidType.Bones)
            {
                lids = new Obj { { "type", "bones" } };
                if (Node(e.upperLeftEyelid) is int a) lids.Add("leftUpper", a);
                if (Node(e.upperRightEyelid) is int b) lids.Add("rightUpper", b);
                if (Node(e.lowerLeftEyelid) is int c) lids.Add("leftLower", c);
                if (Node(e.lowerRightEyelid) is int dd) lids.Add("rightLower", dd);
                lids.Add("default", Lids(e.eyelidsDefault));
                lids.Add("closed", Lids(e.eyelidsClosed));
                lids.Add("lookingUp", Lids(e.eyelidsLookingUp));
                lids.Add("lookingDown", Lids(e.eyelidsLookingDown));
            }
            else if (d.enableEyeLook && e.eyelidType == VRCAvatarDescriptor.EyelidType.Blendshapes && e.eyelidsSkinnedMesh != null && e.eyelidsSkinnedMesh.sharedMesh != null && Node(e.eyelidsSkinnedMesh.transform) is int lm)
            {
                var mesh = e.eyelidsSkinnedMesh.sharedMesh;
                lids = new Obj { { "type", "blendShapes" }, { "mesh", lm } };
                var keys = new[] { "blink", "lookingUp", "lookingDown" };
                for (int i = 0; i < 3; i++)
                {
                    int s = e.eyelidsBlendshapes != null && i < e.eyelidsBlendshapes.Length ? e.eyelidsBlendshapes[i] : -1;
                    if (s >= 0 && s < mesh.blendShapeCount) lids.Add(keys[i], mesh.GetBlendShapeName(s));
                }
            }
            eyes.Add("eyelids", lids);
            eyes.Add("eyeMovement", new Obj { { "confidence", e.eyeMovement.confidence }, { "excitement", e.eyeMovement.excitement } });

            var colliders = new[] { d.collider_head, d.collider_torso, d.collider_footR, d.collider_footL, d.collider_handR, d.collider_handL,
                d.collider_fingerIndexL, d.collider_fingerMiddleL, d.collider_fingerRingL, d.collider_fingerLittleL,
                d.collider_fingerIndexR, d.collider_fingerMiddleR, d.collider_fingerRingR, d.collider_fingerLittleR };
            Drop("avatar descriptor collider", colliders.Count(c => c.state != VRCAvatarDescriptor.ColliderConfig.State.Disabled));
            return new Obj { { "viewPosition", Space.P(d.ViewPosition) }, { "lipSync", lip }, { "eyes", eyes } };
        }

        static Obj Curve(AnimationCurve c)
        {
            var keys = c.keys.Select(k => (object)new Obj
            {
                { "time", k.time }, { "value", k.value }, { "inTangent", Slope(k.inTangent) }, { "outTangent", Slope(k.outTangent) },
                { "inWeight", k.inWeight }, { "outWeight", k.outWeight },
                { "weightedMode", k.weightedMode switch { WeightedMode.In => "in", WeightedMode.Out => "out", WeightedMode.Both => "both", _ => "none" } },
            }).ToList();
            return new Obj { { "keys", keys } };
        }

        static object Slope(float t) => float.IsPositiveInfinity(t) ? "Infinity" : float.IsNegativeInfinity(t) ? "-Infinity" : (object)t;

        static Obj Curved(float value, AnimationCurve curve)
        {
            var o = new Obj { { "value", value } };
            if (curve != null && curve.length > 0) o.Add("curve", Curve(curve));
            return o;
        }

        static Obj Permission(VRCPhysBoneBase.AdvancedBool allow, VRCPhysBoneBase.PermissionFilter filter) => allow switch
        {
            VRCPhysBoneBase.AdvancedBool.True => new Obj { { "allowSelf", true }, { "allowOthers", true } },
            VRCPhysBoneBase.AdvancedBool.False => new Obj { { "allowSelf", false }, { "allowOthers", false } },
            _ => new Obj { { "allowSelf", filter.allowSelf }, { "allowOthers", filter.allowOthers } },
        };

        List<object> Colliders()
        {
            var list = new List<object>();
            foreach (var c in All<VRCPhysBoneColliderBase>())
            {
                colliderIndex[c] = list.Count;
                var shape = c.shapeType switch
                {
                    VRCPhysBoneColliderBase.ShapeType.Sphere => new Obj { { "type", "sphere" }, { "radius", c.radius }, { "insideBounds", c.insideBounds } },
                    VRCPhysBoneColliderBase.ShapeType.Capsule => new Obj { { "type", "capsule" }, { "radius", c.radius }, { "height", c.height }, { "insideBounds", c.insideBounds } },
                    _ => new Obj { { "type", "plane" } },
                };
                if (c.globalCollision != VRCPhysBoneBase.AdvancedBool.False) Drop("physbone collider global collision");
                list.Add(new Obj
                {
                    { "node", NodeOr(c.rootTransform, c, "physbone collider root") },
                    { "shape", shape }, { "position", Space.P(c.position) }, { "rotation", Space.Q(c.rotation) }, { "bonesAsSpheres", c.bonesAsSpheres },
                });
            }
            return list;
        }

        List<Item<VRCPhysBoneBase>> PhysBones()
        {
            var list = new List<Item<VRCPhysBoneBase>>();
            foreach (var p in All<VRCPhysBoneBase>())
            {
                physboneIndex[p] = list.Count;
                var ignored = p.ignoreTransforms.Where(t => t != null).Select(Node).Where(n => n.HasValue).Select(n => (object)n.Value).ToList();
                Drop("physbone ignored transform outside the avatar", p.ignoreTransforms.Count(t => t != null && Node(t) == null));
                Obj limit = p.limitType switch
                {
                    VRCPhysBoneBase.LimitType.Angle => new Obj { { "type", "angle" }, { "maxAngleX", Curved(p.maxAngleX, p.maxAngleXCurve) }, { "rotation", LimitRotation(p) } },
                    VRCPhysBoneBase.LimitType.Hinge => new Obj { { "type", "hinge" }, { "maxAngleX", Curved(p.maxAngleX, p.maxAngleXCurve) }, { "rotation", LimitRotation(p) } },
                    VRCPhysBoneBase.LimitType.Polar => new Obj { { "type", "polar" }, { "maxAngleX", Curved(p.maxAngleX, p.maxAngleXCurve) }, { "maxAngleZ", Curved(p.maxAngleZ, p.maxAngleZCurve) }, { "rotation", LimitRotation(p) } },
                    _ => new Obj { { "type", "none" } },
                };
                if (p.ignoreOtherPhysBones) Drop("physbone ignoreOtherPhysBones");
                if (p.staticFreezeAxis != Vector3.zero) Drop("physbone staticFreezeAxis");
                var record = new Obj
                {
                    { "node", nodeIndex[p.transform] }, { "enabled", p.enabled },
                    { "version", p.version == VRCPhysBoneBase.Version.Version_1_0 ? "1.0" : "1.1" },
                    { "root", NodeOr(p.rootTransform, p, "physbone root") },
                    { "ignoreTransforms", ignored }, { "endpointPosition", Space.P(p.endpointPosition) },
                    { "multiChildType", p.multiChildType.ToString().ToLowerInvariant() },
                    { "integrationType", p.integrationType == VRCPhysBoneBase.IntegrationType.Advanced ? "advanced" : "simplified" },
                    { "pull", Curved(p.pull, p.pullCurve) }, { "spring", Curved(p.spring, p.springCurve) }, { "stiffness", Curved(p.stiffness, p.stiffnessCurve) },
                    { "gravity", Curved(p.gravity, p.gravityCurve) }, { "gravityFalloff", Curved(p.gravityFalloff, p.gravityFalloffCurve) },
                    { "immobileType", p.immobileType == VRCPhysBoneBase.ImmobileType.World ? "world" : "allMotion" }, { "immobile", Curved(p.immobile, p.immobileCurve) },
                    { "limit", limit }, { "radius", Curved(p.radius, p.radiusCurve) },
                    { "maxStretch", Curved(p.maxStretch, p.maxStretchCurve) }, { "stretchMotion", Curved(p.stretchMotion, p.stretchMotionCurve) }, { "maxSquish", Curved(p.maxSquish, p.maxSquishCurve) },
                };
                int outside = 0;
                var colliders = new List<object>();
                foreach (var c in p.colliders)
                {
                    if (c != null && colliderIndex.TryGetValue(c, out int ci)) colliders.Add(ci);
                    else if (c != null) outside++;
                }
                Drop("physbone collider outside the avatar", outside);
                record.Add("colliders", colliders);
                record.Add("collision", Permission(p.allowCollision, p.collisionFilter));
                record.Add("grabbing", Permission(p.allowGrabbing, p.grabFilter));
                record.Add("posing", Permission(p.allowPosing, p.poseFilter));
                record.Add("grabMovement", p.grabMovement);
                record.Add("snapToHand", p.snapToHand);
                if (!string.IsNullOrEmpty(p.parameter)) record.Add("parameter", p.parameter);
                record.Add("isAnimated", p.isAnimated);
                record.Add("resetWhenDisabled", p.resetWhenDisabled);
                list.Add(new Item<VRCPhysBoneBase> { component = p, record = record });
            }
            return list;
        }

        static Obj LimitRotation(VRCPhysBoneBase p)
        {
            // Each curve scales its axis of the value, so mirroring the value mirrors the product.
            var o = new Obj { { "value", Space.E(p.limitRotation) } };
            if (p.limitRotationXCurve != null && p.limitRotationXCurve.length > 0) o.Add("xCurve", Curve(p.limitRotationXCurve));
            if (p.limitRotationYCurve != null && p.limitRotationYCurve.length > 0) o.Add("yCurve", Curve(p.limitRotationYCurve));
            if (p.limitRotationZCurve != null && p.limitRotationZCurve.length > 0) o.Add("zCurve", Curve(p.limitRotationZCurve));
            return o;
        }

        public static AnimationCurve Negated(AnimationCurve c) =>
            new AnimationCurve(c.keys.Select(k => new Keyframe(k.time, -k.value, -k.inTangent, -k.outTangent, k.inWeight, k.outWeight) { weightedMode = k.weightedMode }).ToArray())
            { preWrapMode = c.preWrapMode, postWrapMode = c.postWrapMode };

        List<Item<ContactBase>> Contacts()
        {
            var list = new List<Item<ContactBase>>();
            foreach (var c in All<ContactBase>())
            {
                bool sender = c is ContactSender;
                if (!sender && !(c is ContactReceiver)) { Drop(c.GetType().Name); continue; }
                if (c.shapeType == ContactBase.ShapeType.Box) { Drop("box contact"); continue; }
                if (c.contentTypes != DynamicsUsageFlags.Everything) Drop("contact content type filter");
                var shape = c.shapeType == ContactBase.ShapeType.Capsule
                    ? new Obj { { "type", "capsule" }, { "radius", c.radius }, { "height", c.height } }
                    : new Obj { { "type", "sphere" }, { "radius", c.radius } };
                var record = new Obj
                {
                    { "node", NodeOr(c.rootTransform, c, "contact root") },
                    { "enabled", c.enabled }, { "shape", shape }, { "position", Space.P(c.position) }, { "rotation", Space.Q(c.rotation) },
                    { "collisionTags", c.collisionTags.Where(t => !string.IsNullOrEmpty(t)).ToArray() },
                };
                if (sender)
                {
                    if (c.localOnly) Drop("local-only contact sender");
                    senderIndex[c] = list.Count(e => e.sender);
                }
                else
                {
                    var r = (ContactReceiver)c;
                    if (r.useFaceProximity) Drop("contact receiver face proximity");
                    record.Add("allowSelf", r.allowSelf);
                    record.Add("allowOthers", r.allowOthers);
                    record.Add("localOnly", r.localOnly);
                    record.Add("receiverType", r.receiverType switch
                    {
                        ContactReceiver.ReceiverType.OnEnter => new Obj { { "type", "onEnter" }, { "minVelocity", r.minVelocity } },
                        ContactReceiver.ReceiverType.Proximity => new Obj { { "type", "proximity" } },
                        _ => new Obj { { "type", "constant" } },
                    });
                    record.Add("parameter", r.parameter ?? "");
                    receiverIndex[c] = list.Count(e => !e.sender);
                }
                list.Add(new Item<ContactBase> { component = c, record = record, sender = sender });
            }
            return list;
        }

        static Obj Channel(float[] atRest, float[] offset, bool x, bool y, bool z) =>
            new Obj { { "atRest", atRest }, { "offset", offset }, { "affects", new[] { x, y, z } } };

        static bool Has(Axis axes, Axis a) => (axes & a) != 0;

        object SourceTarget(Transform t)
        {
            if (Node(t) is int n) return new Obj { { "node", n } };
            Drop("constraint source outside the avatar, kept as a fixed world matrix");
            return new Obj { { "world", Space.M(AvatarFrame * t.localToWorldMatrix) } };
        }

        Obj WorldUpNode(Transform t, string kind)
        {
            if (Node(t) is int n) return new Obj { { "type", kind }, { "node", n } };
            Drop("constraint world-up object outside the avatar");
            return new Obj { { "type", "sceneUp" } };
        }

        List<Item<Component>> Constraints()
        {
            var list = new List<Item<Component>>();
            var zero = new float[3];
            var one = new[] { 1f, 1f, 1f };
            foreach (var c in All<VRCConstraintBase>())
            {
                var sources = new List<object>();
                foreach (var s in c.Sources)
                {
                    if (s.SourceTransform == null) { Drop("constraint source with no transform"); continue; }
                    sources.Add(new Obj { { "source", SourceTarget(s.SourceTransform) }, { "weight", s.Weight }, { "positionOffset", Space.P(s.ParentPositionOffset) }, { "rotationOffset", Space.E(s.ParentRotationOffset) } });
                }
                Obj kind;
                switch (c)
                {
                    case VRCParentConstraintBase p:
                        kind = new Obj { { "type", "parent" }, { "position", Channel(Space.P(p.PositionAtRest), zero, p.AffectsPositionX, p.AffectsPositionY, p.AffectsPositionZ) }, { "rotation", Channel(Space.E(p.RotationAtRest), zero, p.AffectsRotationX, p.AffectsRotationY, p.AffectsRotationZ) } };
                        break;
                    case VRCPositionConstraintBase p:
                        kind = new Obj { { "type", "position" }, { "position", Channel(Space.P(p.PositionAtRest), Space.P(p.PositionOffset), p.AffectsPositionX, p.AffectsPositionY, p.AffectsPositionZ) } };
                        break;
                    case VRCRotationConstraintBase p:
                        kind = new Obj { { "type", "rotation" }, { "rotation", Channel(Space.E(p.RotationAtRest), Space.E(p.RotationOffset), p.AffectsRotationX, p.AffectsRotationY, p.AffectsRotationZ) } };
                        break;
                    case VRCAimConstraintBase p:
                        Obj up = p.WorldUp switch
                        {
                            VRCConstraintBase.WorldUpType.ObjectUp => WorldUpNode(p.WorldUpTransform, "objectUp"),
                            VRCConstraintBase.WorldUpType.ObjectRotationUp => Node(p.WorldUpTransform) is int wn ? new Obj { { "type", "objectRotationUp" }, { "node", wn }, { "vector", Space.P(p.WorldUpVector) } } : WorldUpNode(null, ""),
                            VRCConstraintBase.WorldUpType.Vector => new Obj { { "type", "vector" }, { "vector", Space.P(p.WorldUpVector) } },
                            VRCConstraintBase.WorldUpType.None => new Obj { { "type", "none" } },
                            _ => new Obj { { "type", "sceneUp" } },
                        };
                        kind = new Obj { { "type", "aim" }, { "rotation", Channel(Space.E(p.RotationAtRest), Space.E(p.RotationOffset), p.AffectsRotationX, p.AffectsRotationY, p.AffectsRotationZ) }, { "aimAxis", Space.P(p.AimAxis) }, { "upAxis", Space.P(p.UpAxis) }, { "worldUp", up } };
                        break;
                    case VRCLookAtConstraintBase p:
                        kind = new Obj { { "type", "lookAt" }, { "rotation", Channel(Space.E(p.RotationAtRest), Space.E(p.RotationOffset), true, true, true) }, { "roll", -p.Roll } };
                        if (p.UseUpTransform && Node(p.WorldUpTransform) is int lu) kind.Add("worldUpNode", lu);
                        break;
                    case VRCScaleConstraintBase p:
                        kind = new Obj { { "type", "scale" }, { "scale", Channel(Space.S(p.ScaleAtRest), Space.S(p.ScaleOffset), p.AffectsScaleX, p.AffectsScaleY, p.AffectsScaleZ) } };
                        break;
                    default:
                        Drop(c.GetType().Name);
                        continue;
                }
                constraintIndex[c] = list.Count;
                list.Add(new Item<Component> { component = c, record = new Obj
                {
                    { "node", nodeIndex[c.transform] }, { "enabled", c.enabled }, { "isActive", c.IsActive },
                    { "target", NodeOr(c.TargetTransform, c, "constraint target") },
                    { "weight", c.GlobalWeight }, { "sources", sources }, { "solveInLocalSpace", c.SolveInLocalSpace },
                    { "freezeToWorld", c.FreezeToWorld }, { "rebakeOffsetsWhenUnfrozen", c.RebakeOffsetsWhenUnfrozen }, { "kind", kind },
                } });
            }
            foreach (var t in nodes)
            {
                foreach (var c in t.GetComponents<IConstraint>())
                {
                    var component = (Behaviour)c;
                    var sources = new List<object>();
                    var list_ = new List<ConstraintSource>();
                    c.GetSources(list_);
                    for (int i = 0; i < list_.Count; i++)
                    {
                        var s = list_[i];
                        if (s.sourceTransform == null) { Drop("constraint source with no transform"); continue; }
                        var po = c is ParentConstraint pc ? Space.P(pc.GetTranslationOffset(i)) : zero;
                        var ro = c is ParentConstraint pr ? Space.E(pr.GetRotationOffset(i)) : zero;
                        sources.Add(new Obj { { "source", SourceTarget(s.sourceTransform) }, { "weight", s.weight }, { "positionOffset", po }, { "rotationOffset", ro } });
                    }
                    Obj kind;
                    switch (c)
                    {
                        case ParentConstraint p:
                            kind = new Obj { { "type", "parent" }, { "position", Channel(Space.P(p.translationAtRest), zero, Has(p.translationAxis, Axis.X), Has(p.translationAxis, Axis.Y), Has(p.translationAxis, Axis.Z)) }, { "rotation", Channel(Space.E(p.rotationAtRest), zero, Has(p.rotationAxis, Axis.X), Has(p.rotationAxis, Axis.Y), Has(p.rotationAxis, Axis.Z)) } };
                            break;
                        case PositionConstraint p:
                            kind = new Obj { { "type", "position" }, { "position", Channel(Space.P(p.translationAtRest), Space.P(p.translationOffset), Has(p.translationAxis, Axis.X), Has(p.translationAxis, Axis.Y), Has(p.translationAxis, Axis.Z)) } };
                            break;
                        case RotationConstraint p:
                            kind = new Obj { { "type", "rotation" }, { "rotation", Channel(Space.E(p.rotationAtRest), Space.E(p.rotationOffset), Has(p.rotationAxis, Axis.X), Has(p.rotationAxis, Axis.Y), Has(p.rotationAxis, Axis.Z)) } };
                            break;
                        case AimConstraint p:
                            Obj up = p.worldUpType switch
                            {
                                AimConstraint.WorldUpType.ObjectUp => WorldUpNode(p.worldUpObject, "objectUp"),
                                AimConstraint.WorldUpType.ObjectRotationUp => Node(p.worldUpObject) is int wn ? new Obj { { "type", "objectRotationUp" }, { "node", wn }, { "vector", Space.P(p.worldUpVector) } } : WorldUpNode(null, ""),
                                AimConstraint.WorldUpType.Vector => new Obj { { "type", "vector" }, { "vector", Space.P(p.worldUpVector) } },
                                AimConstraint.WorldUpType.None => new Obj { { "type", "none" } },
                                _ => new Obj { { "type", "sceneUp" } },
                            };
                            kind = new Obj { { "type", "aim" }, { "rotation", Channel(Space.E(p.rotationAtRest), Space.E(p.rotationOffset), Has(p.rotationAxis, Axis.X), Has(p.rotationAxis, Axis.Y), Has(p.rotationAxis, Axis.Z)) }, { "aimAxis", Space.P(p.aimVector) }, { "upAxis", Space.P(p.upVector) }, { "worldUp", up } };
                            break;
                        case LookAtConstraint p:
                            kind = new Obj { { "type", "lookAt" }, { "rotation", Channel(Space.E(p.rotationAtRest), Space.E(p.rotationOffset), true, true, true) }, { "roll", -p.roll } };
                            if (p.useUpObject && Node(p.worldUpObject) is int lu) kind.Add("worldUpNode", lu);
                            break;
                        case ScaleConstraint p:
                            kind = new Obj { { "type", "scale" }, { "scale", Channel(Space.S(p.scaleAtRest), Space.S(p.scaleOffset), Has(p.scalingAxis, Axis.X), Has(p.scalingAxis, Axis.Y), Has(p.scalingAxis, Axis.Z)) } };
                            break;
                        default:
                            Drop(c.GetType().Name);
                            continue;
                    }
                    constraintIndex[component] = list.Count;
                    list.Add(new Item<Component> { component = component, record = new Obj
                    {
                        { "node", nodeIndex[t] }, { "enabled", component.enabled }, { "isActive", c.constraintActive }, { "target", nodeIndex[t] },
                        { "weight", c.weight }, { "sources", sources }, { "solveInLocalSpace", false }, { "freezeToWorld", false },
                        { "rebakeOffsetsWhenUnfrozen", false }, { "kind", kind },
                    } });
                }
            }
            return list;
        }

        // Enabled states as the default states leave them.
        void FillEnabled(List<Item<VRCPhysBoneBase>> physbones, List<Item<Component>> constraints, List<Item<ContactBase>> contacts)
        {
            foreach (var p in physbones) p.record["enabled"] = p.component.enabled;
            foreach (var c in constraints) c.record["enabled"] = ((Behaviour)c.component).enabled;
            foreach (var c in contacts) c.record["enabled"] = c.component.enabled;
        }

        List<object> Parameters()
        {
            var list = new List<object>();
            var ps = descriptor.customExpressions ? descriptor.expressionParameters : null;
            if (ps == null || ps.parameters == null) return list;
            foreach (var p in ps.parameters)
            {
                if (string.IsNullOrEmpty(p.name)) continue;
                object value = p.valueType switch
                {
                    VRCExpressionParameters.ValueType.Int => new Obj { { "int", (int)p.defaultValue } },
                    VRCExpressionParameters.ValueType.Bool => new Obj { { "bool", p.defaultValue != 0 } },
                    _ => (object)new Obj { { "float", p.defaultValue } },
                };
                list.Add(new Obj { { "name", p.name }, { "default", value }, { "saved", p.saved }, { "synced", p.networkSynced } });
            }
            return list;
        }

        Obj Menu()
        {
            var menu = descriptor.customExpressions ? descriptor.expressionsMenu : null;
            return Menu(menu, new HashSet<VRCExpressionsMenu>());
        }

        Obj Menu(VRCExpressionsMenu menu, HashSet<VRCExpressionsMenu> open)
        {
            var controls = new List<object>();
            if (menu != null)
            {
                open.Add(menu);
                foreach (var c in menu.controls) controls.Add(Control(c, open));
                open.Remove(menu);
            }
            return new Obj { { "controls", controls } };
        }

        static string ParamName(VRCExpressionsMenu.Control.Parameter p) => p == null || string.IsNullOrEmpty(p.name) ? null : p.name;

        object Label(VRCExpressionsMenu.Control.Label l)
        {
            var o = new Obj { { "name", l.name ?? "" } };
            if (l.icon != null) o.Add("icon", Texture(l.icon));
            return o;
        }

        Obj Control(VRCExpressionsMenu.Control c, HashSet<VRCExpressionsMenu> open)
        {
            var kind = new Obj();
            string type = c.type switch
            {
                VRCExpressionsMenu.Control.ControlType.Button => "button",
                VRCExpressionsMenu.Control.ControlType.Toggle => "toggle",
                VRCExpressionsMenu.Control.ControlType.SubMenu => "subMenu",
                VRCExpressionsMenu.Control.ControlType.TwoAxisPuppet => "twoAxisPuppet",
                VRCExpressionsMenu.Control.ControlType.FourAxisPuppet => "fourAxisPuppet",
                _ => "radialPuppet",
            };
            kind.Add("type", type);
            if (ParamName(c.parameter) is string p) kind.Add("parameter", p);
            kind.Add("value", c.value);
            string Sub(int i) => c.subParameters != null && i < c.subParameters.Length ? ParamName(c.subParameters[i]) : null;
            object[] Labels() => Enumerable.Range(0, 4).Select(i => c.labels != null && i < c.labels.Length ? Label(c.labels[i]) : new Obj { { "name", "" } }).ToArray();
            switch (type)
            {
                case "subMenu":
                    if (c.subMenu != null && open.Contains(c.subMenu)) Drop("menu that contains itself");
                    else if (c.subMenu != null) kind.Add("subMenu", Menu(c.subMenu, open));
                    break;
                case "twoAxisPuppet":
                    kind.Add("subParameters", new object[] { Sub(0), Sub(1) });
                    kind.Add("labels", Labels());
                    break;
                case "fourAxisPuppet":
                    kind.Add("subParameters", new object[] { Sub(0), Sub(1), Sub(2), Sub(3) });
                    kind.Add("labels", Labels());
                    break;
                case "radialPuppet":
                    if (Sub(0) is string s) kind.Add("subParameter", s);
                    break;
            }
            var o = new Obj { { "name", c.name ?? "" } };
            if (c.icon != null) o.Add("icon", Texture(c.icon));
            o.Add("kind", kind);
            return o;
        }
    }
}
