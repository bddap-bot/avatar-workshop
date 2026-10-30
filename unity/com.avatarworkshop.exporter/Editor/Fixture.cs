using System.Collections.Generic;
using System.IO;
using System.Linq;
using Unity.Collections;
using UnityEditor;
using UnityEditor.Animations;
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.Animations;
using VRC.Dynamics;
using VRC.SDK3.Avatars.Components;
using VRC.SDK3.Avatars.ScriptableObjects;
using VRC.SDK3.Dynamics.Constraint.Components;
using VRC.SDK3.Dynamics.Contact.Components;
using VRC.SDK3.Dynamics.PhysBone.Components;
using VRC.SDKBase;

namespace AvatarWorkshop.Exporter
{
    // The model's synthetic fixture as a Unity avatar: a small humanoid that fills every AW_avatar section
    // and carries a few things the model reports as unsupported.
    public static class Fixture
    {
        const string Dir = "Assets/AvatarWorkshopFixture";

        static T Save<T>(T asset, string name) where T : Object
        {
            AssetDatabase.CreateAsset(asset, $"{Dir}/{name}");
            return asset;
        }

        static Transform Bone(string name, Transform parent, float x, float y, float z)
        {
            var t = new GameObject(name).transform;
            t.SetParent(parent, false);
            t.localPosition = new Vector3(x, y, z);
            return t;
        }

