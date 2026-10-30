using System;
using System.Collections.Generic;
using System.Linq;
using UnityEditor;
using UnityEditor.Animations;
using UnityEditor.SceneManagement;
using UnityEngine;
using VRC.Dynamics;
using VRC.SDK3.Avatars.Components;
using VRC.SDK3.Avatars.ScriptableObjects;
using VRC.SDK3.Dynamics.Constraint.Components;
using VRC.SDK3.Dynamics.Contact.Components;
using VRC.SDK3.Dynamics.PhysBone.Components;
using VRC.SDKBase;
using ExprParam = VRC.SDK3.Avatars.ScriptableObjects.VRCExpressionParameters.Parameter;
using Control = VRC.SDK3.Avatars.ScriptableObjects.VRCExpressionsMenu.Control;

namespace AvatarWorkshop.Reference
{
    public static class Fixture
    {
        public const string Dir = "Assets/AWFixture";
        public const string ScenePath = Dir + "/Fixture.unity";
        public const string RootName = "AWFixture";

        static readonly string[] Visemes = { "vrc.v_sil", "vrc.v_pp", "vrc.v_ff", "vrc.v_th", "vrc.v_dd", "vrc.v_kk", "vrc.v_ch", "vrc.v_ss", "vrc.v_nn", "vrc.v_rr", "vrc.v_aa", "vrc.v_e", "vrc.v_ih", "vrc.v_oh", "vrc.v_ou" };
        static readonly string[] Faces = { "Smile", "Fist", "Open", "Point", "Victory", "Rock", "Gun", "Thumbs", "Wave", "Pout", "TailAngle", "LeanUp", "LeanRight", "LeanDown", "LeanLeft" };

        class Bone
        {
            public string Name; public string Parent; public Vector3 Local; public float Width;
        }

        static readonly List<Bone> Skeleton = new List<Bone>();

        static void B(string name, string parent, float x, float y, float z, float width)
        {
            Skeleton.Add(new Bone { Name = name, Parent = parent, Local = new Vector3(x, y, z), Width = width });
        }

        static void DefineSkeleton()
        {
            Skeleton.Clear();
            B("Armature", null, 0, 0, 0, 0);
            B("Hips", "Armature", 0, 0.95f, 0, 0.16f);
            B("Spine", "Hips", 0, 0.1f, 0, 0.14f);
            B("Chest", "Spine", 0, 0.15f, 0, 0.16f);
            B("Neck", "Chest", 0, 0.2f, 0, 0.06f);
            B("Head", "Neck", 0, 0.1f, 0, 0.18f);
            B("HeadTop", "Head", 0, 0.22f, 0, 0);
            foreach (var s in new[] { ("Left", -1f), ("Right", 1f) })
            {
                var (side, sx) = s;
                B(side + "Shoulder", "Chest", sx * 0.05f, 0.15f, 0, 0.06f);
                B(side + "UpperArm", side + "Shoulder", sx * 0.1f, 0, 0, 0.06f);
                B(side + "LowerArm", side + "UpperArm", sx * 0.26f, 0, 0, 0.05f);
                B(side + "Hand", side + "LowerArm", sx * 0.24f, 0, 0, 0.05f);
                B(side + "HandTip", side + "Hand", sx * 0.1f, 0, 0, 0);
                B(side + "UpperLeg", "Hips", sx * 0.1f, -0.05f, 0, 0.08f);
                B(side + "LowerLeg", side + "UpperLeg", 0, -0.42f, 0, 0.07f);
                B(side + "Foot", side + "LowerLeg", 0, -0.42f, 0, 0.06f);
                B(side + "Toes", side + "Foot", 0, -0.04f, 0.12f, 0);
            }
            foreach (var s in new[] { ("EarL", -1f), ("EarR", 1f) })
            {
                var (n, sx) = s;
                B(n + ".0", "Head", sx * 0.07f, 0.18f, 0, 0.03f);
                B(n + ".1", n + ".0", sx * 0.02f, 0.06f, 0, 0.025f);
                B(n + ".2", n + ".1", sx * 0.01f, 0.06f, 0, 0.02f);
                B(n + ".3", n + ".2", 0, 0.05f, 0, 0);
            }
            B("Hair", "Head", 0, 0.1f, -0.1f, 0.03f);
            foreach (var (n, x) in new[] { ("A", -0.06f), ("B", 0f), ("C", 0.06f) })
            {
                B("Hair" + n + ".0", "Hair", x, -0.02f, -0.02f, 0.025f);
                B("Hair" + n + ".1", "Hair" + n + ".0", 0, -0.08f, -0.01f, 0.02f);
                B("Hair" + n + ".2", "Hair" + n + ".1", 0, -0.08f, 0, 0);
            }
            B("Tail.0", "Hips", 0, -0.02f, -0.1f, 0.04f);
            B("Tail.1", "Tail.0", 0, -0.05f, -0.12f, 0.035f);
            B("Tail.2", "Tail.1", 0, -0.08f, -0.1f, 0.03f);
            B("Tail.3", "Tail.2", 0, -0.1f, -0.06f, 0.025f);
            B("Tail.4", "Tail.3", 0, -0.1f, 0, 0);
            B("Skirt", "Hips", 0, -0.02f, 0, 0.02f);
            for (int i = 0; i < 6; i++)
            {
                float a = i * Mathf.PI / 3f;
                var d = new Vector3(Mathf.Sin(a), 0, Mathf.Cos(a));
                B($"Skirt{i}.0", "Skirt", d.x * 0.15f, 0, d.z * 0.13f, 0.03f);
                B($"Skirt{i}.1", $"Skirt{i}.0", d.x * 0.04f, -0.12f, d.z * 0.04f, 0.025f);
                B($"Skirt{i}.2", $"Skirt{i}.1", d.x * 0.03f, -0.12f, d.z * 0.03f, 0);
            }
            B("Antenna", "Head", 0, 0.22f, 0.05f, 0.02f);
            B("Antenna.0", "Antenna", -0.03f, 0.04f, 0, 0.015f);
            B("Antenna.1", "Antenna", 0.03f, 0.04f, 0, 0.015f);
            B("Antenna.0.0", "Antenna.0", 0, 0.05f, 0, 0);
            B("Antenna.1.0", "Antenna.1", 0, 0.05f, 0, 0);
            B("ProxReceiver", "Hips", -0.5f, 0.0f, 0.12f, 0);
        }

        static readonly Dictionary<string, string> Human = new Dictionary<string, string>
        {
            {"Hips","Hips"},{"Spine","Spine"},{"Chest","Chest"},{"Neck","Neck"},{"Head","Head"},
            {"LeftShoulder","LeftShoulder"},{"LeftUpperArm","LeftUpperArm"},{"LeftLowerArm","LeftLowerArm"},{"LeftHand","LeftHand"},
            {"RightShoulder","RightShoulder"},{"RightUpperArm","RightUpperArm"},{"RightLowerArm","RightLowerArm"},{"RightHand","RightHand"},
            {"LeftUpperLeg","LeftUpperLeg"},{"LeftLowerLeg","LeftLowerLeg"},{"LeftFoot","LeftFoot"},{"LeftToes","LeftToes"},
            {"RightUpperLeg","RightUpperLeg"},{"RightLowerLeg","RightLowerLeg"},{"RightFoot","RightFoot"},{"RightToes","RightToes"},
        };

