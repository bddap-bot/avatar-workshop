using System;
using System.Collections.Generic;
using System.Linq;
using System.Reflection;
using UnityEngine;
using VRC.SDK3.Avatars.Components;
using VRC.SDKBase.Editor.BuildPipeline;
using Object = UnityEngine.Object;

namespace AvatarWorkshop.Exporter
{
    public static class Exporter
    {
        public const string Version = "0.1.0";

        // Exports the avatar as the SDK would upload it: a clone, DynamicBones migrated by the SDK's own
        // conversion, then the SDK's build preprocess (which runs every build-time tool and strips EditorOnly).
        public static ExportReport Export(GameObject avatar, string path)
        {
            if (avatar.GetComponent<VRCAvatarDescriptor>() == null) throw new ArgumentException(avatar.name + " has no VRCAvatarDescriptor");
            var wasActive = avatar.activeSelf;
            var clone = Object.Instantiate(avatar);
            clone.name = avatar.name;
            clone.SetActive(true);
            avatar.SetActive(false);
            try
            {
                bool migrated = MigrateDynamicBones(clone);
                if (!VRCBuildPipelineCallbacks.OnPreprocessAvatar(clone)) throw new Exception("the SDK's build preprocess failed for " + avatar.name);
                var export = new Export(clone);
                export.Run(path);
                return new ExportReport { Path = path, Nodes = export.NodeCount, Unsupported = export.Unsupported, MigratedDynamicBones = migrated };
            }
            finally
            {
                Object.DestroyImmediate(clone);
                avatar.SetActive(wasActive);
            }
        }

        static bool MigrateDynamicBones(GameObject avatar)
        {
            var db = AppDomain.CurrentDomain.GetAssemblies().Select(a => a.GetType("DynamicBone")).FirstOrDefault(t => t != null);
            if (db == null || avatar.GetComponentsInChildren(db, true).Length == 0) return false;
            var convert = typeof(VRC.SDK3.Avatars.AvatarDynamicsSetup).GetMethod("ConvertToPhysBones", BindingFlags.NonPublic | BindingFlags.Static)
                ?? throw new Exception("the SDK has no DynamicBone conversion");
            convert.Invoke(null, new object[] { avatar });
            return true;
        }
    }

    public sealed class ExportReport
    {
        public string Path;
        public int Nodes;
        public bool MigratedDynamicBones;
        public SortedDictionary<string, int> Unsupported;
    }

    sealed partial class Export
    {
        readonly GameObject avatar;
        readonly Transform root;
        readonly VRCAvatarDescriptor descriptor;
        readonly Gltf gltf = new Gltf();
        readonly List<Transform> nodes = new List<Transform>();
        readonly Dictionary<Transform, int> nodeIndex = new Dictionary<Transform, int>();
        public readonly SortedDictionary<string, int> Unsupported = new SortedDictionary<string, int>(StringComparer.Ordinal);
        public int NodeCount => nodes.Count;

        public Export(GameObject avatar)
        {
            this.avatar = avatar;
            root = avatar.transform;
            descriptor = avatar.GetComponent<VRCAvatarDescriptor>();
        }

        public void Drop(string kind, int count = 1)
        {
            if (count <= 0) return;
            Unsupported.TryGetValue(kind, out int n);
            Unsupported[kind] = n + count;
        }

        int? Node(Transform t) => t != null && nodeIndex.TryGetValue(t, out int i) ? i : (int?)null;

        // A component's optional transform reference, or the component's own node when it has none.
        int NodeOr(Transform t, Component owner, string what)
        {
            if (t == null) return nodeIndex[owner.transform];
            if (Node(t) is int n) return n;
            Drop(what + " outside the avatar, taken as the component's own node");
            return nodeIndex[owner.transform];
        }

        // The frame every World matrix is given in: the avatar root's position and rotation, not its scale.
        Matrix4x4 AvatarFrame => Matrix4x4.TRS(root.position, root.rotation, Vector3.one).inverse;