        public static GameObject Build()
        {
            if (AssetDatabase.IsValidFolder(Dir)) AssetDatabase.DeleteAsset(Dir);
            AssetDatabase.CreateFolder("Assets", "AvatarWorkshopFixture");
            var scene = EditorSceneManager.NewScene(NewSceneSetup.EmptyScene, NewSceneMode.Single);

            var avatar = new GameObject("Fixture");
            var root = avatar.transform;
            var armature = Bone("Armature", root, 0, 0, 0);
            var hips = Bone("Hips", armature, 0, 1, 0);
            var spine = Bone("Spine", hips, 0, 0.15f, 0);
            var chest = Bone("Chest", spine, 0, 0.15f, 0);
            var neck = Bone("Neck", chest, 0, 0.15f, 0);
            var head = Bone("Head", neck, 0, 0.1f, 0);
            head.localRotation = Quaternion.Euler(5, 10, 0);
            var jaw = Bone("Jaw", head, 0, -0.02f, 0.05f);
            var leftEye = Bone("LeftEye", head, -0.03f, 0.05f, 0.08f);
            var rightEye = Bone("RightEye", head, 0.03f, 0.05f, 0.08f);
            var hair = Bone("Hair", head, 0, 0.15f, -0.05f);
            var hairTip = Bone("HairTip", hair, 0, 0, -0.2f);
            var bones = new Dictionary<HumanBodyBones, Transform>
            {
                [HumanBodyBones.Hips] = hips, [HumanBodyBones.Spine] = spine, [HumanBodyBones.Chest] = chest,
                [HumanBodyBones.Neck] = neck, [HumanBodyBones.Head] = head, [HumanBodyBones.Jaw] = jaw,
                [HumanBodyBones.LeftEye] = leftEye, [HumanBodyBones.RightEye] = rightEye,
            };
            foreach (var (side, s) in new[] { ("Left", -1f), ("Right", 1f) })
            {
                var upperLeg = Bone(side + "UpperLeg", hips, 0.1f * s, -0.05f, 0);
                var lowerLeg = Bone(side + "LowerLeg", upperLeg, 0, -0.45f, 0);
                var foot = Bone(side + "Foot", lowerLeg, 0, -0.45f, 0);
                var toes = Bone(side + "Toes", foot, 0, -0.05f, 0.1f);
                var shoulder = Bone(side + "Shoulder", chest, 0.05f * s, 0.1f, 0);
                var upperArm = Bone(side + "UpperArm", shoulder, 0.1f * s, 0, 0);
                upperArm.localRotation = Quaternion.Euler(0, 0, -40 * s);
                var lowerArm = Bone(side + "LowerArm", upperArm, 0.25f * s, 0, 0);
                var hand = Bone(side + "Hand", lowerArm, 0.25f * s, 0, 0);
                foreach (var (part, bone) in new[] { ("UpperLeg", upperLeg), ("LowerLeg", lowerLeg), ("Foot", foot), ("Toes", toes), ("Shoulder", shoulder), ("UpperArm", upperArm), ("LowerArm", lowerArm), ("Hand", hand) })
                    bones[(HumanBodyBones)System.Enum.Parse(typeof(HumanBodyBones), side + part)] = bone;
            }
            var leftHand = bones[HumanBodyBones.LeftHand];
            var rightHand = bones[HumanBodyBones.RightHand];

            var checker = new Texture2D(8, 8, TextureFormat.RGBA32, false);
            for (int y = 0; y < 8; y++) for (int x = 0; x < 8; x++) checker.SetPixel(x, y, (x + y) % 2 == 0 ? Color.white : new Color(0.8f, 0.3f, 0.2f, 1));
            File.WriteAllBytes(Dir + "/Checker.png", checker.EncodeToPNG());
            AssetDatabase.ImportAsset(Dir + "/Checker.png");
            var checkerAsset = AssetDatabase.LoadAssetAtPath<Texture2D>(Dir + "/Checker.png");
            var noise = new Texture2D(4, 4, TextureFormat.RGBA32, false);
            for (int i = 0; i < 16; i++) noise.SetPixel(i % 4, i / 4, new Color(i / 16f, 1 - i / 16f, 0.5f, 1));
            noise.Apply();
            Save(noise, "Noise.asset");

            var fur = new Material(Shader.Find("Standard")) { name = "Fur" };
            fur.SetTexture("_MainTex", checkerAsset);
            fur.SetTextureScale("_MainTex", new Vector2(2, 2));
            fur.SetColor("_Color", new Color(1, 0.9f, 0.8f, 1));
            Save(fur, "Fur.mat");
            var face = Save(new Material(Shader.Find("VRChat/Mobile/Toon Lit")) { name = "Face" }, "Face.mat");
            var glass = Save(new Material(Shader.Find("Unlit/Transparent")) { name = "Glass" }, "Glass.mat");
            var swap = new Material(Shader.Find("Standard")) { name = "Swap" };
            swap.EnableKeyword("_EMISSION");
            swap.SetColor("_EmissionColor", new Color(0.2f, 0.4f, 1, 1));
            swap.SetTexture("_DetailAlbedoMap", noise);
            Save(swap, "Swap.mat");

            var body = new GameObject("Body");
            body.transform.SetParent(root, false);
            var skinBones = new[] { hips, spine, chest, head, leftHand, rightHand };
            var mesh = BodyMesh(root, skinBones);
            Save(mesh, "Body.asset");
            var smr = body.AddComponent<SkinnedMeshRenderer>();
            smr.sharedMesh = mesh;
            smr.bones = skinBones;
            smr.rootBone = hips;
            smr.sharedMaterials = new[] { fur, face, glass };
            smr.SetBlendShapeWeight(mesh.GetBlendShapeIndex("Smile"), 25);

            var prop = GameObject.CreatePrimitive(PrimitiveType.Cube);
            prop.name = "Prop";
            prop.transform.SetParent(rightHand, false);
            prop.transform.localScale = Vector3.one * 0.1f;
            prop.GetComponent<MeshRenderer>().sharedMaterial = swap;
            var sparkles = new GameObject("Sparkles");
            sparkles.transform.SetParent(head, false);
            sparkles.AddComponent<ParticleSystem>();
            var speaker = new GameObject("Speaker");
            speaker.transform.SetParent(chest, false);
            speaker.AddComponent<AudioSource>();

            var desc = new HumanDescription
            {
                human = bones.Select(kv => new HumanBone
                {
                    boneName = kv.Value.name, humanName = HumanTrait.BoneName[(int)kv.Key],
                    limit = kv.Key == HumanBodyBones.Head
                        ? new HumanLimit { useDefaultValues = false, min = new Vector3(-40, -30, -20), max = new Vector3(40, 30, 20), center = Vector3.zero, axisLength = 0.1f }
                        : new HumanLimit { useDefaultValues = true },
                }).ToArray(),
                skeleton = root.GetComponentsInChildren<Transform>().Select(t => new SkeletonBone { name = t.name, position = t.localPosition, rotation = t.localRotation, scale = t.localScale }).ToArray(),
                armStretch = 0.05f, legStretch = 0.05f, upperArmTwist = 0.5f, lowerArmTwist = 0.5f, upperLegTwist = 0.5f, lowerLegTwist = 0.5f, feetSpacing = 0, hasTranslationDoF = false,
            };
            var human = AvatarBuilder.BuildHumanAvatar(avatar, desc);
            if (!human.isValid || !human.isHuman) throw new System.Exception("the fixture skeleton is not a valid humanoid");
            human.name = "Fixture Avatar";
            Save(human, "Fixture Avatar.asset");
            var animator = avatar.AddComponent<Animator>();
            animator.avatar = human;

            var pipeline = avatar.GetComponent<VRC.Core.PipelineManager>() ?? avatar.AddComponent<VRC.Core.PipelineManager>();
            var d = avatar.AddComponent<VRCAvatarDescriptor>();
            d.ViewPosition = new Vector3(0, 1.62f, 0.08f);
            d.lipSync = VRC_AvatarDescriptor.LipSyncStyle.VisemeBlendShape;
            d.VisemeSkinnedMesh = smr;
            d.VisemeBlendShapes = Visemes.Select(v => "vrc.v_" + v).ToArray();
            d.enableEyeLook = true;
            var e = d.customEyeLookSettings;
            e.leftEye = leftEye;
            e.rightEye = rightEye;
            var straight = new VRCAvatarDescriptor.CustomEyeLookSettings.EyeRotations { linked = true, left = Quaternion.identity, right = Quaternion.identity };
            e.eyesLookingStraight = straight;
            e.eyesLookingUp = new VRCAvatarDescriptor.CustomEyeLookSettings.EyeRotations { linked = true, left = Quaternion.Euler(-15, 0, 0), right = Quaternion.Euler(-15, 0, 0) };
            e.eyesLookingDown = new VRCAvatarDescriptor.CustomEyeLookSettings.EyeRotations { linked = true, left = Quaternion.Euler(12, 0, 0), right = Quaternion.Euler(12, 0, 0) };
            e.eyesLookingLeft = new VRCAvatarDescriptor.CustomEyeLookSettings.EyeRotations { linked = false, left = Quaternion.Euler(0, -20, 0), right = Quaternion.Euler(0, -15, 0) };
            e.eyesLookingRight = new VRCAvatarDescriptor.CustomEyeLookSettings.EyeRotations { linked = false, left = Quaternion.Euler(0, 15, 0), right = Quaternion.Euler(0, 20, 0) };
            e.eyelidType = VRCAvatarDescriptor.EyelidType.Blendshapes;
            e.eyelidsSkinnedMesh = smr;
            e.eyelidsBlendshapes = new[] { mesh.GetBlendShapeIndex("Blink"), -1, -1 };
            e.eyeMovement = new VRCAvatarDescriptor.CustomEyeLookSettings.EyeMovements { confidence = 0.6f, excitement = 0.4f };
            d.customEyeLookSettings = e;

            var headCollider = head.gameObject.AddComponent<VRCPhysBoneCollider>();
            headCollider.radius = 0.09f;
            headCollider.position = new Vector3(0, 0.08f, 0.01f);
            var hipCollider = hips.gameObject.AddComponent<VRCPhysBoneCollider>();
            hipCollider.shapeType = VRCPhysBoneColliderBase.ShapeType.Capsule;
            hipCollider.radius = 0.15f;
            hipCollider.height = 0.5f;
            hipCollider.insideBounds = true;
            hipCollider.rotation = Quaternion.Euler(0, 0, 90);
            var floor = root.gameObject.AddComponent<VRCPhysBoneCollider>();
            floor.shapeType = VRCPhysBoneColliderBase.ShapeType.Plane;

            var pb = hair.gameObject.AddComponent<VRCPhysBone>();
            pb.rootTransform = hair;
            pb.integrationType = VRCPhysBoneBase.IntegrationType.Advanced;
            pb.pull = 0.3f;
            pb.pullCurve = new AnimationCurve(new Keyframe(0, 1), new Keyframe(1, 0.5f, -0.5f, -0.5f));
            pb.spring = 0.4f;
            pb.gravity = 0.2f;
            pb.gravityFalloff = 0.5f;
            pb.immobileType = VRCPhysBoneBase.ImmobileType.World;
            pb.immobile = 0.3f;
            pb.limitType = VRCPhysBoneBase.LimitType.Polar;
            pb.maxAngleX = 60;
            pb.maxAngleZ = 30;
            pb.limitRotation = new Vector3(10, 20, 0);
            pb.radius = 0.02f;
            pb.maxStretch = 0.2f;
            pb.maxSquish = 0.1f;
            pb.colliders = new List<VRCPhysBoneColliderBase> { headCollider, hipCollider, floor };
            pb.allowGrabbing = VRCPhysBoneBase.AdvancedBool.Other;
            pb.grabFilter = new VRCPhysBoneBase.PermissionFilter(true, false);
            pb.allowPosing = VRCPhysBoneBase.AdvancedBool.False;
            pb.parameter = "Hair";
            pb.ignoreTransforms = new List<Transform> { hairTip };

            var sender = leftHand.gameObject.AddComponent<VRCContactSender>();
            sender.radius = 0.05f;
            sender.collisionTags = new List<string> { "Hand" };
            var boop = head.gameObject.AddComponent<VRCContactReceiver>();
            boop.shapeType = ContactBase.ShapeType.Capsule;
            boop.radius = 0.05f;
            boop.height = 0.2f;
            boop.collisionTags = new List<string> { "Hand" };
            boop.receiverType = ContactReceiver.ReceiverType.Proximity;
            boop.parameter = "Boop";
            var tap = chest.gameObject.AddComponent<VRCContactReceiver>();
            tap.receiverType = ContactReceiver.ReceiverType.OnEnter;
            tap.parameter = "Tap";
            tap.allowSelf = false;
            tap.localOnly = true;

            var parent = prop.AddComponent<VRCParentConstraint>();
            parent.Sources.Add(new VRCConstraintSource(rightHand, 1, new Vector3(0, 0.05f, 0), new Vector3(0, 90, 0)));
            parent.IsActive = true;
            var position = sparkles.AddComponent<VRCPositionConstraint>();
            position.Sources.Add(new VRCConstraintSource(head, 0.5f));
            position.PositionOffset = new Vector3(0.1f, 0, 0);
            position.AffectsPositionY = false;
            position.IsActive = true;
            var rotation = jaw.gameObject.AddComponent<VRCRotationConstraint>();
            rotation.Sources.Add(new VRCConstraintSource(head, 1));
            rotation.RotationOffset = new Vector3(0, 0, 15);
            rotation.SolveInLocalSpace = true;
            var aim = leftEye.gameObject.AddComponent<VRCAimConstraint>();
            aim.Sources.Add(new VRCConstraintSource(prop.transform, 1));
            aim.WorldUp = VRCConstraintBase.WorldUpType.ObjectUp;
            aim.WorldUpTransform = head;
            aim.IsActive = true;
            var look = rightEye.gameObject.AddComponent<VRCLookAtConstraint>();
            look.Sources.Add(new VRCConstraintSource(prop.transform, 1));
            look.UseUpTransform = true;
            look.WorldUpTransform = head;
            look.Roll = 5;
            look.IsActive = true;
            var scale = hairTip.gameObject.AddComponent<VRCScaleConstraint>();
            scale.Sources.Add(new VRCConstraintSource(hair, 1));
            scale.FreezeToWorld = true;
            var unity = speaker.AddComponent<ParentConstraint>();
            unity.AddSource(new ConstraintSource { sourceTransform = head, weight = 1 });
            unity.SetTranslationOffset(0, new Vector3(0, 0.1f, 0));
            unity.constraintActive = true;

            var parameters = ScriptableObject.CreateInstance<VRCExpressionParameters>();
            parameters.parameters = new[]
            {
                new VRCExpressionParameters.Parameter { name = "Prop", valueType = VRCExpressionParameters.ValueType.Bool, defaultValue = 1, saved = true, networkSynced = true },
                new VRCExpressionParameters.Parameter { name = "Outfit", valueType = VRCExpressionParameters.ValueType.Int, defaultValue = 2, saved = true, networkSynced = true },
                new VRCExpressionParameters.Parameter { name = "Sweep", valueType = VRCExpressionParameters.ValueType.Float, defaultValue = 0.5f, saved = false, networkSynced = true },
                new VRCExpressionParameters.Parameter { name = "PuppetX", valueType = VRCExpressionParameters.ValueType.Float, defaultValue = 0, saved = false, networkSynced = false },
                new VRCExpressionParameters.Parameter { name = "PuppetY", valueType = VRCExpressionParameters.ValueType.Float, defaultValue = 0, saved = false, networkSynced = false },
            };
            Save(parameters, "Parameters.asset");
            d.customExpressions = true;
            d.expressionParameters = parameters;
            d.expressionsMenu = Menus(noise);

            var mask = new AvatarMask();
            foreach (var part in System.Enum.GetValues(typeof(AvatarMaskBodyPart)).Cast<AvatarMaskBodyPart>().Where(p => p != AvatarMaskBodyPart.LastBodyPart))
                mask.SetHumanoidBodyPartActive(part, part == AvatarMaskBodyPart.Head || part == AvatarMaskBodyPart.LeftFingers || part == AvatarMaskBodyPart.RightFingers);
            mask.transformCount = 2;
            mask.SetTransformPath(0, "Armature/Hips");
            mask.SetTransformActive(0, true);
            mask.SetTransformPath(1, "Armature/Hips/Spine/Chest/Neck/Head/Hair");
            mask.SetTransformActive(1, false);
            Save(mask, "Hands.mask");

            var clips = Clips(swap);
            var fx = Fx(clips, mask);
            var gesture = AnimatorController.CreateAnimatorControllerAtPath(Dir + "/Gesture.controller");
            var idle = gesture.layers[0].stateMachine.AddState("Idle");
            idle.motion = clips["Wave"];
            VRCAvatarDescriptor.CustomAnimLayer Layer(VRCAvatarDescriptor.AnimLayerType type, RuntimeAnimatorController c = null, AvatarMask m = null) =>
                new VRCAvatarDescriptor.CustomAnimLayer { type = type, isEnabled = true, isDefault = c == null, animatorController = c, mask = m };
            d.customizeAnimationLayers = true;
            d.baseAnimationLayers = new[]
            {
                Layer(VRCAvatarDescriptor.AnimLayerType.Base), Layer(VRCAvatarDescriptor.AnimLayerType.Additive),
                Layer(VRCAvatarDescriptor.AnimLayerType.Gesture, gesture, mask), Layer(VRCAvatarDescriptor.AnimLayerType.Action),
                Layer(VRCAvatarDescriptor.AnimLayerType.FX, fx),
            };
            d.specialAnimationLayers = new[] { Layer(VRCAvatarDescriptor.AnimLayerType.Sitting), Layer(VRCAvatarDescriptor.AnimLayerType.TPose), Layer(VRCAvatarDescriptor.AnimLayerType.IKPose) };

            AssetDatabase.SaveAssets();
            EditorSceneManager.SaveScene(scene, Dir + "/Fixture.unity");
            return avatar;
        }