        public static void Build()
        {
            if (AssetDatabase.IsValidFolder(Dir)) AssetDatabase.DeleteAsset(Dir);
            AssetDatabase.CreateFolder("Assets", "AWFixture");
            var scene = EditorSceneManager.NewScene(NewSceneSetup.EmptyScene, NewSceneMode.Single);
            DefineSkeleton();

            var root = new GameObject(RootName);
            var t = new Dictionary<string, Transform>();
            foreach (var b in Skeleton)
            {
                var go = new GameObject(b.Name);
                go.transform.SetParent(b.Parent == null ? root.transform : t[b.Parent], false);
                go.transform.localPosition = b.Local;
                t[b.Name] = go.transform;
            }

            var mats = new Dictionary<string, Material>();
            foreach (var (n, c) in new[] { ("Skin", new Color(0.9f, 0.75f, 0.65f)), ("Cloth", new Color(0.2f, 0.35f, 0.8f)), ("ClothAlt", new Color(0.8f, 0.3f, 0.2f)), ("Hat", new Color(0.2f, 0.7f, 0.3f)), ("Orb", new Color(0.9f, 0.85f, 0.2f)) })
            {
                var m = new Material(Shader.Find("Standard")) { name = n, color = c };
                AssetDatabase.CreateAsset(m, $"{Dir}/{n}.mat");
                mats[n] = m;
            }

            var body = BodyMesh(t, root.transform);
            AssetDatabase.CreateAsset(body, $"{Dir}/Body.asset");
            var bodyGo = new GameObject("Body");
            bodyGo.transform.SetParent(root.transform, false);
            var smr = bodyGo.AddComponent<SkinnedMeshRenderer>();
            smr.sharedMesh = body;
            smr.bones = Skeleton.Select(b => t[b.Name]).ToArray();
            smr.rootBone = t["Hips"];
            smr.sharedMaterials = new[] { mats["Skin"], mats["Cloth"] };
            smr.localBounds = new Bounds(new Vector3(0, -0.1f, 0), new Vector3(2, 2.2f, 1));

            var hatMesh = BoxMesh("Hat", new Vector3(0, 0.25f, 0), new Vector3(0.22f, 0.08f, 0.22f), t, "Head", root.transform);
            AssetDatabase.CreateAsset(hatMesh, $"{Dir}/Hat.asset");
            var hatGo = new GameObject("Hat");
            hatGo.transform.SetParent(root.transform, false);
            var hat = hatGo.AddComponent<SkinnedMeshRenderer>();
            hat.sharedMesh = hatMesh;
            hat.bones = Skeleton.Select(b => t[b.Name]).ToArray();
            hat.rootBone = t["Head"];
            hat.sharedMaterials = new[] { mats["Hat"] };
            hatGo.SetActive(false);

            var animator = root.AddComponent<Animator>();
            animator.avatar = HumanAvatar(root, t);
            animator.applyRootMotion = false;

            Orbs(root.transform, t, mats["Orb"]);
            PhysBones(t);
            Contacts(t);

            var parameters = Parameters();
            var menu = Menu(parameters);
            var fx = FxController();

            var d = root.AddComponent<VRCAvatarDescriptor>();
            d.ViewPosition = new Vector3(0, 1.62f, 0.08f);
            d.lipSync = VRC_AvatarDescriptor.LipSyncStyle.VisemeBlendShape;
            d.VisemeSkinnedMesh = smr;
            d.VisemeBlendShapes = Visemes;
            d.customExpressions = true;
            d.expressionParameters = parameters;
            d.expressionsMenu = menu;
            d.customizeAnimationLayers = true;
            d.baseAnimationLayers = new[]
            {
                Default(VRCAvatarDescriptor.AnimLayerType.Base),
                Default(VRCAvatarDescriptor.AnimLayerType.Additive),
                Default(VRCAvatarDescriptor.AnimLayerType.Gesture),
                Default(VRCAvatarDescriptor.AnimLayerType.Action),
                new VRCAvatarDescriptor.CustomAnimLayer { type = VRCAvatarDescriptor.AnimLayerType.FX, animatorController = fx, isDefault = false, isEnabled = true },
            };
            d.specialAnimationLayers = new[]
            {
                Default(VRCAvatarDescriptor.AnimLayerType.Sitting),
                Default(VRCAvatarDescriptor.AnimLayerType.TPose),
                Default(VRCAvatarDescriptor.AnimLayerType.IKPose),
            };

            AssetDatabase.SaveAssets();
            EditorSceneManager.SaveScene(scene, ScenePath);
            AssetDatabase.SaveAssets();
            AssetDatabase.Refresh();
        }

        static VRCAvatarDescriptor.CustomAnimLayer Default(VRCAvatarDescriptor.AnimLayerType type) =>
            new VRCAvatarDescriptor.CustomAnimLayer { type = type, isDefault = true, isEnabled = true };

        class MeshBuilder
        {
            public readonly List<Vector3> V = new List<Vector3>();
            public readonly List<Vector3> N = new List<Vector3>();
            public readonly List<BoneWeight> W = new List<BoneWeight>();
            public readonly List<int>[] Sub = { new List<int>(), new List<int>() };
            public readonly Dictionary<string, (int start, int count)> Boxes = new Dictionary<string, (int, int)>();

            public void Box(string key, Vector3 a, Vector3 b, float width, int bone, int sub)
            {
                int start = V.Count;
                var axis = b - a;
                var up = Mathf.Abs(Vector3.Dot(axis.normalized, Vector3.up)) > 0.9f ? Vector3.forward : Vector3.up;
                var u = Vector3.Cross(axis, up).normalized * width * 0.5f;
                var w = Vector3.Cross(axis, u).normalized * width * 0.5f;
                Vector3[] c =
                {
                    a - u - w, a + u - w, a + u + w, a - u + w,
                    b - u - w, b + u - w, b + u + w, b - u + w,
                };
                int[][] faces = { new[] { 0, 1, 2, 3 }, new[] { 7, 6, 5, 4 }, new[] { 0, 4, 5, 1 }, new[] { 1, 5, 6, 2 }, new[] { 2, 6, 7, 3 }, new[] { 3, 7, 4, 0 } };
                foreach (var f in faces)
                {
                    var n = Vector3.Cross(c[f[1]] - c[f[0]], c[f[2]] - c[f[0]]).normalized;
                    int i0 = V.Count;
                    foreach (var k in f)
                    {
                        V.Add(c[k]);
                        N.Add(n);
                        W.Add(new BoneWeight { boneIndex0 = bone, weight0 = 1 });
                    }
                    Sub[sub].AddRange(new[] { i0, i0 + 2, i0 + 1, i0, i0 + 3, i0 + 2 });
                }
                Boxes[key] = (start, V.Count - start);
            }
        }