        public void Run(string path)
        {
            Collect(root);
            if (root.rotation != Quaternion.identity) Drop("avatar root rotated in the scene; world-up vectors stay in scene space");
            var renderers = nodes.Select(t => t.GetComponent<Renderer>()).Where(r => r is SkinnedMeshRenderer || r is MeshRenderer).ToList();
            var slots = renderers.ToDictionary(r => r, r => r.sharedMaterials);
            var weights = renderers.OfType<SkinnedMeshRenderer>().ToDictionary(r => r, r =>
                r.sharedMesh == null ? new float[0] : Enumerable.Range(0, r.sharedMesh.blendShapeCount).Select(r.GetBlendShapeWeight).ToArray());
            WriteNodes(renderers, slots, weights);

            var model = new Obj();
            model.Add("source", new Obj { { "importer", "avatar-workshop-unity-exporter" }, { "importerVersion", Exporter.Version }, { "format", "vrchat" } });
            model.Add("humanoid", Humanoid());
            model.Add("descriptor", Descriptor());
            var colliders = Colliders();
            var physbones = PhysBones();
            var contacts = Contacts();
            var constraints = Constraints();
            var parameters = Parameters();
            var menu = Menu();
            var layers = Layers();

            // The default states run on the clone; everything read before this is the authored rest state.
            EvaluateInitialState();

            model.Add("renderers", renderers.Select(r => (object)new Obj
            {
                { "node", nodeIndex[r.transform] }, { "enabled", r.enabled },
                { "materials", slots[r].Select(m => (object)Material(m)).ToList() },
            }).ToList());
            model.Add("nodes", nodes.Select(t => (object)new Obj { { "active", t.gameObject.activeSelf } }).ToList());
            FillEnabled(physbones, constraints, contacts);
            model.Add("materials", materialRecords);
            model.Add("physbones", physbones.Select(p => (object)p.record).ToList());
            model.Add("colliders", colliders);
            model.Add("contacts", new Obj { { "senders", contacts.Where(c => c.sender).Select(c => (object)c.record).ToList() }, { "receivers", contacts.Where(c => !c.sender).Select(c => (object)c.record).ToList() } });
            model.Add("constraints", constraints.Select(c => (object)c.record).ToList());
            model.Add("parameters", parameters);
            model.Add("menu", menu);
            model.Add("layers", layers);
            model.Add("behaviours", behaviourRecords);
            model.Add("clips", clipRecords);
            ReportComponents();
            Json.MakeFinite(model, "", at => Drop("non-finite value in " + at.Split('.', '[')[1] + ", written as 0 or the largest finite value"));
            model.Add("unsupported", Unsupported.Select(kv => (object)new Obj { { "kind", kv.Key }, { "count", kv.Value } }).ToList());
            gltf.WriteGlb(model, path);
        }

        void Collect(Transform t)
        {
            nodeIndex[t] = nodes.Count;
            nodes.Add(t);
            foreach (Transform child in t) Collect(child);
        }

        void WriteNodes(List<Renderer> renderers, Dictionary<Renderer, Material[]> slots, Dictionary<SkinnedMeshRenderer, float[]> weights)
        {
            var meshes = new Dictionary<(Mesh, string, string), int>();
            foreach (var t in nodes)
            {
                var node = new Obj { { "name", t.name } };
                if (t == root)
                {
                    if (t.localScale != Vector3.one) node.Add("scale", Space.S(t.localScale));
                }
                else
                {
                    if (t.localPosition != Vector3.zero) node.Add("translation", Space.P(t.localPosition));
                    if (t.localRotation != Quaternion.identity) node.Add("rotation", Space.Q(t.localRotation));
                    if (t.localScale != Vector3.one) node.Add("scale", Space.S(t.localScale));
                }
                var children = t.Cast<Transform>().Select(c => nodeIndex[c]).ToArray();
                if (children.Length > 0) node.Add("children", children);
                var renderer = t.GetComponent<Renderer>();
                Mesh mesh = renderer is SkinnedMeshRenderer smr ? smr.sharedMesh
                    : renderer is MeshRenderer ? t.GetComponent<MeshFilter>()?.sharedMesh : null;
                if (mesh != null && renderers.Contains(renderer))
                {
                    var skin = renderer is SkinnedMeshRenderer sk && mesh.bindposes.Length > 0 ? Joints(sk, mesh) : null;
                    var key = (mesh, string.Join(",", slots[renderer].Select(Material)), skin == null ? null : string.Join(",", skin.boneToJoint));
                    if (!meshes.TryGetValue(key, out int m))
                    {
                        m = WriteMesh(mesh, slots[renderer], skin);
                        meshes[key] = m;
                    }
                    node.Add("mesh", m);
                    if (renderer is SkinnedMeshRenderer smr2)
                    {
                        if (skin != null) node.Add("skin", WriteSkin(smr2, skin));
                        var w = weights[smr2];
                        if (w.Any(x => x != 0))
                            node.Add("weights", w.Select((x, s) => x / FullWeight(mesh, s)).ToArray());
                    }
                }
                gltf.Nodes.Add(node);
            }
        }