        static readonly string[] Visemes = { "sil", "PP", "FF", "TH", "DD", "kk", "CH", "SS", "nn", "RR", "aa", "E", "ih", "oh", "ou" };

        static Mesh BodyMesh(Transform root, Transform[] bones)
        {
            var mesh = new Mesh { name = "Body" };
            var v = new List<Vector3>();
            for (int i = 0; i < 8; i++) v.Add(new Vector3((i & 1) == 0 ? -0.15f : 0.15f, (i & 2) == 0 ? 0.9f : 1.7f, (i & 4) == 0 ? -0.1f : 0.1f));
            mesh.SetVertices(v);
            mesh.SetUVs(0, v.Select(p => new Vector2(p.x + 0.5f, p.y - 0.9f)).ToList());
            mesh.SetUVs(1, v.Select(p => new Vector2(p.z + 0.5f, p.y - 0.9f)).ToList());
            mesh.SetUVs(2, v.Select(p => new Vector3(p.x, p.y, p.z)).ToList());
            mesh.SetColors(v.Select(p => new Color(p.x + 0.5f, p.y - 0.9f, p.z + 0.5f, 1)).ToList());
            mesh.subMeshCount = 2;
            mesh.SetTriangles(new[] { 0, 2, 1, 1, 2, 3, 4, 5, 6, 5, 7, 6, 0, 1, 4, 1, 5, 4 }, 0);
            mesh.SetTriangles(new[] { 2, 6, 3, 3, 6, 7, 0, 4, 2, 2, 4, 6, 1, 3, 5, 3, 7, 5 }, 1);
            mesh.RecalculateNormals();
            mesh.RecalculateTangents();
            var perVertex = new byte[8];
            var weights = new List<BoneWeight1>();
            for (int i = 0; i < 8; i++)
            {
                if (i == 7)
                {
                    perVertex[i] = 5;
                    foreach (var (b, w) in new[] { (3, 0.4f), (2, 0.2f), (1, 0.2f), (4, 0.1f), (5, 0.1f) }) weights.Add(new BoneWeight1 { boneIndex = b, weight = w });
                }
                else
                {
                    perVertex[i] = 2;
                    weights.Add(new BoneWeight1 { boneIndex = (i & 2) == 0 ? 0 : 3, weight = 0.75f });
                    weights.Add(new BoneWeight1 { boneIndex = 1, weight = 0.25f });
                }
            }
            using (var pv = new NativeArray<byte>(perVertex, Allocator.Temp))
            using (var bw = new NativeArray<BoneWeight1>(weights.ToArray(), Allocator.Temp))
                mesh.SetBoneWeights(pv, bw);
            mesh.bindposes = bones.Select(b => b.worldToLocalMatrix * root.localToWorldMatrix).ToArray();
            var zero = new Vector3[8];
            Vector3[] Delta(int vertex, Vector3 d) { var a = new Vector3[8]; a[vertex] = d; return a; }
            for (int i = 0; i < Visemes.Length; i++) mesh.AddBlendShapeFrame("vrc.v_" + Visemes[i], 100, Delta(2 + i % 2, new Vector3(0, -0.01f * (i + 1), 0)), zero, zero);
            mesh.AddBlendShapeFrame("Blink", 100, Delta(6, new Vector3(0, -0.02f, 0)), Delta(6, new Vector3(0, 0.1f, 0)), zero);
            mesh.AddBlendShapeFrame("Smile", 100, Delta(7, new Vector3(0.01f, 0.01f, 0)), zero, zero);
            mesh.AddBlendShapeFrame("Frown", 100, Delta(7, new Vector3(-0.01f, -0.01f, 0)), zero, zero);
            mesh.AddBlendShapeFrame("Puff", 50, Delta(4, new Vector3(0, 0, 0.02f)), zero, zero);
            mesh.AddBlendShapeFrame("Puff", 100, Delta(4, new Vector3(0, 0, 0.05f)), zero, zero);
            mesh.RecalculateBounds();
            return mesh;
        }