        static int Index(string name) => Skeleton.FindIndex(b => b.Name == name);

        static Mesh BodyMesh(Dictionary<string, Transform> t, Transform root)
        {
            var m = new MeshBuilder();
            foreach (var b in Skeleton)
            {
                if (b.Width <= 0) continue;
                var kids = Skeleton.Where(c => c.Parent == b.Name).ToList();
                var a = root.InverseTransformPoint(t[b.Name].position);
                if (b.Name == "Head")
                {
                    m.Box("Head", a, a + new Vector3(0, 0.22f, 0), b.Width, Index("Head"), 0);
                    continue;
                }
                if (b.Name == "Hips") { m.Box("Hips", a + new Vector3(0, -0.05f, 0), a + new Vector3(0, 0.1f, 0), b.Width, Index("Hips"), 1); continue; }
                if (b.Name == "Hair" || b.Name == "Skirt" || b.Name == "Antenna") continue;
                foreach (var k in kids.Take(b.Name == "Chest" ? 0 : 1))
                {
                    var e = root.InverseTransformPoint(t[k.Name].position);
                    int sub = b.Name.Contains("Leg") || b.Name.StartsWith("Skirt") || b.Name.StartsWith("Tail") ? 1 : 0;
                    m.Box(b.Name, a, e, b.Width, Index(b.Name), sub);
                }
                if (b.Name == "Chest") m.Box("Chest", a, a + new Vector3(0, 0.2f, 0), b.Width * 1.6f, Index("Chest"), 1);
            }
            var mesh = new Mesh { name = "Body" };
            mesh.SetVertices(m.V);
            mesh.SetNormals(m.N);
            mesh.boneWeights = m.W.ToArray();
            mesh.subMeshCount = 2;
            mesh.SetTriangles(m.Sub[0], 0);
            mesh.SetTriangles(m.Sub[1], 1);
            mesh.bindposes = Skeleton.Select(b => t[b.Name].worldToLocalMatrix * root.localToWorldMatrix).ToArray();
            var (hs, hc) = m.Boxes["Head"];
            var names = Visemes.Concat(Faces).ToArray();
            for (int s = 0; s < names.Length; s++)
            {
                var dv = new Vector3[m.V.Count];
                float phase = s * 0.7f;
                for (int i = hs; i < hs + hc; i++)
                {
                    var p = m.V[i];
                    if (p.z > 0) dv[i] = new Vector3(0.02f * Mathf.Sin(phase) * Mathf.Sign(p.x), 0.015f * Mathf.Cos(phase), 0.03f + 0.002f * s);
                }
                mesh.AddBlendShapeFrame(names[s], 100, dv, null, null);
            }
            mesh.RecalculateBounds();
            return mesh;
        }

        static Mesh BoxMesh(string name, Vector3 offset, Vector3 size, Dictionary<string, Transform> t, string bone, Transform root)
        {
            var m = new MeshBuilder();
            var a = root.InverseTransformPoint(t[bone].position) + offset;
            m.Box(name, a - new Vector3(0, size.y / 2, 0), a + new Vector3(0, size.y / 2, 0), size.x, Index(bone), 0);
            var mesh = new Mesh { name = name };
            mesh.SetVertices(m.V);
            mesh.SetNormals(m.N);
            mesh.boneWeights = m.W.ToArray();
            mesh.SetTriangles(m.Sub[0], 0);
            mesh.bindposes = Skeleton.Select(b => t[b.Name].worldToLocalMatrix * root.localToWorldMatrix).ToArray();
            mesh.RecalculateBounds();
            return mesh;
        }

        static Avatar HumanAvatar(GameObject root, Dictionary<string, Transform> t)
        {
            var desc = new HumanDescription
            {
                human = Human.Select(kv => new HumanBone { boneName = kv.Value, humanName = HumanTrait.BoneName.First(h => h.Replace(" ", "") == kv.Key), limit = new HumanLimit { useDefaultValues = true } }).ToArray(),
                skeleton = new[] { new SkeletonBone { name = root.name, position = Vector3.zero, rotation = Quaternion.identity, scale = Vector3.one } }
                    .Concat(Skeleton.Select(b => new SkeletonBone { name = b.Name, position = t[b.Name].localPosition, rotation = t[b.Name].localRotation, scale = Vector3.one })).ToArray(),
                upperArmTwist = 0.5f, lowerArmTwist = 0.5f, upperLegTwist = 0.5f, lowerLegTwist = 0.5f,
                armStretch = 0.05f, legStretch = 0.05f, feetSpacing = 0, hasTranslationDoF = false,
            };
            var avatar = AvatarBuilder.BuildHumanAvatar(root, desc);
            if (!avatar.isValid || !avatar.isHuman) throw new Exception("fixture humanoid avatar is invalid");
            avatar.name = "AWFixtureAvatar";
            AssetDatabase.CreateAsset(avatar, $"{Dir}/Avatar.asset");
            return avatar;
        }

