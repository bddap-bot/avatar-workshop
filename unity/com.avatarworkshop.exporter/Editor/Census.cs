using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using Unity.Collections;
using UnityEditor;
using UnityEditor.Animations;
using UnityEngine;
using UnityEngine.Animations;
using UnityEngine.Playables;
using VRC.Dynamics;
using VRC.SDK3.Avatars.Components;
using VRC.SDK3.Avatars.ScriptableObjects;

namespace AvatarWorkshop.Exporter
{
    // What Unity holds for the preprocessed avatar, read straight from Unity's APIs and independent of the
    // exporter's own code, so `avatar-core/examples/census.rs` can compare an export against it. Values stay in
    // Unity's space except the vertex streams, which follow the documented glTF mapping so they compare exactly.
    public static class Census
    {
        public static void Write(GameObject avatar, string path)
        {
            var census = new Writer(avatar);
            using var bin = new FileStream(path + ".bin", FileMode.Create, FileAccess.Write);
            File.WriteAllText(path, Json.Write(census.Run(new BinaryWriter(bin))));
        }

        sealed class Writer
        {
            readonly Transform root;
            readonly VRCAvatarDescriptor descriptor;
            readonly List<Transform> nodes = new List<Transform>();
            readonly Dictionary<Transform, int> index = new Dictionary<Transform, int>();
            BinaryWriter bin;
            long offset;

            public Writer(GameObject avatar)
            {
                root = avatar.transform;
                descriptor = avatar.GetComponent<VRCAvatarDescriptor>();
                void Collect(Transform t)
                {
                    index[t] = nodes.Count;
                    nodes.Add(t);
                    foreach (Transform c in t) Collect(c);
                }
                Collect(root);
            }

            public Obj Run(BinaryWriter b)
            {
                bin = b;
                var all = nodes.SelectMany(t => t.GetComponents<Component>()).Where(c => !Exporter.StrippedOnUpload(c)).ToList();
                var o = new Obj
                {
                    { "nodes", nodes.Select(t => (object)t.name).ToList() },
                    { "authoredInactive", nodes.Where(t => !t.gameObject.activeSelf).Select(t => (object)index[t]).ToList() },
                    { "components", Components(all) },
                    { "physbones", all.OfType<VRCPhysBoneBase>().Select(p => (object)Fields(p)).ToList() },
                    { "colliders", all.OfType<VRCPhysBoneColliderBase>().Select(c => (object)Reference(c)).ToList() },
                    { "contacts", all.OfType<ContactBase>().Select(c => (object)new Obj { { "sender", c is ContactSender }, { "box", c.shapeType == ContactBase.ShapeType.Box }, { "enabled", c.enabled } }).ToList() },
                    { "constraints", all.Where(c => c is VRCConstraintBase || c is IConstraint).Select(c => (object)new Obj { { "type", c.GetType().Name }, { "enabled", ((Behaviour)c).enabled } }).ToList() },
                };
                var renderers = all.OfType<Renderer>().Where(r => r is SkinnedMeshRenderer || r is MeshRenderer).ToList();
                var materials = new List<Material>();
                void Use(Material m) { if (!materials.Contains(m)) materials.Add(m); }
                foreach (var r in renderers) foreach (var m in r.sharedMaterials) Use(m);
                o.Add("renderers", renderers.Select(r => (object)Renderer(r)).ToList());
                var ps = descriptor.customExpressions ? descriptor.expressionParameters : null;
                o.Add("parameters", (ps != null && ps.parameters != null ? ps.parameters : new VRCExpressionParameters.Parameter[0]).Select(p => (object)p.name).ToList());
                var controls = new List<object>();
                MenuControls(descriptor.customExpressions ? descriptor.expressionsMenu : null, new HashSet<VRCExpressionsMenu>(), controls);
                o.Add("menuControls", controls);
                var clips = new List<AnimationClip>();
                o.Add("layers", Layers(clips));
                o.Add("behaviourTypes", behaviourTypes.Select(kv => (object)new Obj { { "type", kv.Key }, { "count", kv.Value } }).ToList());
                for (int i = 0; i < clips.Count; i++)
                {
                    var s = AnimationUtility.GetAnimationClipSettings(clips[i]);
                    if (s.hasAdditiveReferencePose && s.additiveReferencePoseClip != null && !clips.Contains(s.additiveReferencePoseClip)) clips.Add(s.additiveReferencePoseClip);
                }
                o.Add("clips", clips.Count);
                o.Add("clipSettings", clips.Select(c => (object)ClipSettings(c)).ToList());
                o.Add("humanRest", HumanRest());
                o.Add("curves", Curves(clips, Use));
                o.Add("materials", materials.Select(m => (object)MaterialCensus(m)).ToList());
                return o;
            }