        static VRCExpressionsMenu Menus(Texture2D icon)
        {
            VRCExpressionsMenu.Control.Parameter P(string name) => new VRCExpressionsMenu.Control.Parameter { name = name };
            VRCExpressionsMenu.Control.Label L(string name) => new VRCExpressionsMenu.Control.Label { name = name };
            var main = ScriptableObject.CreateInstance<VRCExpressionsMenu>();
            var more = ScriptableObject.CreateInstance<VRCExpressionsMenu>();
            main.controls = new List<VRCExpressionsMenu.Control>
            {
                new VRCExpressionsMenu.Control { name = "Prop", type = VRCExpressionsMenu.Control.ControlType.Toggle, parameter = P("Prop"), value = 1, icon = icon },
                new VRCExpressionsMenu.Control { name = "Outfit", type = VRCExpressionsMenu.Control.ControlType.Button, parameter = P("Outfit"), value = 3 },
                new VRCExpressionsMenu.Control { name = "More", type = VRCExpressionsMenu.Control.ControlType.SubMenu, subMenu = more },
                new VRCExpressionsMenu.Control { name = "Puppet", type = VRCExpressionsMenu.Control.ControlType.TwoAxisPuppet, subParameters = new[] { P("PuppetX"), P("PuppetY") }, labels = new[] { L("Up"), L("Right"), L("Down"), L("Left") } },
                new VRCExpressionsMenu.Control { name = "Four", type = VRCExpressionsMenu.Control.ControlType.FourAxisPuppet, subParameters = new[] { P("PuppetY"), P("PuppetX"), P("Sweep"), P("") }, labels = new[] { L("N"), new VRCExpressionsMenu.Control.Label { name = "E", icon = icon }, L("S"), L("W") } },
            };
            more.controls = new List<VRCExpressionsMenu.Control>
            {
                new VRCExpressionsMenu.Control { name = "Sweep", type = VRCExpressionsMenu.Control.ControlType.RadialPuppet, subParameters = new[] { P("Sweep") } },
                new VRCExpressionsMenu.Control { name = "Back", type = VRCExpressionsMenu.Control.ControlType.SubMenu, subMenu = main },
            };
            Save(main, "Menu.asset");
            Save(more, "More.asset");
            return main;
        }