        static void Orbs(Transform root, Dictionary<string, Transform> t, Material mat)
        {
            var cube = Resources.GetBuiltinResource<Mesh>("Cube.fbx");
            var group = new GameObject("Orbs").transform;
            group.SetParent(root, false);
            Transform Orb(string name, Vector3 pos, float size = 0.06f)
            {
                var go = new GameObject(name);
                go.transform.SetParent(group, false);
                go.transform.localPosition = pos;
                go.transform.localScale = Vector3.one * size;
                go.AddComponent<MeshFilter>().sharedMesh = cube;
                go.AddComponent<MeshRenderer>().sharedMaterial = mat;
                return go.transform;
            }

            var parent = Orb("Parent", new Vector3(-0.85f, 1.35f, 0));
            var pc = parent.gameObject.AddComponent<VRCParentConstraint>();
            pc.Sources.Add(new VRCConstraintSource { SourceTransform = t["LeftHand"], Weight = 1, ParentPositionOffset = new Vector3(0, 0, 0.08f), ParentRotationOffset = Vector3.zero });
            pc.IsActive = true; pc.Locked = true;

            var position = Orb("Position", new Vector3(0, 1.3f, 0.3f));
            var pos = position.gameObject.AddComponent<VRCPositionConstraint>();
            pos.Sources.Add(new VRCConstraintSource { SourceTransform = t["LeftHand"], Weight = 0.5f });
            pos.Sources.Add(new VRCConstraintSource { SourceTransform = t["RightHand"], Weight = 0.5f });
            pos.IsActive = true; pos.Locked = true;
            Flags(pos, ("AffectsPositionY", false), ("PositionOffset", new Vector3(0, 0, 0.3f)));

            var rotation = Orb("Rotation", new Vector3(0.3f, 1.8f, 0.2f), 0.1f);
            var rot = rotation.gameObject.AddComponent<VRCRotationConstraint>();
            rot.Sources.Add(new VRCConstraintSource { SourceTransform = t["Head"], Weight = 1 });
            rot.GlobalWeight = 0.75f; rot.IsActive = true; rot.Locked = true;
            Flags(rot, ("AffectsRotationX", false));

            var scale = Orb("Scale", new Vector3(-0.3f, 1.8f, 0.2f));
            var sc = scale.gameObject.AddComponent<VRCScaleConstraint>();
            var scaleSource = new GameObject("ScaleSource").transform;
            scaleSource.SetParent(t["RightHand"], false);
            scaleSource.localScale = Vector3.one * 0.06f;
            sc.Sources.Add(new VRCConstraintSource { SourceTransform = scaleSource, Weight = 1 });
            sc.IsActive = true; sc.Locked = true; sc.SolveInLocalSpace = true;

            var aim = Orb("Aim", new Vector3(0.4f, 1.2f, 0.4f), 0.08f);
            var ac = aim.gameObject.AddComponent<VRCAimConstraint>();
            ac.Sources.Add(new VRCConstraintSource { SourceTransform = t["Head"], Weight = 1 });
            ac.IsActive = true; ac.Locked = true;
            Flags(ac, ("WorldUp", VRCConstraintBase.WorldUpType.ObjectUp), ("WorldUpTransform", t["Chest"]));

            var look = Orb("LookAt", new Vector3(-0.4f, 1.2f, 0.4f), 0.08f);
            var lc = look.gameObject.AddComponent<VRCLookAtConstraint>();
            lc.Sources.Add(new VRCConstraintSource { SourceTransform = t["RightHand"], Weight = 1 });
            lc.IsActive = true; lc.Locked = true;
            Flags(lc, ("UseUpTransform", true), ("WorldUpTransform", t["Head"]), ("Roll", 15f));

            var frozen = Orb("Frozen", new Vector3(0, 0.5f, 0.4f));
            var fc = frozen.gameObject.AddComponent<VRCParentConstraint>();
            fc.Sources.Add(new VRCConstraintSource { SourceTransform = t["Hips"], Weight = 1, ParentPositionOffset = new Vector3(0, -0.45f, 0.4f) });
            fc.IsActive = true; fc.Locked = true; fc.FreezeToWorld = true;

            Orb("Puppet", new Vector3(0, 2.0f, 0), 0.07f);
            Orb("Free", new Vector3(0, 2.15f, 0), 0.05f);
        }

        static void Flags(Component c, params (string name, object value)[] values)
        {
            var type = c.GetType();
            foreach (var (name, value) in values)
            {
                var f = type.GetField(name);
                if (f != null) { f.SetValue(c, value); continue; }
                var p = type.GetProperty(name);
                if (p == null || !p.CanWrite) throw new Exception($"{type.Name} has no settable {name}");
                p.SetValue(c, value);
            }
        }

        static AnimationCurve Ramp(float a, float b) => new AnimationCurve(new Keyframe(0, a, 0, b - a), new Keyframe(1, b, b - a, 0));

        static void PhysBones(Dictionary<string, Transform> t)
        {
            VRCPhysBoneCollider Collider(string bone, VRCPhysBoneColliderBase.ShapeType shape, float radius, float height, Vector3 pos, Quaternion rot, bool inside = false, bool asSpheres = false)
            {
                var c = t[bone].gameObject.AddComponent<VRCPhysBoneCollider>();
                c.shapeType = shape; c.radius = radius; c.height = height; c.position = pos; c.rotation = rot; c.insideBounds = inside; c.bonesAsSpheres = asSpheres;
                return c;
            }
            var hipSphere = Collider("Hips", VRCPhysBoneColliderBase.ShapeType.Sphere, 0.12f, 0, new Vector3(0, -0.05f, -0.02f), Quaternion.identity);
            var legL = Collider("LeftUpperLeg", VRCPhysBoneColliderBase.ShapeType.Capsule, 0.07f, 0.45f, new Vector3(0, -0.2f, 0), Quaternion.identity);
            var legR = Collider("RightUpperLeg", VRCPhysBoneColliderBase.ShapeType.Capsule, 0.07f, 0.45f, new Vector3(0, -0.2f, 0), Quaternion.identity);
            var floor = Collider("Hips", VRCPhysBoneColliderBase.ShapeType.Plane, 0, 0, new Vector3(0, -0.55f, 0), Quaternion.identity);
            var skirtBounds = Collider("Hips", VRCPhysBoneColliderBase.ShapeType.Sphere, 0.42f, 0, new Vector3(0, -0.2f, 0), Quaternion.identity, inside: true);
            var headSphere = Collider("Head", VRCPhysBoneColliderBase.ShapeType.Capsule, 0.1f, 0.3f, new Vector3(0, 0.1f, 0), Quaternion.Euler(0, 0, 90), asSpheres: true);

            var tail = t["Tail.0"].gameObject.AddComponent<VRCPhysBone>();
            tail.version = VRCPhysBoneBase.Version.Version_1_0;
            tail.integrationType = VRCPhysBoneBase.IntegrationType.Simplified;
            tail.pull = 0.2f; tail.pullCurve = Ramp(1, 0.5f);
            tail.spring = 0.4f; tail.stiffness = 0.2f;
            tail.gravity = 0.4f; tail.gravityCurve = Ramp(0.5f, 1);
            tail.gravityFalloff = 0.5f;
            tail.immobileType = VRCPhysBoneBase.ImmobileType.AllMotion; tail.immobile = 0.3f; tail.immobileCurve = Ramp(1, 0);
            tail.limitType = VRCPhysBoneBase.LimitType.Angle; tail.maxAngleX = 60; tail.maxAngleXCurve = Ramp(0.5f, 1);
            tail.limitRotation = new Vector3(10, 0, 0);
            tail.radius = 0.03f; tail.radiusCurve = Ramp(1, 0.5f);
            tail.allowCollision = VRCPhysBoneBase.AdvancedBool.True;
            tail.colliders = new List<VRCPhysBoneColliderBase> { hipSphere, legL, legR, floor };
            tail.allowGrabbing = VRCPhysBoneBase.AdvancedBool.True;
            tail.allowPosing = VRCPhysBoneBase.AdvancedBool.True;
            tail.grabMovement = 0.8f;
            tail.parameter = "Tail";

            foreach (var (side, v11) in new[] { ("EarL", true), ("EarR", false) })
            {
                var ear = t[side + ".0"].gameObject.AddComponent<VRCPhysBone>();
                ear.version = v11 ? VRCPhysBoneBase.Version.Version_1_1 : VRCPhysBoneBase.Version.Version_1_0;
                ear.integrationType = v11 ? VRCPhysBoneBase.IntegrationType.Advanced : VRCPhysBoneBase.IntegrationType.Simplified;
                ear.pull = 0.3f; ear.spring = 0.6f; ear.stiffness = 0.3f; ear.springCurve = Ramp(0.5f, 1); ear.stiffnessCurve = Ramp(1, 0.3f);
                ear.gravity = 0.15f; ear.gravityFalloff = 1f;
                ear.immobileType = v11 ? VRCPhysBoneBase.ImmobileType.World : VRCPhysBoneBase.ImmobileType.AllMotion;
                ear.immobile = 0.2f;
                ear.limitType = v11 ? VRCPhysBoneBase.LimitType.Hinge : VRCPhysBoneBase.LimitType.Polar;
                ear.maxAngleX = 45; ear.maxAngleZ = 25; ear.maxAngleZCurve = Ramp(1, 0.6f);
                ear.limitRotation = new Vector3(0, v11 ? 90 : 0, v11 ? 0 : 15);
                ear.limitRotationYCurve = Ramp(1, 0.8f);
                ear.maxStretch = v11 ? 0.3f : 0.1f; ear.maxStretchCurve = Ramp(1, 0.5f);
                ear.maxSquish = v11 ? 0.4f : 0f; ear.maxSquishCurve = Ramp(0.5f, 1);
                ear.stretchMotion = v11 ? 0.5f : 0f; ear.stretchMotionCurve = Ramp(1, 0.5f);
                ear.radius = 0.02f;
                ear.colliders = new List<VRCPhysBoneColliderBase> { headSphere };
                ear.parameter = side;
            }

            var hair = t["Hair"].gameObject.AddComponent<VRCPhysBone>();
            hair.version = VRCPhysBoneBase.Version.Version_1_1;
            hair.integrationType = VRCPhysBoneBase.IntegrationType.Advanced;
            hair.multiChildType = VRCPhysBoneBase.MultiChildType.First;
            hair.endpointPosition = new Vector3(0, -0.05f, 0);
            hair.pull = 0.15f; hair.spring = 0.5f; hair.stiffness = 0.1f; hair.gravity = 0.5f; hair.gravityFalloff = 0.2f;
            hair.limitType = VRCPhysBoneBase.LimitType.Angle; hair.maxAngleX = 80;
            hair.ignoreTransforms = new List<Transform> { t["HairC.0"] };
            hair.colliders = new List<VRCPhysBoneColliderBase> { headSphere };
            hair.radius = 0.02f;

            var skirt = t["Skirt"].gameObject.AddComponent<VRCPhysBone>();
            skirt.version = VRCPhysBoneBase.Version.Version_1_1;
            skirt.multiChildType = VRCPhysBoneBase.MultiChildType.Average;
            skirt.pull = 0.25f; skirt.spring = 0.3f; skirt.gravity = 0.3f; skirt.gravityFalloff = 0.8f;
            skirt.limitType = VRCPhysBoneBase.LimitType.None;
            skirt.radius = 0.025f;
            skirt.colliders = new List<VRCPhysBoneColliderBase> { legL, legR, skirtBounds };

            var antenna = t["Antenna"].gameObject.AddComponent<VRCPhysBone>();
            antenna.multiChildType = VRCPhysBoneBase.MultiChildType.Ignore;
            antenna.pull = 0.1f; antenna.spring = 0.9f; antenna.gravity = 0; antenna.immobile = 0.5f;
            antenna.allowGrabbing = VRCPhysBoneBase.AdvancedBool.False;
            antenna.allowCollision = VRCPhysBoneBase.AdvancedBool.False;
        }