            static string Category(Component c) => c switch
            {
                Transform _ => "transform",
                SkinnedMeshRenderer _ => "renderer",
                MeshRenderer _ => "renderer",
                MeshFilter _ => "meshFilter",
                Animator _ => "animator",
                VRCAvatarDescriptor _ => "descriptor",
                VRC.Core.PipelineManager _ => "pipeline",
                VRCPhysBoneBase _ => "physbone",
                VRCPhysBoneColliderBase _ => "collider",
                ContactSender _ => "contactSender",
                ContactReceiver _ => "contactReceiver",
                VRCConstraintBase _ => "constraint",
                IConstraint _ => "constraint",
                _ => "other",
            };

            List<object> Components(List<Component> all) => all
                .Select(c => c == null ? ("missing script", "other", false) : (c.GetType().Name, Category(c), c.transform == root))
                .GroupBy(k => k)
                .Select(g => (object)new Obj { { "type", g.Key.Item1 }, { "category", g.Key.Item2 }, { "onRoot", g.Key.Item3 }, { "count", g.Count() } })
                .ToList();

            Obj Fields(Component c)
            {
                var fields = new Obj();
                var p = new SerializedObject(c).GetIterator();
                for (bool more = p.Next(true); more; more = p.Next(false)) fields.Add(p.name, Value(p));
                return new Obj { { "node", index[c.transform] }, { "type", c.GetType().Name }, { "fields", fields } };
            }

            Obj Reference(Component c) => new Obj { { "node", index[c.transform] }, { "component", Array.IndexOf(c.transform.GetComponents(c.GetType()), c) } };

            object Value(SerializedProperty p)
            {
                switch (p.propertyType)
                {
                    case SerializedPropertyType.Boolean: return p.boolValue;
                    case SerializedPropertyType.Integer: return p.longValue;
                    case SerializedPropertyType.Float: return F(p.floatValue);
                    case SerializedPropertyType.String: return p.stringValue;
                    case SerializedPropertyType.Enum: return p.enumValueIndex >= 0 && p.enumValueIndex < p.enumNames.Length ? p.enumNames[p.enumValueIndex] : p.intValue.ToString();
                    case SerializedPropertyType.Vector2: return new[] { F(p.vector2Value.x), F(p.vector2Value.y) };
                    case SerializedPropertyType.Vector3: return new[] { F(p.vector3Value.x), F(p.vector3Value.y), F(p.vector3Value.z) };
                    case SerializedPropertyType.Vector4: return new[] { F(p.vector4Value.x), F(p.vector4Value.y), F(p.vector4Value.z), F(p.vector4Value.w) };
                    case SerializedPropertyType.Quaternion: return new[] { F(p.quaternionValue.x), F(p.quaternionValue.y), F(p.quaternionValue.z), F(p.quaternionValue.w) };
                    case SerializedPropertyType.AnimationCurve: return CurveKeys(p.animationCurveValue);
                    case SerializedPropertyType.ObjectReference:
                        var obj = p.objectReferenceValue;
                        if (obj == null) return null;
                        var t = obj is Component oc ? oc.transform : obj is GameObject go ? go.transform : null;
                        if (t != null && index.ContainsKey(t)) return obj is Component comp && !(obj is Transform) ? Reference(comp) : new Obj { { "node", index[t] } };
                        return new Obj { { "outside", obj.name } };
                }
                if (p.isArray && p.propertyType == SerializedPropertyType.Generic)
                {
                    var list = new List<object>();
                    for (int i = 0; i < p.arraySize; i++) list.Add(Value(p.GetArrayElementAtIndex(i)));
                    return list;
                }
                if (p.hasChildren && p.propertyType == SerializedPropertyType.Generic)
                {
                    var o = new Obj();
                    var child = p.Copy();
                    var end = p.GetEndProperty();
                    for (bool more = child.Next(true); more && !SerializedProperty.EqualContents(child, end); more = child.Next(false)) o.Add(child.name, Value(child));
                    return o;
                }
                return "<" + p.propertyType + ">";
            }