        // A blendshape is fully applied at its last frame's weight; Unity curves and renderer weights use that scale.
        float FullWeight(Mesh mesh, int shape)
        {
            float w = mesh.GetBlendShapeFrameWeight(shape, mesh.GetBlendShapeFrameCount(shape) - 1);
            if (w > 0) return w;
            Drop("blendshape whose last frame weight is not positive, taken as 100");
            return 100;
        }

        sealed class Skin
        {
            public int[] boneToJoint;
            public readonly List<int> joints = new List<int>();
            public readonly List<Matrix4x4> binds = new List<Matrix4x4>();
        }

        // glTF joints are unique nodes, while Unity's bone list may repeat a bone or hold empty entries that no
        // vertex weights; the joints are the distinct bones, and the vertices' bone indices map onto them.
        Skin Joints(SkinnedMeshRenderer r, Mesh mesh)
        {
            var bones = r.bones;
            var binds = mesh.bindposes;
            var weighted = new bool[binds.Length];
            foreach (var bw in mesh.GetAllBoneWeights())
                if (bw.weight > 0 && bw.boneIndex < binds.Length) weighted[bw.boneIndex] = true;
            var skin = new Skin { boneToJoint = new int[binds.Length] };
            var jointOf = new Dictionary<int, int>();
            int Add(int node, Matrix4x4 bind)
            {
                if (jointOf.TryGetValue(node, out int j)) return j;
                jointOf[node] = skin.joints.Count;
                skin.joints.Add(node);
                skin.binds.Add(bind);
                return skin.joints.Count - 1;
            }
            var empty = new List<int>();
            for (int i = 0; i < binds.Length; i++)
            {
                var bone = i < bones.Length ? bones[i] : null;
                if (Node(bone) is int n)
                {
                    if (jointOf.TryGetValue(n, out int j) && !Same(skin.binds[j], binds[i])) Drop("skin bone listed twice with different bind poses");
                    skin.boneToJoint[i] = Add(n, binds[i]);
                }
                else if (weighted[i])
                {
                    Drop("weighted skin bone missing or outside the avatar, bound to the renderer's node");
                    skin.boneToJoint[i] = Add(nodeIndex[r.transform], binds[i]);
                }
                else empty.Add(i);
            }
            if (skin.joints.Count == 0) Add(nodeIndex[r.transform], Matrix4x4.identity);
            foreach (var i in empty) skin.boneToJoint[i] = 0;
            return skin;
        }

        static bool Same(Matrix4x4 a, Matrix4x4 b)
        {
            for (int i = 0; i < 16; i++) if (Mathf.Abs(a[i] - b[i]) > 1e-5f) return false;
            return true;
        }

        int WriteSkin(SkinnedMeshRenderer r, Skin skin)
        {
            var matrices = skin.binds.SelectMany(Space.M).ToArray();
            var o = new Obj { { "joints", skin.joints }, { "inverseBindMatrices", gltf.Floats(matrices, 16, vertex: false) } };
            if (Node(r.rootBone) is int rb && skin.joints.Contains(rb)) o.Add("skeleton", rb);
            gltf.Skins.Add(o);
            return gltf.Skins.Count - 1;
        }