        static AnimationCurve Step(float value) => new AnimationCurve(new Keyframe(0, value, float.PositiveInfinity, float.PositiveInfinity));

        static Dictionary<string, AnimationClip> Clips(Material swap)
        {
            var off = new AnimationClip { name = "PropOff" };
            off.SetCurve("Armature/Hips/Spine/Chest/RightShoulder/RightUpperArm/RightLowerArm/RightHand/Prop", typeof(GameObject), "m_IsActive", Step(0));
            off.SetCurve("Body", typeof(SkinnedMeshRenderer), "m_Enabled", Step(1));
            off.SetCurve("Body", typeof(SkinnedMeshRenderer), "blendShape.Smile", AnimationCurve.Linear(0, 0, 1, 100));
            off.SetCurve("Body", typeof(SkinnedMeshRenderer), "material._Color.r", AnimationCurve.Constant(0, 1, 0.5f));
            const string hair = "Armature/Hips/Spine/Chest/Neck/Head/Hair";
            off.SetCurve(hair, typeof(Transform), "m_LocalPosition.x", AnimationCurve.Linear(0, 0.01f, 1, 0.03f));
            off.SetCurve(hair, typeof(Transform), "localEulerAnglesRaw.y", AnimationCurve.Linear(0, 0, 1, 30));
            off.SetCurve(hair, typeof(Transform), "m_LocalScale.z", AnimationCurve.Constant(0, 1, 1.2f));
            off.SetCurve("", typeof(Animator), "Head Nod Down-Up", AnimationCurve.Constant(0, 1, 0.3f));
            off.SetCurve("", typeof(Animator), "Smile", AnimationCurve.Constant(0, 1, 1));
            off.SetCurve(hair, typeof(VRCPhysBone), "pull", AnimationCurve.Constant(0, 1, 0.5f));
            off.SetCurve("Armature/Hips/Spine/Chest/RightShoulder/RightUpperArm/RightLowerArm/RightHand/Prop", typeof(VRCParentConstraint), "GlobalWeight", AnimationCurve.Constant(0, 1, 0.5f));
            off.SetCurve("Armature/Hips/Spine/Chest/LeftShoulder/LeftUpperArm/LeftLowerArm/LeftHand", typeof(VRCContactSender), "radius", AnimationCurve.Constant(0, 1, 0.08f));
            off.SetCurve("Armature/Hips/Spine/Chest/Neck/Head", typeof(VRCContactReceiver), "radius", AnimationCurve.Constant(0, 1, 0.08f));
            off.SetCurve("Armature/Hips/Spine/Chest/Neck/Head/Sparkles", typeof(ParticleSystem), "InitialModule.startSize.scalar", AnimationCurve.Constant(0, 1, 2));
            off.SetCurve("Nowhere", typeof(GameObject), "m_IsActive", Step(1));
            AnimationUtility.SetObjectReferenceCurve(off, EditorCurveBinding.PPtrCurve("Body", typeof(SkinnedMeshRenderer), "m_Materials.Array.data[1]"),
                new[] { new ObjectReferenceKeyframe { time = 0, value = swap } });
            var on = new AnimationClip { name = "PropOn" };
            on.SetCurve("Armature/Hips/Spine/Chest/RightShoulder/RightUpperArm/RightLowerArm/RightHand/Prop", typeof(GameObject), "m_IsActive", Step(1));
            var wave = new AnimationClip { name = "Wave" };
            var rot = Quaternion.Euler(0, 0, 20);
            const string hand = "Armature/Hips/Spine/Chest/LeftShoulder/LeftUpperArm/LeftLowerArm/LeftHand";
            wave.SetCurve(hand, typeof(Transform), "m_LocalRotation.x", AnimationCurve.Constant(0, 1, rot.x));
            wave.SetCurve(hand, typeof(Transform), "m_LocalRotation.y", AnimationCurve.Constant(0, 1, rot.y));
            wave.SetCurve(hand, typeof(Transform), "m_LocalRotation.z", AnimationCurve.Constant(0, 1, rot.z));
            wave.SetCurve(hand, typeof(Transform), "m_LocalRotation.w", AnimationCurve.Constant(0, 1, rot.w));
            var settings = AnimationUtility.GetAnimationClipSettings(wave);
            settings.loopTime = true;
            AnimationUtility.SetAnimationClipSettings(wave, settings);
            var smile = new AnimationClip { name = "Smile" };
            smile.SetCurve("Body", typeof(SkinnedMeshRenderer), "blendShape.Smile", AnimationCurve.Constant(0, 0, 100));
            var frown = new AnimationClip { name = "Frown" };
            frown.SetCurve("Body", typeof(SkinnedMeshRenderer), "blendShape.Frown", AnimationCurve.Constant(0, 0, 100));
            var clips = new Dictionary<string, AnimationClip> { ["PropOff"] = off, ["PropOn"] = on, ["Wave"] = wave, ["Smile"] = smile, ["Frown"] = frown };
            foreach (var kv in clips) Save(kv.Value, kv.Key + ".anim");
            return clips;
        }