            static List<object> CurveKeys(AnimationCurve c) => c == null ? new List<object>() : c.keys.Select(k => (object)new object[]
            {
                F(k.time), F(k.value), F(k.inTangent), F(k.outTangent), F(k.inWeight), F(k.outWeight), (int)k.weightedMode,
            }).ToList();

            static object F(float f) => float.IsNaN(f) ? "NaN" : float.IsPositiveInfinity(f) ? "Infinity" : float.IsNegativeInfinity(f) ? "-Infinity" : (object)f;

            Obj Renderer(Renderer r)
            {
                var o = new Obj { { "node", index[r.transform] }, { "enabled", r.enabled }, { "materials", r.sharedMaterials.Select(m => (object)(m == null ? null : m.name)).ToList() } };
                var filter = r.GetComponent<MeshFilter>();
                var mesh = r is SkinnedMeshRenderer s ? s.sharedMesh : filter != null ? filter.sharedMesh : null;
                if (mesh == null) return o;
                o.Add("mesh", mesh.name);
                o.Add("vertices", mesh.vertexCount);
                o.Add("blendshapes", Enumerable.Range(0, mesh.blendShapeCount).Select(i => (object)mesh.GetBlendShapeName(i)).ToList());
                o.Add("blendshapeFrames", Enumerable.Range(0, mesh.blendShapeCount).Select(i => (object)mesh.GetBlendShapeFrameCount(i)).ToList());
                o.Add("streams", Streams(r, mesh));
                return o;
            }