        static void Contacts(Dictionary<string, Transform> t)
        {
            var sender = t["LeftHand"].gameObject.AddComponent<VRCContactSender>();
            sender.shapeType = ContactBase.ShapeType.Sphere; sender.radius = 0.06f; sender.position = new Vector3(-0.06f, 0, 0);
            sender.collisionTags = new List<string> { "AWHand" };
            var receiver = t["ProxReceiver"].gameObject.AddComponent<VRCContactReceiver>();
            receiver.shapeType = ContactBase.ShapeType.Sphere; receiver.radius = 0.2f;
            receiver.collisionTags = new List<string> { "AWHand" };
            receiver.allowSelf = true; receiver.allowOthers = false;
            receiver.receiverType = ContactReceiver.ReceiverType.Proximity;
            receiver.parameter = "Proximity";
            var touch = t["ProxReceiver"].gameObject.AddComponent<VRCContactReceiver>();
            touch.shapeType = ContactBase.ShapeType.Capsule; touch.radius = 0.08f; touch.height = 0.3f;
            touch.collisionTags = new List<string> { "AWHand" };
            touch.allowSelf = true;
            touch.receiverType = ContactReceiver.ReceiverType.OnEnter;
            touch.parameter = "Touched";
        }

        static ExprParam P(string name, VRCExpressionParameters.ValueType type, float def, bool saved = true, bool synced = true) =>
            new ExprParam { name = name, valueType = type, defaultValue = def, saved = saved, networkSynced = synced };

        static VRCExpressionParameters Parameters()
        {
            var p = ScriptableObject.CreateInstance<VRCExpressionParameters>();
            const VRCExpressionParameters.ValueType B = VRCExpressionParameters.ValueType.Bool, I = VRCExpressionParameters.ValueType.Int, F = VRCExpressionParameters.ValueType.Float;
            p.parameters = new[]
            {
                P("Hat", B, 0), P("Outfit", I, 0), P("Smile", F, 0), P("Hue", F, 0.25f, saved: false),
                P("PuppetX", F, 0, saved: false), P("PuppetY", F, 0, saved: false),
                P("Up", F, 0, saved: false), P("Right", F, 0, saved: false), P("Down", F, 0, saved: false), P("Left", F, 0, saved: false),
                P("Wave", B, 0, saved: false), P("InPuppets", B, 0, saved: false, synced: false),
                P("Counter", I, 0, saved: false, synced: false), P("Rand", F, 0, saved: false), P("Copied", F, 0, saved: false),
                P("Setter", B, 0, saved: false), P("Local", B, 0, saved: false, synced: false), P("Chance", B, 0, saved: false),
            };
            AssetDatabase.CreateAsset(p, $"{Dir}/Parameters.asset");
            return p;
        }

        static Control C(string name, Control.ControlType type, string param, float value = 1, params string[] sub) => new Control
        {
            name = name, type = type, value = value,
            parameter = new Control.Parameter { name = param ?? "" },
            subParameters = sub.Select(s => new Control.Parameter { name = s }).ToArray(),
            labels = new Control.Label[type == Control.ControlType.FourAxisPuppet || type == Control.ControlType.TwoAxisPuppet ? 4 : 0],
        };