        int WriteMesh(Mesh mesh, Material[] slotMaterials, Skin skin)
        {
            int vc = mesh.vertexCount;
            var attributes = new Obj();
            var pos = mesh.vertices;
            attributes.Add("POSITION", gltf.Floats(Mirrored(pos), 3, bounds: true));
            var normals = mesh.normals;
            if (normals.Length == vc) attributes.Add("NORMAL", gltf.Floats(Mirrored(normals), 3));
            var tangents = mesh.tangents;
            if (tangents.Length == vc)
            {
                var t = new float[vc * 4];
                for (int i = 0; i < vc; i++) { t[i * 4] = -tangents[i].x; t[i * 4 + 1] = tangents[i].y; t[i * 4 + 2] = tangents[i].z; t[i * 4 + 3] = -tangents[i].w; }
                attributes.Add("TANGENT", gltf.Floats(t, 4));
            }
            var uvs = new List<Vector4>();
            for (int ch = 0; ch < 8; ch++)
            {
                var attr = UnityEngine.Rendering.VertexAttribute.TexCoord0 + ch;
                if (!mesh.HasVertexAttribute(attr)) continue;
                int dim = mesh.GetVertexAttributeDimension(attr);
                mesh.GetUVs(ch, uvs);
                if (dim == 1)
                {
                    attributes.Add("_TEXCOORD_" + ch, gltf.Floats(uvs.Select(u => u.x).ToArray(), 1));
                    continue;
                }
                // The first two components are glTF's texture coordinate, v flipped; the rest keep Unity's values.
                var uv = new float[vc * 2];
                for (int i = 0; i < vc; i++) { uv[i * 2] = uvs[i].x; uv[i * 2 + 1] = 1 - uvs[i].y; }
                attributes.Add("TEXCOORD_" + ch, gltf.Floats(uv, 2));
                if (dim > 2)
                {
                    var rest = new float[vc * (dim - 2)];
                    for (int i = 0; i < vc; i++)
                        for (int c = 2; c < dim; c++) rest[i * (dim - 2) + c - 2] = uvs[i][c];
                    attributes.Add($"_TEXCOORD_{ch}_ZW", gltf.Floats(rest, dim - 2));
                }
            }
            if (mesh.HasVertexAttribute(UnityEngine.Rendering.VertexAttribute.Color))
            {
                var colors = mesh.colors;
                var a = new float[vc * 4];
                for (int i = 0; i < vc; i++) { a[i * 4] = colors[i].r; a[i * 4 + 1] = colors[i].g; a[i * 4 + 2] = colors[i].b; a[i * 4 + 3] = colors[i].a; }
                attributes.Add("COLOR_0", gltf.Floats(a, 4));
            }
            if (skin != null) WriteWeights(mesh, attributes, skin.boneToJoint);

            var targets = new List<object>();
            var names = new List<object>();
            var dv = new Vector3[vc];
            var dn = new Vector3[vc];
            var dt = new Vector3[vc];
            for (int s = 0; s < mesh.blendShapeCount; s++)
            {
                int frames = mesh.GetBlendShapeFrameCount(s);
                if (frames > 1) Drop("blendshape frames beyond the last", frames - 1);
                mesh.GetBlendShapeFrameVertices(s, frames - 1, dv, dn, dt);
                var target = new Obj { { "POSITION", gltf.Sparse(Mirrored(dv), 3, bounds: true) } };
                if (normals.Length == vc) target.Add("NORMAL", gltf.Sparse(Mirrored(dn), 3, bounds: false));
                if (tangents.Length == vc) target.Add("TANGENT", gltf.Sparse(Mirrored(dt), 3, bounds: false));
                targets.Add(target);
                names.Add(mesh.GetBlendShapeName(s));
            }

            var primitives = new List<object>();
            var indexAccessors = new Dictionary<int, (int accessor, int mode)>();
            int slotCount = Math.Max(slotMaterials.Length, 1);
            for (int slot = 0; slot < slotCount; slot++)
            {
                // Unity draws a renderer's slots beyond the last submesh with that submesh again.
                int sub = Math.Min(slot, mesh.subMeshCount - 1);
                if (!indexAccessors.TryGetValue(sub, out var ia))
                {
                    ia = Indices(mesh, sub);
                    indexAccessors[sub] = ia;
                }
                var prim = new Obj { { "attributes", attributes }, { "indices", ia.accessor }, { "mode", ia.mode } };
                prim.Add("material", Material(slot < slotMaterials.Length ? slotMaterials[slot] : null));
                if (targets.Count > 0) prim.Add("targets", targets);
                primitives.Add(prim);
            }
            var m = new Obj { { "name", mesh.name }, { "primitives", primitives } };
            if (names.Count > 0) m.Add("extras", new Obj { { "targetNames", names } });
            gltf.Meshes.Add(m);
            return gltf.Meshes.Count - 1;
        }