            List<object> Streams(Renderer r, Mesh mesh)
            {
                int vc = mesh.vertexCount;
                var streams = new List<object>();
                void Put(string name, int components, IEnumerable<float> values)
                {
                    long start = offset;
                    foreach (var v in values) { bin.Write(v); offset += 4; }
                    streams.Add(new Obj { { "name", name }, { "components", components }, { "offset", start }, { "count", (offset - start) / 4 / components } });
                }
                IEnumerable<float> Mirror(Vector3[] v) => v.SelectMany(x => new[] { -x.x, x.y, x.z });
                Put("POSITION", 3, Mirror(mesh.vertices));
                if (mesh.normals.Length == vc) Put("NORMAL", 3, Mirror(mesh.normals));
                if (mesh.tangents.Length == vc) Put("TANGENT", 4, mesh.tangents.SelectMany(t => new[] { -t.x, t.y, t.z, -t.w }));
                var uvs = new List<Vector4>();
                for (int ch = 0; ch < 8; ch++)
                {
                    var attr = UnityEngine.Rendering.VertexAttribute.TexCoord0 + ch;
                    if (!mesh.HasVertexAttribute(attr)) continue;
                    int dim = mesh.GetVertexAttributeDimension(attr);
                    mesh.GetUVs(ch, uvs);
                    if (dim == 1) { Put("_TEXCOORD_" + ch, 1, uvs.Select(u => u.x)); continue; }
                    Put("TEXCOORD_" + ch, 2, uvs.SelectMany(u => new[] { u.x, 1 - u.y }));
                    if (dim > 2) Put($"_TEXCOORD_{ch}_ZW", dim - 2, uvs.SelectMany(u => new[] { u.z, u.w }.Take(dim - 2)));
                }
                if (mesh.HasVertexAttribute(UnityEngine.Rendering.VertexAttribute.Color)) Put("COLOR_0", 4, mesh.colors.SelectMany(c => new[] { c.r, c.g, c.b, c.a }));
                if (r is SkinnedMeshRenderer smr && mesh.bindposes.Length > 0)
                {
                    var bones = smr.bones;
                    var perVertex = mesh.GetBonesPerVertex();
                    var weights = mesh.GetAllBoneWeights();
                    var triples = new List<float>();
                    int at = 0;
                    for (int v = 0; v < perVertex.Length; v++)
                        for (int k = 0; k < perVertex[v]; k++, at++)
                        {
                            var bw = weights[at];
                            if (bw.weight == 0) continue;
                            var bone = bw.boneIndex < bones.Length ? bones[bw.boneIndex] : null;
                            int node = bone != null && index.TryGetValue(bone, out int n) ? n : index[r.transform];
                            triples.Add(v);
                            triples.Add(node);
                            triples.Add(bw.weight);
                        }
                    Put("WEIGHTS", 3, triples);
                }
                for (int sub = 0; sub < mesh.subMeshCount; sub++)
                {
                    var idx = mesh.GetIndices(sub);
                    if (mesh.GetTopology(sub) == MeshTopology.Triangles)
                        for (int i = 0; i + 2 < idx.Length; i += 3) (idx[i + 1], idx[i + 2]) = (idx[i + 2], idx[i + 1]);
                    Put("INDICES_" + sub, 1, idx.Select(i => (float)i));
                }
                var dv = new Vector3[vc];
                var dn = new Vector3[vc];
                var dt = new Vector3[vc];
                for (int s = 0; s < mesh.blendShapeCount; s++)
                {
                    mesh.GetBlendShapeFrameVertices(s, mesh.GetBlendShapeFrameCount(s) - 1, dv, dn, dt);
                    Put($"MORPH_{s}_POSITION", 3, Mirror(dv));
                    if (mesh.normals.Length == vc) Put($"MORPH_{s}_NORMAL", 3, Mirror(dn));
                    if (mesh.tangents.Length == vc) Put($"MORPH_{s}_TANGENT", 3, Mirror(dt));
                }
                return streams;
            }

            void MenuControls(VRCExpressionsMenu menu, HashSet<VRCExpressionsMenu> open, List<object> controls)
            {
                if (menu == null || !open.Add(menu)) return;
                foreach (var c in menu.controls)
                {
                    controls.Add(new Obj { { "name", c.name ?? "" }, { "type", c.type.ToString() }, { "parameter", c.parameter?.name ?? "" } });
                    if (c.type == VRCExpressionsMenu.Control.ControlType.SubMenu) MenuControls(c.subMenu, open, controls);
                }
                open.Remove(menu);
            }

            readonly SortedDictionary<string, int> behaviourTypes = new SortedDictionary<string, int>(StringComparer.Ordinal);