        static VRCExpressionsMenu Menu(VRCExpressionParameters parameters)
        {
            var puppets = ScriptableObject.CreateInstance<VRCExpressionsMenu>();
            puppets.Parameters = parameters;
            puppets.controls = new List<Control>
            {
                C("Lean", Control.ControlType.TwoAxisPuppet, null, 1, "PuppetX", "PuppetY"),
                C("Blend", Control.ControlType.FourAxisPuppet, null, 1, "Up", "Right", "Down", "Left"),
            };
            AssetDatabase.CreateAsset(puppets, $"{Dir}/MenuPuppets.asset");
            var main = ScriptableObject.CreateInstance<VRCExpressionsMenu>();
            main.Parameters = parameters;
            main.controls = new List<Control>
            {
                C("Hat", Control.ControlType.Toggle, "Hat"),
                C("Outfit Red", Control.ControlType.Toggle, "Outfit", 1),
                C("Outfit Tint", Control.ControlType.Toggle, "Outfit", 2),
                C("Wave", Control.ControlType.Button, "Wave"),
                C("Smile", Control.ControlType.RadialPuppet, null, 1, "Smile"),
                C("Hue", Control.ControlType.RadialPuppet, null, 1, "Hue"),
            };
            var sub = C("Puppets", Control.ControlType.SubMenu, "InPuppets");
            sub.subMenu = puppets;
            main.controls.Add(sub);
            AssetDatabase.CreateAsset(main, $"{Dir}/Menu.asset");
            return main;
        }

        class Clips
        {
            readonly string path;
            public Clips(string path) { this.path = path; }
            public AnimationClip Make(string name, Action<AnimationClip> fill, float length = 0, bool loop = false)
            {
                var c = new AnimationClip { name = name };
                fill(c);
                var s = AnimationUtility.GetAnimationClipSettings(c);
                s.loopTime = loop;
                AnimationUtility.SetAnimationClipSettings(c, s);
                AssetDatabase.CreateAsset(c, $"{path}/{name}.anim");
                return c;
            }
        }

        static void Curve(AnimationClip c, string path, Type type, string prop, params (float t, float v)[] keys)
        {
            var curve = new AnimationCurve(keys.Select(k => new Keyframe(k.t, k.v)).ToArray());
            for (int i = 0; i < curve.length; i++) AnimationUtility.SetKeyLeftTangentMode(curve, i, AnimationUtility.TangentMode.Auto);
            AnimationUtility.SetEditorCurve(c, EditorCurveBinding.FloatCurve(path, type, prop), curve);
        }

        static void Const(AnimationClip c, string path, Type type, string prop, float v) => Curve(c, path, type, prop, (0, v), (1 / 60f, v));

        static void Shape(AnimationClip c, string shape, float v) => Const(c, "Body", typeof(SkinnedMeshRenderer), "blendShape." + shape, v);

        static void Pos(AnimationClip c, string path, Vector3 p)
        {
            Const(c, path, typeof(Transform), "m_LocalPosition.x", p.x);
            Const(c, path, typeof(Transform), "m_LocalPosition.y", p.y);
            Const(c, path, typeof(Transform), "m_LocalPosition.z", p.z);
        }

        static void Color(AnimationClip c, string prop, Color col)
        {
            Const(c, "Body", typeof(SkinnedMeshRenderer), $"material.{prop}.r", col.r);
            Const(c, "Body", typeof(SkinnedMeshRenderer), $"material.{prop}.g", col.g);
            Const(c, "Body", typeof(SkinnedMeshRenderer), $"material.{prop}.b", col.b);
            Const(c, "Body", typeof(SkinnedMeshRenderer), $"material.{prop}.a", col.a);
        }

