using System;
using System.Collections.Generic;
using System.Linq;
using UnityEditor;
using UnityEngine;

namespace AvatarWorkshop.Exporter
{
    public static partial class Exporter
    {
        // Fields every serialized Unity object or component carries, which say nothing about the component.
        public static readonly HashSet<string> ObjectFields = new HashSet<string>
        {
            "m_ObjectHideFlags", "m_CorrespondingSourceObject", "m_PrefabInstance", "m_PrefabAsset", "m_GameObject", "m_Script", "m_EditorHideFlags", "m_EditorClassIdentifier",
        };

        static HashSet<string> rendererFields;

        // The serialized fields every Renderer has, read from a MeshRenderer: a ParticleSystemRenderer's own are the rest.
        public static HashSet<string> RendererFields()
        {
            if (rendererFields != null) return rendererFields;
            var probe = new GameObject("aw renderer fields") { hideFlags = HideFlags.HideAndDontSave };
            try
            {
                rendererFields = new HashSet<string>();
                var it = new SerializedObject(probe.AddComponent<MeshRenderer>()).GetIterator();
                for (bool more = it.Next(true); more; more = it.Next(false)) rendererFields.Add(it.name);
            }
            finally { UnityEngine.Object.DestroyImmediate(probe); }
            return rendererFields;
        }
    }

    sealed partial class Export
    {
        readonly Dictionary<(Mesh, Material), int> particleMeshes = new Dictionary<(Mesh, Material), int>();

        List<object> ParticleSystems()
        {
            var records = new List<object>();
            foreach (var t in nodes)
            {
                var system = t.GetComponent<ParticleSystem>();
                if (system == null) continue;
                var renderer = t.GetComponent<ParticleSystemRenderer>();
                records.Add(new Obj
                {
                    { "node", nodeIndex[t] },
                    { "system", Serialized(new SerializedObject(system), new HashSet<string>(), renderer) },
                    { "renderer", Serialized(new SerializedObject(renderer), Exporter.RendererFields(), renderer) },
                });
            }
            return records;
        }

        Obj Serialized(SerializedObject so, HashSet<string> skip, ParticleSystemRenderer renderer)
        {
            var o = new Obj();
            var it = so.GetIterator();
            for (bool more = it.Next(true); more; more = it.Next(false))
            {
                if (Exporter.ObjectFields.Contains(it.name) || skip.Contains(it.name)) continue;
                if (Value(it.Copy(), it.name, renderer) is object v) o.Add(it.name, v);
            }
            return o;
        }

        object Value(SerializedProperty p, string path, ParticleSystemRenderer renderer)
        {
            switch (p.propertyType)
            {
                case SerializedPropertyType.Boolean: return new Obj { { "bool", p.boolValue } };
                case SerializedPropertyType.Integer:
                case SerializedPropertyType.ArraySize:
                case SerializedPropertyType.Character: return new Obj { { "int", p.longValue } };
                case SerializedPropertyType.Enum:
                case SerializedPropertyType.LayerMask: return new Obj { { "int", p.intValue } };
                case SerializedPropertyType.Float: return new Obj { { "float", p.floatValue } };
                case SerializedPropertyType.Vector2: return new Obj { { "vector2", new[] { p.vector2Value.x, p.vector2Value.y } } };
                case SerializedPropertyType.Vector3:
                    var v = p.vector3Value;
                    return new Obj { { "vector3", new[] { v.x, v.y, v.z } } };
                case SerializedPropertyType.Color: return new Obj { { "color", new[] { p.colorValue.r, p.colorValue.g, p.colorValue.b, p.colorValue.a } } };
                case SerializedPropertyType.String: return new Obj { { "string", p.stringValue } };
                case SerializedPropertyType.AnimationCurve: return new Obj { { "curve", Curve(p.animationCurveValue) } };
                case SerializedPropertyType.Gradient: return new Obj { { "gradient", Gradient(p.gradientValue) } };
                case SerializedPropertyType.ObjectReference: return Reference(p.objectReferenceValue, renderer);
                case SerializedPropertyType.Generic when p.isArray:
                    var items = new List<object>();
                    for (int i = 0; i < p.arraySize; i++)
                        if (Value(p.GetArrayElementAtIndex(i), $"{path}.Array.data[{i}]", renderer) is object item) items.Add(item);
                    return new Obj { { "array", items } };
                case SerializedPropertyType.Generic:
                    var fields = new Obj();
                    var end = p.GetEndProperty();
                    var c = p.Copy();
                    for (bool more = c.Next(true); more && !SerializedProperty.EqualContents(c, end); more = c.Next(false))
                        if (Value(c.Copy(), path + "." + c.name, renderer) is object field) fields.Add(c.name, field);
                    return new Obj { { "struct", fields } };
                default:
                    Drop("particle system serialized " + p.propertyType);
                    return null;
            }
        }

        static Obj Gradient(Gradient g) => new Obj
        {
            { "mode", g.mode switch { GradientMode.Fixed => "fixed", GradientMode.PerceptualBlend => "perceptualBlend", _ => "blend" } },
            { "colorKeys", g.colorKeys.Select(k => (object)new Obj { { "time", k.time }, { "color", new[] { k.color.r, k.color.g, k.color.b } } }).ToList() },
            { "alphaKeys", g.alphaKeys.Select(k => (object)new Obj { { "time", k.time }, { "alpha", k.alpha } }).ToList() },
        };

        Obj Reference(UnityEngine.Object o, ParticleSystemRenderer renderer)
        {
            switch (o)
            {
                case null: return new Obj { { "null", new Obj() } };
                case GameObject g when Node(g.transform) is int n: return new Obj { { "node", n } };
                case Component c when Node(c.transform) is int n: return new Obj { { "node", n } };
                case Mesh m: return new Obj { { "mesh", ParticleMesh(m, renderer) } };
                case Texture2D t: return new Obj { { "texture", Texture(t) } };
                case GameObject _:
                case Component _:
                    Drop("particle system reference outside the avatar");
                    return new Obj { { "null", new Obj() } };
                default:
                    Drop("particle system reference to a " + o.GetType().Name);
                    return new Obj { { "null", new Obj() } };
            }
        }

        // A mesh a particle system draws or emits from: its primitives take the particle material, as Unity draws them.
        int ParticleMesh(Mesh mesh, ParticleSystemRenderer renderer)
        {
            var material = renderer != null ? renderer.sharedMaterial : null;
            if (particleMeshes.TryGetValue((mesh, material), out int i)) return i;
            i = WriteMesh(mesh, Enumerable.Repeat(material, Math.Max(mesh.subMeshCount, 1)).ToArray(), null);
            particleMeshes[(mesh, material)] = i;
            return i;
        }
    }
}