            Obj Layers(List<AnimationClip> clips)
            {
                var o = new Obj();
                foreach (var l in Playable.Layers(descriptor))
                {
                    var runs = Playable.Controller(l);
                    if (runs == null) continue;
                    var overrides = new Dictionary<AnimationClip, AnimationClip>();
                    var controller = runs as AnimatorController;
                    if (runs is AnimatorOverrideController ovr)
                    {
                        controller = ovr.runtimeAnimatorController as AnimatorController;
                        var pairs = new List<KeyValuePair<AnimationClip, AnimationClip>>();
                        ovr.GetOverrides(pairs);
                        foreach (var kv in pairs) if (kv.Value != null) overrides[kv.Key] = kv.Value;
                    }
                    if (controller == null) continue;
                    int states = 0, machines = 0, transitions = 0, otherLayer = 0, trees = 0, behaviours = 0;
                    void Behaviours(StateMachineBehaviour[] list)
                    {
                        foreach (var b in list ?? new StateMachineBehaviour[0])
                        {
                            behaviours++;
                            var type = b == null ? "missing state machine behaviour" : b.GetType().Name;
                            behaviourTypes.TryGetValue(type, out int n);
                            behaviourTypes[type] = n + 1;
                        }
                    }
                    void Motion(Motion m)
                    {
                        if (m is AnimationClip clip)
                        {
                            if (overrides.TryGetValue(clip, out var replaced)) clip = replaced;
                            if (!clips.Contains(clip)) clips.Add(clip);
                        }
                        else if (m is BlendTree tree)
                        {
                            trees++;
                            foreach (var c in tree.children) if (c.motion != null) Motion(c.motion);
                        }
                    }
                    var inLayer = new HashSet<UnityEngine.Object>();
                    void Count(AnimatorTransitionBase[] ts)
                    {
                        transitions += ts.Length;
                        foreach (var t in ts)
                            if ((t.destinationState != null && !inLayer.Contains(t.destinationState)) || (t.destinationStateMachine != null && !inLayer.Contains(t.destinationStateMachine))) otherLayer++;
                    }
                    void Members(AnimatorStateMachine m)
                    {
                        inLayer.Add(m);
                        foreach (var s in m.states) inLayer.Add(s.state);
                        foreach (var sub in m.stateMachines) Members(sub.stateMachine);
                    }
                    void Machine(AnimatorStateMachine m, AnimatorStateMachine parent)
                    {
                        machines++;
                        Behaviours(m.behaviours);
                        Count(m.anyStateTransitions);
                        Count(m.entryTransitions);
                        if (parent != null) Count(parent.GetStateMachineTransitions(m));
                        foreach (var s in m.states)
                        {
                            states++;
                            Behaviours(s.state.behaviours);
                            Count(s.state.transitions);
                            if (s.state.motion != null) Motion(s.state.motion);
                        }
                        foreach (var sub in m.stateMachines) Machine(sub.stateMachine, m);
                    }
                    var layers = controller.layers;
                    foreach (var layer in layers)
                    {
                        if (layer.syncedLayerIndex >= 0 && layer.syncedLayerIndex < layers.Length)
                        {
                            void Synced(AnimatorStateMachine m)
                            {
                                foreach (var s in m.states)
                                {
                                    Behaviours(layer.GetOverrideBehaviours(s.state));
                                    if (layer.GetOverrideMotion(s.state) is Motion om) Motion(om);
                                }
                                foreach (var sub in m.stateMachines) Synced(sub.stateMachine);
                            }
                            Synced(layers[layer.syncedLayerIndex].stateMachine);
                        }
                        else if (layer.stateMachine != null)
                        {
                            inLayer.Clear();
                            Members(layer.stateMachine);
                            Machine(layer.stateMachine, null);
                        }
                    }
                    o.Add(l.type.ToString(), new Obj
                    {
                        { "parameters", controller.parameters.Select(p => (object)p.name).ToList() },
                        { "layers", layers.Length }, { "stateMachines", machines }, { "states", states },
                        { "transitions", transitions }, { "otherLayerTransitions", otherLayer }, { "blendTrees", trees }, { "behaviours", behaviours },
                    });
                }
                return o;
            }