        static AnimatorController FxController()
        {
            var dir = Dir + "/Clips";
            AssetDatabase.CreateFolder(Dir, "Clips");
            var k = new Clips(dir);
            var ctrl = AnimatorController.CreateAnimatorControllerAtPath($"{Dir}/FX.controller");
            ctrl.RemoveLayer(0);
            foreach (var (n, ty) in new[]
            {
                ("Hat", AnimatorControllerParameterType.Bool), ("Outfit", AnimatorControllerParameterType.Int), ("Smile", AnimatorControllerParameterType.Float),
                ("Hue", AnimatorControllerParameterType.Float), ("PuppetX", AnimatorControllerParameterType.Float), ("PuppetY", AnimatorControllerParameterType.Float),
                ("Up", AnimatorControllerParameterType.Float), ("Right", AnimatorControllerParameterType.Float), ("Down", AnimatorControllerParameterType.Float), ("Left", AnimatorControllerParameterType.Float),
                ("Wave", AnimatorControllerParameterType.Bool), ("InPuppets", AnimatorControllerParameterType.Bool), ("Counter", AnimatorControllerParameterType.Int),
                ("Rand", AnimatorControllerParameterType.Float), ("Copied", AnimatorControllerParameterType.Float), ("Setter", AnimatorControllerParameterType.Bool),
                ("Local", AnimatorControllerParameterType.Bool), ("Chance", AnimatorControllerParameterType.Bool),
                ("GestureLeft", AnimatorControllerParameterType.Int), ("GestureRight", AnimatorControllerParameterType.Int), ("GestureLeftWeight", AnimatorControllerParameterType.Float),
                ("Proximity", AnimatorControllerParameterType.Float), ("Touched", AnimatorControllerParameterType.Bool),
                ("Tail_Angle", AnimatorControllerParameterType.Float), ("Tail_IsGrabbed", AnimatorControllerParameterType.Bool), ("Tail_Stretch", AnimatorControllerParameterType.Float),
                ("EarL_Squish", AnimatorControllerParameterType.Float), ("IsLocal", AnimatorControllerParameterType.Bool), ("Viseme", AnimatorControllerParameterType.Int),
                ("Grounded", AnimatorControllerParameterType.Bool),
            }) ctrl.AddParameter(n, ty);

            AnimatorControllerLayer Layer(string name, AnimatorLayerBlendingMode mode = AnimatorLayerBlendingMode.Override)
            {
                var sm = new AnimatorStateMachine { name = name, hideFlags = HideFlags.HideInHierarchy };
                AssetDatabase.AddObjectToAsset(sm, ctrl);
                var l = new AnimatorControllerLayer { name = name, stateMachine = sm, defaultWeight = 1, blendingMode = mode };
                ctrl.AddLayer(l);
                return l;
            }

            AnimatorStateTransition Go(AnimatorState from, AnimatorState to, float duration, bool fixedDuration = true)
            {
                var tr = from.AddTransition(to);
                tr.hasExitTime = false; tr.duration = duration; tr.hasFixedDuration = fixedDuration;
                return tr;
            }

            BlendTree Tree(string name, BlendTreeType type, string px, string py = null)
            {
                var bt = new BlendTree { name = name, blendType = type, blendParameter = px, blendParameterY = py ?? px, useAutomaticThresholds = false, hideFlags = HideFlags.HideInHierarchy };
                AssetDatabase.AddObjectToAsset(bt, ctrl);
                return bt;
            }

            var empty = k.Make("Empty", c => Const(c, "_aw_none", typeof(GameObject), "m_IsActive", 0));

            var hatL = Layer("Hat");
            var hatOff = hatL.stateMachine.AddState("Off"); hatOff.motion = k.Make("HatOff", c => Const(c, "Hat", typeof(GameObject), "m_IsActive", 0)); hatOff.writeDefaultValues = false;
            var hatOn = hatL.stateMachine.AddState("On"); hatOn.motion = k.Make("HatOn", c => Const(c, "Hat", typeof(GameObject), "m_IsActive", 1)); hatOn.writeDefaultValues = false;
            hatL.stateMachine.defaultState = hatOff;
            Go(hatOff, hatOn, 0.25f).AddCondition(AnimatorConditionMode.If, 0, "Hat");
            Go(hatOn, hatOff, 0.25f).AddCondition(AnimatorConditionMode.IfNot, 0, "Hat");

            var outL = Layer("Outfit");
            var o0 = outL.stateMachine.AddState("Plain"); o0.motion = empty; o0.writeDefaultValues = true;
            var o1 = outL.stateMachine.AddState("Red"); o1.writeDefaultValues = true;
            o1.motion = k.Make("OutfitRed", c => AnimationUtility.SetObjectReferenceCurve(c, EditorCurveBinding.PPtrCurve("Body", typeof(SkinnedMeshRenderer), "m_Materials.Array.data[1]"),
                new[] { new ObjectReferenceKeyframe { time = 0, value = AssetDatabase.LoadAssetAtPath<Material>($"{Dir}/ClothAlt.mat") } }));
            var o2 = outL.stateMachine.AddState("Tint"); o2.writeDefaultValues = true;
            o2.motion = k.Make("OutfitTint", c => { Color(c, "_Color", new Color(0.3f, 0.9f, 0.9f, 1)); Const(c, "Hat", typeof(SkinnedMeshRenderer), "m_Enabled", 0); });
            outL.stateMachine.defaultState = o0;
            foreach (var (s, v) in new[] { (o0, 0), (o1, 1), (o2, 2) })
            {
                var tr = outL.stateMachine.AddAnyStateTransition(s);
                tr.hasExitTime = false; tr.duration = 0.1f; tr.hasFixedDuration = false; tr.canTransitionToSelf = false;
                tr.AddCondition(AnimatorConditionMode.Equals, v, "Outfit");
            }

            var smileL = Layer("Smile");
            var smileTree = Tree("Smile", BlendTreeType.Simple1D, "Smile");
            smileTree.AddChild(k.Make("Smile0", c => Shape(c, "Smile", 0)), 0);
            smileTree.AddChild(k.Make("Smile100", c => Shape(c, "Smile", 100)), 1);
            var smileS = smileL.stateMachine.AddState("Smile"); smileS.motion = smileTree; smileS.writeDefaultValues = false;

            var hueL = Layer("Hue");
            var hueTree = Tree("Hue", BlendTreeType.Simple1D, "Hue");
            for (int i = 0; i <= 4; i++)
            {
                var col = UnityEngine.Color.HSVToRGB(i / 4f % 1f, 0.6f, 0.9f);
                hueTree.AddChild(k.Make($"Hue{i}", c => Color(c, "_Color", col)), i / 4f);
            }
            var hueS = hueL.stateMachine.AddState("Hue"); hueS.motion = hueTree; hueS.writeDefaultValues = false;

            var leanL = Layer("Lean");
            var lean = Tree("Lean", BlendTreeType.SimpleDirectional2D, "PuppetX", "PuppetY");
            lean.AddChild(k.Make("LeanCenter", c => Pos(c, "Orbs/Puppet", new Vector3(0, 2.0f, 0))), Vector2.zero);
            lean.AddChild(k.Make("LeanUp", c => { Pos(c, "Orbs/Puppet", new Vector3(0, 2.15f, 0)); Shape(c, "LeanUp", 100); }), new Vector2(0, 1));
            lean.AddChild(k.Make("LeanRight", c => { Pos(c, "Orbs/Puppet", new Vector3(0.15f, 2.0f, 0)); Shape(c, "LeanRight", 100); }), new Vector2(1, 0));
            lean.AddChild(k.Make("LeanDown", c => { Pos(c, "Orbs/Puppet", new Vector3(0, 1.85f, 0)); Shape(c, "LeanDown", 100); }), new Vector2(0, -1));
            lean.AddChild(k.Make("LeanLeft", c => { Pos(c, "Orbs/Puppet", new Vector3(-0.15f, 2.0f, 0)); Shape(c, "LeanLeft", 100); }), new Vector2(-1, 0));
            var leanS = leanL.stateMachine.AddState("Lean"); leanS.motion = lean; leanS.writeDefaultValues = true;

            var freeL = Layer("Freeform");
            var freeDir = Tree("FreeDirectional", BlendTreeType.FreeformDirectional2D, "PuppetX", "PuppetY");
            freeDir.AddChild(k.Make("FreeCenter", c => Pos(c, "Orbs/Free", new Vector3(0, 2.15f, 0))), Vector2.zero);
            freeDir.AddChild(k.Make("FreeA", c => Pos(c, "Orbs/Free", new Vector3(0.1f, 2.25f, 0))), new Vector2(0.7f, 0.7f));
            freeDir.AddChild(k.Make("FreeB", c => Pos(c, "Orbs/Free", new Vector3(-0.1f, 2.25f, 0))), new Vector2(-0.7f, 0.7f));
            freeDir.AddChild(k.Make("FreeC", c => Pos(c, "Orbs/Free", new Vector3(0, 2.05f, 0.1f))), new Vector2(0, -1));
            var freeS = freeL.stateMachine.AddState("Directional"); freeS.motion = freeDir; freeS.writeDefaultValues = false;

            var cartL = Layer("Cartesian");
            var cart = Tree("Cartesian", BlendTreeType.FreeformCartesian2D, "Up", "Right");
            cart.AddChild(k.Make("Cart00", c => Const(c, "Orbs/Free", typeof(Transform), "m_LocalScale.x", 0.05f)), new Vector2(0, 0));
            cart.AddChild(k.Make("Cart10", c => Const(c, "Orbs/Free", typeof(Transform), "m_LocalScale.x", 0.1f)), new Vector2(1, 0));
            cart.AddChild(k.Make("Cart01", c => Const(c, "Orbs/Free", typeof(Transform), "m_LocalScale.x", 0.02f)), new Vector2(0, 1));
            cart.AddChild(k.Make("Cart11", c => Const(c, "Orbs/Free", typeof(Transform), "m_LocalScale.x", 0.15f)), new Vector2(1, 1));
            var cartS = cartL.stateMachine.AddState("Cartesian"); cartS.motion = cart; cartS.writeDefaultValues = false;

            var directL = Layer("Direct");
            var direct = Tree("Direct", BlendTreeType.Direct, "Up");
            var dirKids = new List<ChildMotion>();
            foreach (var (pname, shape) in new[] { ("Up", "Fist"), ("Right", "Open"), ("Down", "Point"), ("Left", "Victory") })
                dirKids.Add(new ChildMotion { motion = k.Make("Direct" + pname, c => Shape(c, shape, 100)), directBlendParameter = pname, timeScale = 1 });
            direct.children = dirKids.ToArray();
            var directS = directL.stateMachine.AddState("Direct"); directS.motion = direct; directS.writeDefaultValues = true;

            var waveL = Layer("Wave");
            var idle = waveL.stateMachine.AddState("Idle"); idle.motion = empty; idle.writeDefaultValues = false;
            var waving = waveL.stateMachine.AddState("Waving"); waving.writeDefaultValues = false;
            waving.motion = k.Make("Waving", c => Curve(c, "Body", typeof(SkinnedMeshRenderer), "blendShape.Wave", (0, 0), (0.25f, 100), (0.5f, 0)), loop: true);
            waving.speed = 1.5f; waving.cycleOffset = 0.1f;
            var drv = waving.AddStateMachineBehaviour<VRCAvatarParameterDriver>();
            drv.parameters = new List<VRC_AvatarParameterDriver.Parameter>
            {
                new VRC_AvatarParameterDriver.Parameter { type = VRC_AvatarParameterDriver.ChangeType.Set, name = "Setter", value = 1 },
                new VRC_AvatarParameterDriver.Parameter { type = VRC_AvatarParameterDriver.ChangeType.Add, name = "Counter", value = 1 },
                new VRC_AvatarParameterDriver.Parameter { type = VRC_AvatarParameterDriver.ChangeType.Random, name = "Rand", valueMin = 0.2f, valueMax = 0.9f },
                new VRC_AvatarParameterDriver.Parameter { type = VRC_AvatarParameterDriver.ChangeType.Random, name = "Chance", chance = 0.5f },
                new VRC_AvatarParameterDriver.Parameter { type = VRC_AvatarParameterDriver.ChangeType.Copy, name = "Copied", source = "Smile", convertRange = true, sourceMin = 0, sourceMax = 1, destMin = 1, destMax = 0 },
            };
            var localDrv = waving.AddStateMachineBehaviour<VRCAvatarParameterDriver>();
            localDrv.localOnly = true;
            localDrv.parameters = new List<VRC_AvatarParameterDriver.Parameter> { new VRC_AvatarParameterDriver.Parameter { type = VRC_AvatarParameterDriver.ChangeType.Set, name = "Local", value = 1 } };
            waveL.stateMachine.defaultState = idle;
            var toWave = Go(idle, waving, 0.2f, false); toWave.AddCondition(AnimatorConditionMode.If, 0, "Wave"); toWave.offset = 0.25f;
            var back = waving.AddTransition(idle); back.hasExitTime = true; back.exitTime = 0.9f; back.duration = 0.15f; back.hasFixedDuration = true;
            back.AddCondition(AnimatorConditionMode.IfNot, 0, "Wave");

            var gestL = Layer("Gestures");
            var faces = gestL.stateMachine.AddStateMachine("Faces");
            var gStates = new AnimatorState[8];
            string[] gShapes = { null, "Fist", "Open", "Point", "Victory", "Rock", "Gun", "Thumbs" };
            for (int g = 0; g < 8; g++)
            {
                var s = faces.AddState(g == 0 ? "Neutral" : gShapes[g]);
                var shape = gShapes[g];
                s.motion = g == 0 ? empty : k.Make("Gesture" + shape, c => Shape(c, shape, 100));
                s.writeDefaultValues = g % 2 == 0;
                if (g == 1) { s.timeParameterActive = false; s.speedParameterActive = true; s.speedParameter = "GestureLeftWeight"; }
                gStates[g] = s;
            }
            faces.defaultState = gStates[0];
            gestL.stateMachine.defaultState = gStates[0];
            var interrupt = new[] { TransitionInterruptionSource.None, TransitionInterruptionSource.Source, TransitionInterruptionSource.Destination, TransitionInterruptionSource.SourceThenDestination, TransitionInterruptionSource.DestinationThenSource };
            for (int g = 0; g < 8; g++)
            {
                var tr = gestL.stateMachine.AddAnyStateTransition(gStates[g]);
                tr.hasExitTime = false; tr.hasFixedDuration = g % 2 == 0; tr.duration = g % 2 == 0 ? 0.1f : 0.3f;
                tr.interruptionSource = interrupt[g % interrupt.Length]; tr.orderedInterruption = g % 3 != 0; tr.canTransitionToSelf = false;
                tr.AddCondition(AnimatorConditionMode.Equals, g, "GestureLeft");
            }

            var contactL = Layer("Contact");
            var prox = Tree("Proximity", BlendTreeType.Simple1D, "Proximity");
            prox.AddChild(k.Make("Pout0", c => Shape(c, "Pout", 0)), 0);
            prox.AddChild(k.Make("Pout100", c => Shape(c, "Pout", 100)), 1);
            var proxS = contactL.stateMachine.AddState("Proximity"); proxS.motion = prox; proxS.writeDefaultValues = false;

            var tailL = Layer("TailParam");
            var tailTree = Tree("TailAngle", BlendTreeType.Simple1D, "Tail_Angle");
            tailTree.AddChild(k.Make("TailAngle0", c => Shape(c, "TailAngle", 0)), 0);
            tailTree.AddChild(k.Make("TailAngle100", c => Shape(c, "TailAngle", 100)), 1);
            var tailS = tailL.stateMachine.AddState("Angle"); tailS.motion = tailTree; tailS.writeDefaultValues = true;
            var grabbed = tailL.stateMachine.AddState("Grabbed"); grabbed.motion = k.Make("Grabbed", c => Const(c, "Orbs/Frozen", typeof(MeshRenderer), "m_Enabled", 0)); grabbed.writeDefaultValues = true;
            Go(tailS, grabbed, 0).AddCondition(AnimatorConditionMode.If, 0, "Tail_IsGrabbed");
            Go(grabbed, tailS, 0).AddCondition(AnimatorConditionMode.IfNot, 0, "Tail_IsGrabbed");

            var addL = Layer("Additive", AnimatorLayerBlendingMode.Additive);
            var addS = addL.stateMachine.AddState("Add"); addS.writeDefaultValues = false;
            addS.motion = k.Make("Additive", c => Curve(c, "Orbs/Puppet", typeof(Transform), "m_LocalPosition.z", (0, 0), (1, 0.1f), (2, 0)), loop: true);

            var visL = Layer("Viseme");
            var visIdle = visL.stateMachine.AddState("Quiet"); visIdle.motion = empty; visIdle.writeDefaultValues = false;
            var visTalk = visL.stateMachine.AddState("Talking"); visTalk.motion = k.Make("Talking", c => Const(c, "Orbs/Rotation", typeof(MeshRenderer), "m_Enabled", 0)); visTalk.writeDefaultValues = false;
            visL.stateMachine.defaultState = visIdle;
            Go(visIdle, visTalk, 0.05f).AddCondition(AnimatorConditionMode.Greater, 0, "Viseme");
            Go(visTalk, visIdle, 0.05f).AddCondition(AnimatorConditionMode.Equals, 0, "Viseme");

            EditorUtility.SetDirty(ctrl);
            return ctrl;
        }
    }
}