        static AnimatorController Fx(Dictionary<string, AnimationClip> clips, AvatarMask mask)
        {
            var c = AnimatorController.CreateAnimatorControllerAtPath(Dir + "/FX.controller");
            c.AddParameter("Prop", AnimatorControllerParameterType.Bool);
            c.AddParameter("Outfit", AnimatorControllerParameterType.Int);
            foreach (var f in new[] { "Sweep", "PuppetX", "PuppetY", "Smile", "Frown", "Offset", "Hair", "Boop", "Tap" }) c.AddParameter(f, AnimatorControllerParameterType.Float);
            c.AddParameter(new AnimatorControllerParameter { name = "Speed", type = AnimatorControllerParameterType.Float, defaultFloat = 1 });
            c.AddParameter("Mirror", AnimatorControllerParameterType.Bool);
            c.AddParameter("Reset", AnimatorControllerParameterType.Trigger);
            c.AddParameter("IsLocal", AnimatorControllerParameterType.Bool);

            var layers = c.layers;
            layers[0].name = "Toggle";
            c.layers = layers;
            var sm = c.layers[0].stateMachine;
            var off = sm.AddState("PropOff");
            off.motion = clips["PropOff"];
            off.writeDefaultValues = true;
            var on = sm.AddState("PropOn");
            on.tag = "on";
            on.speedParameterActive = true;
            on.speedParameter = "Speed";
            on.cycleOffsetParameterActive = true;
            on.cycleOffsetParameter = "Offset";
            on.mirrorParameterActive = true;
            on.mirrorParameter = "Mirror";
            on.iKOnFeet = true;
            var tree = new BlendTree { name = "Sweep", blendType = BlendTreeType.Simple1D, blendParameter = "Sweep", useAutomaticThresholds = false };
            AssetDatabase.AddObjectToAsset(tree, c);
            tree.AddChild(clips["PropOn"], 0);
            tree.AddChild(clips["Wave"], 1);
            on.motion = tree;
            sm.defaultState = off;
            var t1 = off.AddTransition(on);
            t1.AddCondition(AnimatorConditionMode.If, 0, "Prop");
            t1.hasExitTime = true;
            t1.exitTime = 0.75f;
            t1.duration = 0.25f;
            t1.interruptionSource = TransitionInterruptionSource.SourceThenDestination;
            var t2 = on.AddExitTransition();
            t2.AddCondition(AnimatorConditionMode.IfNot, 0, "Prop");
            t2.AddCondition(AnimatorConditionMode.Greater, 0.1f, "Sweep");
            t2.AddCondition(AnimatorConditionMode.Equals, 2, "Outfit");
            t2.hasFixedDuration = false;
            t2.duration = 0.5f;
            var entry = sm.AddEntryTransition(on);
            entry.AddCondition(AnimatorConditionMode.If, 0, "Prop");

            var blends = sm.AddStateMachine("Blends");
            BlendTree Tree2D(string name, BlendTreeType type)
            {
                var b = new BlendTree { name = name, blendType = type, blendParameter = "PuppetX", blendParameterY = "PuppetY" };
                AssetDatabase.AddObjectToAsset(b, c);
                b.AddChild(clips["Smile"], new Vector2(0, 1));
                b.AddChild(clips["Frown"], new Vector2(0, -1));
                b.AddChild(clips["Wave"], new Vector2(1, 0));
                return b;
            }
            var directional = blends.AddState("Directional");
            directional.motion = Tree2D("Simple", BlendTreeType.SimpleDirectional2D);
            blends.AddState("Freeform").motion = Tree2D("Freeform", BlendTreeType.FreeformDirectional2D);
            blends.AddState("Cartesian").motion = Tree2D("Cartesian", BlendTreeType.FreeformCartesian2D);
            var direct = new BlendTree { name = "Direct", blendType = BlendTreeType.Direct };
            AssetDatabase.AddObjectToAsset(direct, c);
            direct.AddChild(clips["Smile"]);
            direct.AddChild(clips["Frown"]);
            var children = direct.children;
            children[0].directBlendParameter = "Smile";
            children[1].directBlendParameter = "Frown";
            direct.children = children;
            var so = new SerializedObject(direct);
            so.FindProperty("m_NormalizedBlendValues").boolValue = true;
            so.ApplyModifiedPropertiesWithoutUndo();
            blends.AddState("Direct").motion = direct;
            var any = sm.AddAnyStateTransition(blends);
            any.AddCondition(AnimatorConditionMode.If, 0, "Reset");
            any.canTransitionToSelf = true;
            any.interruptionSource = TransitionInterruptionSource.Destination;
            var back = sm.AddStateMachineTransition(blends, off);
            back.mute = true;

            var driver = off.AddStateMachineBehaviour<VRCAvatarParameterDriver>();
            driver.localOnly = true;
            driver.parameters = new List<VRC_AvatarParameterDriver.Parameter>
            {
                new VRC_AvatarParameterDriver.Parameter { type = VRC_AvatarParameterDriver.ChangeType.Set, name = "Outfit", value = 1 },
                new VRC_AvatarParameterDriver.Parameter { type = VRC_AvatarParameterDriver.ChangeType.Add, name = "Sweep", value = 0.1f },
                new VRC_AvatarParameterDriver.Parameter { type = VRC_AvatarParameterDriver.ChangeType.Random, name = "PuppetX", valueMin = -1, valueMax = 1, chance = 0.5f },
                new VRC_AvatarParameterDriver.Parameter { type = VRC_AvatarParameterDriver.ChangeType.Copy, source = "Sweep", name = "PuppetY", convertRange = true, sourceMin = 0, sourceMax = 1, destMin = -1, destMax = 1 },
            };
            var tracking = on.AddStateMachineBehaviour<VRCAnimatorTrackingControl>();
            tracking.trackingHead = VRC_AnimatorTrackingControl.TrackingType.Animation;
            tracking.trackingEyes = VRC_AnimatorTrackingControl.TrackingType.Tracking;
            on.AddStateMachineBehaviour<VRCAnimatorLocomotionControl>().disableLocomotion = true;
            var playable = directional.AddStateMachineBehaviour<VRCPlayableLayerControl>();
            playable.layer = VRC_PlayableLayerControl.BlendableLayer.Action;
            playable.goalWeight = 1;
            playable.blendDuration = 0.5f;
            var layerControl = directional.AddStateMachineBehaviour<VRCAnimatorLayerControl>();
            layerControl.playable = VRC_AnimatorLayerControl.BlendableLayer.FX;
            layerControl.layer = 1;
            layerControl.goalWeight = 0.5f;
            var pose = directional.AddStateMachineBehaviour<VRCAnimatorTemporaryPoseSpace>();
            pose.enterPoseSpace = true;
            pose.delayTime = 0.2f;
            directional.AddStateMachineBehaviour<VRCAnimatorPlayAudio>();
            blends.AddStateMachineBehaviour<VRCAnimatorLocomotionControl>();

            c.AddLayer(new AnimatorControllerLayer
            {
                name = "Synced", defaultWeight = 0.5f, blendingMode = AnimatorLayerBlendingMode.Additive, avatarMask = mask, iKPass = true,
                syncedLayerIndex = 0, syncedLayerAffectsTiming = true, stateMachine = new AnimatorStateMachine { name = "Synced" },
            });
            var all = c.layers;
            all[1].SetOverrideMotion(off, clips["Wave"]);
            c.layers = all;
            AssetDatabase.AddObjectToAsset(all[1].stateMachine, c);
            return c;
        }
    }
}