            static Obj ClipSettings(AnimationClip c)
            {
                var s = AnimationUtility.GetAnimationClipSettings(c);
                var o = new Obj { { "name", c.name }, { "loop", s.loopTime }, { "cycleOffset", s.cycleOffset }, { "hasAdditiveReferencePose", s.hasAdditiveReferencePose } };
                if (s.hasAdditiveReferencePose && s.additiveReferencePoseClip != null)
                    o.Add("additiveReferencePose", new Obj { { "clip", s.additiveReferencePoseClip.name }, { "time", s.additiveReferencePoseTime } });
                o.Add("humanMotion", c.humanMotion);
                o.Add("mirror", s.mirror);
                o.Add("loopBlend", s.loopBlend);
                o.Add("loopBlendOrientation", s.loopBlendOrientation);
                o.Add("keepOriginalOrientation", s.keepOriginalOrientation);
                o.Add("orientationOffsetY", s.orientationOffsetY);
                o.Add("loopBlendPositionY", s.loopBlendPositionY);
                o.Add("keepOriginalPositionY", s.keepOriginalPositionY);
                o.Add("heightFromFeet", s.heightFromFeet);
                o.Add("level", s.level);
                o.Add("loopBlendPositionXZ", s.loopBlendPositionXZ);
                o.Add("keepOriginalPositionXZ", s.keepOriginalPositionXZ);
                return o;
            }

            struct Stream : IAnimationJob
            {
                public NativeArray<float> Out;
                public NativeArray<int> Runs;
                public void ProcessRootMotion(AnimationStream stream) { }
                public void ProcessAnimation(AnimationStream stream)
                {
                    if (!stream.isHumanStream) return;
                    var h = stream.AsHuman();
                    var at = new (Vector3 p, Quaternion q)[]
                    {
                        (h.bodyLocalPosition, h.bodyLocalRotation),
                        (h.GetGoalLocalPosition(AvatarIKGoal.LeftFoot), h.GetGoalLocalRotation(AvatarIKGoal.LeftFoot)),
                        (h.GetGoalLocalPosition(AvatarIKGoal.RightFoot), h.GetGoalLocalRotation(AvatarIKGoal.RightFoot)),
                        (h.GetGoalLocalPosition(AvatarIKGoal.LeftHand), h.GetGoalLocalRotation(AvatarIKGoal.LeftHand)),
                        (h.GetGoalLocalPosition(AvatarIKGoal.RightHand), h.GetGoalLocalRotation(AvatarIKGoal.RightHand)),
                    };
                    for (int i = 0; i < at.Length; i++)
                        for (int k = 0; k < 7; k++) Out[i * 7 + k] = k < 3 ? at[i].p[k] : at[i].q[k - 3];
                    Runs[0]++;
                }
            }

            // Measured through a clip holding every muscle at 0, where the exporter plays nothing, so the two agree only
            // if both are Unity's rest.
            Obj HumanRest()
            {
                var animator = root.GetComponent<Animator>();
                if (animator == null || animator.avatar == null || !animator.avatar.isHuman) return null;
                var saved = nodes.Select(t => (t, t.localPosition, t.localRotation, t.localScale)).ToList();
                var (culling, enabled) = (animator.cullingMode, animator.enabled);
                var values = new NativeArray<float>(35, Allocator.TempJob);
                var runs = new NativeArray<int>(1, Allocator.TempJob);
                var clip = new AnimationClip();
                foreach (var m in HumanTrait.MuscleName)
                    clip.SetCurve("", typeof(Animator), System.Text.RegularExpressions.Regex.Replace(m, @"^(Left|Right) (Thumb|Index|Middle|Ring|Little) (\d Stretched|Spread)$", "$1Hand.$2.$3"), AnimationCurve.Constant(0, 1, 0));
                var graph = PlayableGraph.Create("census rest");
                try
                {
                    animator.cullingMode = AnimatorCullingMode.AlwaysAnimate;
                    animator.enabled = true;
                    var reader = AnimationScriptPlayable.Create(graph, new Stream { Out = values, Runs = runs }, 1);
                    graph.Connect(AnimationClipPlayable.Create(graph, clip), 0, reader, 0);
                    reader.SetInputWeight(0, 1);
                    AnimationPlayableOutput.Create(graph, "census rest", animator).SetSourcePlayable(reader);
                    graph.Evaluate(0);
                    if (runs[0] == 0) return null;
                    var names = new[] { "body", "leftFoot", "rightFoot", "leftHand", "rightHand" };
                    var o = new Obj();
                    for (int i = 0; i < names.Length; i++)
                        o.Add(names[i], Enumerable.Range(0, 7).Select(k => (object)values[i * 7 + k]).ToList());
                    return o;
                }
                finally
                {
                    graph.Destroy();
                    values.Dispose();
                    runs.Dispose();
                    UnityEngine.Object.DestroyImmediate(clip);
                    (animator.cullingMode, animator.enabled) = (culling, enabled);
                    foreach (var (t, p, r, sc) in saved) { t.localPosition = p; t.localRotation = r; t.localScale = sc; }
                }
            }