        (int accessor, int mode) Indices(Mesh mesh, int sub)
        {
            var topology = mesh.GetTopology(sub);
            var idx = mesh.GetIndices(sub);
            switch (topology)
            {
                case MeshTopology.Triangles:
                    for (int i = 0; i + 2 < idx.Length; i += 3) (idx[i + 1], idx[i + 2]) = (idx[i + 2], idx[i + 1]);
                    return (gltf.Indices(idx), 4);
                case MeshTopology.Quads:
                    Drop("quad submesh drawn as triangles");
                    var tri = new List<int>();
                    for (int i = 0; i + 3 < idx.Length; i += 4)
                        tri.AddRange(new[] { idx[i], idx[i + 2], idx[i + 1], idx[i], idx[i + 3], idx[i + 2] });
                    return (gltf.Indices(tri.ToArray()), 4);
                case MeshTopology.Lines: return (gltf.Indices(idx), 1);
                case MeshTopology.LineStrip: return (gltf.Indices(idx), 3);
                default: return (gltf.Indices(idx), 0);
            }
        }

        void WriteWeights(Mesh mesh, Obj attributes, int[] boneToJoint)
        {
            var perVertex = mesh.GetBonesPerVertex();
            if (perVertex.Length == 0) return;
            var all = mesh.GetAllBoneWeights();
            int max = 0;
            foreach (var n in perVertex) max = Math.Max(max, n);
            if (max == 0) return;
            int sets = (max + 3) / 4;
            int vc = perVertex.Length;
            var joints = new ushort[sets][];
            var weights = new float[sets][];
            for (int s = 0; s < sets; s++) { joints[s] = new ushort[vc * 4]; weights[s] = new float[vc * 4]; }
            int at = 0, beyond = 0;
            for (int v = 0; v < vc; v++)
            {
                for (int k = 0; k < perVertex[v]; k++, at++)
                {
                    var bw = all[at];
                    if (bw.boneIndex >= boneToJoint.Length) beyond++;
                    joints[k / 4][v * 4 + k % 4] = checked((ushort)(bw.boneIndex < boneToJoint.Length ? boneToJoint[bw.boneIndex] : 0));
                    weights[k / 4][v * 4 + k % 4] = bw.weight;
                }
            }
            Drop("vertex weight on a bone with no bind pose", beyond);
            for (int s = 0; s < sets; s++)
            {
                attributes.Add("JOINTS_" + s, gltf.Joints(joints[s]));
                attributes.Add("WEIGHTS_" + s, gltf.Floats(weights[s], 4));
            }
        }

        static float[] Mirrored(Vector3[] v)
        {
            var a = new float[v.Length * 3];
            for (int i = 0; i < v.Length; i++) { a[i * 3] = -v[i].x; a[i * 3 + 1] = v[i].y; a[i * 3 + 2] = v[i].z; }
            return a;
        }

        // Every component the model does not represent, by kind.
        void ReportComponents()
        {
            foreach (var t in nodes)
            {
                foreach (var c in t.GetComponents<Component>())
                {
                    if (c == null) { Drop("missing script"); continue; }
                    if (Represented(c)) continue;
                    Drop(c.GetType().Name);
                }
            }
            foreach (var r in nodes.SelectMany(t => t.GetComponents<Renderer>()))
                if (r is SkinnedMeshRenderer smr && smr.sharedMesh == null) Drop("skinned mesh renderer without a mesh");
        }

        bool Represented(Component c) =>
            c is Transform || c is SkinnedMeshRenderer || c is MeshRenderer || c is MeshFilter
            || (c is Animator && c.transform == root) || c is VRCAvatarDescriptor || c is VRC.Core.PipelineManager
            || c is VRC.Dynamics.VRCPhysBoneBase || c is VRC.Dynamics.VRCPhysBoneColliderBase || c is VRC.Dynamics.ContactBase
            || c is VRC.Dynamics.VRCConstraintBase || c is UnityEngine.Animations.IConstraint;
    }
}