            Obj Curves(List<AnimationClip> clips, Action<Material> use)
            {
                var counts = new SortedDictionary<string, int>(StringComparer.Ordinal);
                void Count(string kind) { counts.TryGetValue(kind, out int n); counts[kind] = n + 1; }
                var parameters = new HashSet<string>(Playable.Layers(descriptor)
                    .SelectMany(l => Playable.Resolved(Playable.Controller(l))?.parameters ?? new AnimatorControllerParameter[0])
                    .Select(p => p.name));
                foreach (var clip in clips)
                {
                    foreach (var b in AnimationUtility.GetCurveBindings(clip))
                    {
                        var t = string.IsNullOrEmpty(b.path) ? root : root.Find(b.path);
                        var p = b.propertyName;
                        var component = typeof(Component).IsAssignableFrom(b.type) ? t?.GetComponent(b.type) : null;
                        if (t == null) Count("missing object");
                        else if (b.type == typeof(GameObject) && p == "m_IsActive") Count("nodeActive");
                        else if (b.type == typeof(Transform) && p.StartsWith("m_LocalPosition.")) Count("translation");
                        else if (b.type == typeof(Transform) && p.StartsWith("m_LocalRotation.")) Count("rotation");
                        else if (b.type == typeof(Transform) && p.StartsWith("m_LocalScale.")) Count("scale");
                        else if (b.type == typeof(Transform) && p.Contains("EulerAngles")) Count("eulerRotation");
                        else if (b.type != typeof(GameObject) && component == null) Count("missing component");
                        else if ((component is SkinnedMeshRenderer || component is MeshRenderer) && p == "m_Enabled") Count("rendererEnabled");
                        else if (component is SkinnedMeshRenderer && b.type == typeof(SkinnedMeshRenderer) && p.StartsWith("blendShape.")) Count("blendShape");
                        else if ((component is SkinnedMeshRenderer || component is MeshRenderer) && p.StartsWith("material.")) Count("materialProperty");
                        else if (b.type == typeof(Animator) && t == root) Count(parameters.Contains(p) ? "animatorParameter" : "muscle");
                        else if (component is VRCPhysBoneBase) Count("physBone");
                        else if (component is VRCConstraintBase || component is IConstraint) Count("constraint");
                        else if (component is ContactSender) Count("contactSender");
                        else if (component is ContactReceiver) Count("contactReceiver");
                        else Count("other " + b.type.Name + "." + p.Split('.')[0]);
                    }
                    foreach (var b in AnimationUtility.GetObjectReferenceCurveBindings(clip))
                    {
                        var t = string.IsNullOrEmpty(b.path) ? root : root.Find(b.path);
                        bool slot = t != null && (b.type == typeof(SkinnedMeshRenderer) || b.type == typeof(MeshRenderer)) && b.propertyName.StartsWith("m_Materials.Array.data[") && t.GetComponent(b.type) != null;
                        bool past = slot && int.TryParse(b.propertyName.Substring("m_Materials.Array.data[".Length).TrimEnd(']'), out int n) && n >= ((Renderer)t.GetComponent(b.type)).sharedMaterials.Length;
                        Count(t == null ? "missing object" : past ? "slot past the renderer's slots" : slot ? "materialSlot" : "other reference " + b.type.Name + "." + b.propertyName.Split('.')[0]);
                        slot &= !past;
                        if (slot) foreach (var k in AnimationUtility.GetObjectReferenceCurve(clip, b)) if (k.value is Material m) use(m);
                    }
                }
                var o = new Obj();
                foreach (var kv in counts) o.Add(kv.Key, kv.Value);
                return o;
            }

            // Material.GetColor can differ from the saved colour in its last bit, as measured on the corpus.
            static Color? SavedColor(SerializedProperty saved, string name)
            {
                var a = saved?.FindPropertyRelative("m_Colors");
                for (int i = 0; a != null && i < a.arraySize; i++)
                {
                    var e = a.GetArrayElementAtIndex(i);
                    if (e.FindPropertyRelative("first").stringValue == name) return e.FindPropertyRelative("second").colorValue;
                }
                return null;
            }

            static Obj MaterialCensus(Material m)
            {
                if (m == null) return new Obj { { "name", null } };
                var saved = new SerializedObject(m).FindProperty("m_SavedProperties");
                var names = new List<object>();
                foreach (var list in new[] { "m_TexEnvs", "m_Ints", "m_Floats", "m_Colors" })
                {
                    var a = saved?.FindPropertyRelative(list);
                    if (a == null) continue;
                    for (int i = 0; i < a.arraySize; i++) names.Add(a.GetArrayElementAtIndex(i).FindPropertyRelative("first").stringValue);
                }
                var values = new Obj();
                var seen = new HashSet<string>();
                var shader = m.shader;
                for (int i = 0; shader != null && i < shader.GetPropertyCount(); i++)
                {
                    var n = shader.GetPropertyName(i);
                    if (!seen.Add(n)) continue;
                    switch (shader.GetPropertyType(i))
                    {
                        case UnityEngine.Rendering.ShaderPropertyType.Float:
                        case UnityEngine.Rendering.ShaderPropertyType.Range: values.Add(n, new object[] { "float", F(m.GetFloat(n)) }); break;
                        case UnityEngine.Rendering.ShaderPropertyType.Int: values.Add(n, new object[] { "int", m.GetInteger(n) }); break;
                        case UnityEngine.Rendering.ShaderPropertyType.Color: var c = SavedColor(saved, n) ?? m.GetColor(n); values.Add(n, new object[] { "color", F(c.r), F(c.g), F(c.b), F(c.a) }); break;
                        case UnityEngine.Rendering.ShaderPropertyType.Vector: var v = m.GetVector(n); values.Add(n, new object[] { "vector", F(v.x), F(v.y), F(v.z), F(v.w) }); break;
                        case UnityEngine.Rendering.ShaderPropertyType.Texture:
                            var t = m.GetTexture(n); var sc = m.GetTextureScale(n); var of = m.GetTextureOffset(n);
                            values.Add(n, new object[] { "texture", t == null ? null : t.GetType().Name, F(sc.x), F(sc.y), F(of.x), F(of.y) });
                            break;
                    }
                }
                var serialized = new List<object>();
                foreach (var list in new[] { "m_ValidKeywords", "m_InvalidKeywords" })
                {
                    var a = new SerializedObject(m).FindProperty(list);
                    for (int i = 0; a != null && i < a.arraySize; i++) serialized.Add(a.GetArrayElementAtIndex(i).stringValue);
                }
                return new Obj
                {
                    { "name", m.name }, { "shader", shader != null ? shader.name : "" }, { "properties", names }, { "values", values },
                    { "keywords", m.shaderKeywords.OrderBy(k => k, StringComparer.Ordinal).Select(k => (object)k).ToList() }, { "serializedKeywords", serialized },
                };
            }
        }
    }
}
