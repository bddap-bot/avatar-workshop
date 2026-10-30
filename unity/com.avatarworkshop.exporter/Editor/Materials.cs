using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text.RegularExpressions;
using UnityEditor;
using UnityEngine;
using UnityEngine.Rendering;
using UnityEngine.Experimental.Rendering;

namespace AvatarWorkshop.Exporter
{
    sealed partial class Export
    {
        readonly Dictionary<Material, int> materialIndex = new Dictionary<Material, int>();
        readonly List<object> materialRecords = new List<object>();
        int? missingMaterial;
        readonly Dictionary<Texture2D, int> textureIndex = new Dictionary<Texture2D, int>();

        int Material(Material m)
        {
            if (m == null)
            {
                if (missingMaterial == null)
                {
                    Drop("empty material slot");
                    missingMaterial = AddMaterial(new Obj { { "name", "missing" } }, new Obj
                    {
                        { "shader", "unknown" }, { "shaderName", "" }, { "properties", new Obj() }, { "keywords", new object[0] },
                        { "renderQueue", 2000 }, { "cull", "back" }, { "depthWrite", true }, { "depthTest", "lessEqual" },
                    });
                }
                return missingMaterial.Value;
            }
            if (materialIndex.TryGetValue(m, out int i)) return i;
            var (record, fallback) = MaterialRecord(m);
            i = AddMaterial(fallback, record);
            materialIndex[m] = i;
            return i;
        }

        int AddMaterial(Obj fallback, Obj record)
        {
            gltf.Materials.Add(fallback);
            materialRecords.Add(record);
            return gltf.Materials.Count - 1;
        }

        static string Family(string shader)
        {
            var s = shader.ToLowerInvariant();
            if (s.Contains("poiyomi")) return "poiyomi";
            if (s.Contains("liltoon")) return "liltoon";
            if (s.StartsWith("vrchat/mobile/")) return "vrchat-mobile";
            if (s.Contains("mtoon")) return "mtoon";
            if (shader == "Standard" || shader == "Standard (Specular setup)") return "standard";
            return "unknown";
        }

        (Obj record, Obj fallback) MaterialRecord(Material m)
        {
            var shader = m.shader;
            var shaderName = shader != null ? shader.name : "";
            var family = Family(shaderName);
            if (family == "unknown") Drop("unknown shader " + shaderName);
            var props = new SortedDictionary<string, object>(StringComparer.Ordinal);
            var floats = new Dictionary<string, float>();
            var saved = new SerializedObject(m).FindProperty("m_SavedProperties");
            foreach (var (name, value) in Pairs(saved, "m_Floats"))
            {
                float f = value.floatValue;
                floats[name] = f;
                props[name] = new Obj { { "float", f } };
            }
            foreach (var (name, value) in Pairs(saved, "m_Ints"))
            {
                floats[name] = value.intValue;
                props[name] = new Obj { { "int", value.intValue } };
            }
            foreach (var (name, value) in Pairs(saved, "m_Colors"))
            {
                var c = value.colorValue;
                bool vector = shader != null && shader.FindPropertyIndex(name) is int pi && pi >= 0 && shader.GetPropertyType(pi) == ShaderPropertyType.Vector;
                props[name] = new Obj { { vector ? "vector" : "color", new[] { c.r, c.g, c.b, c.a } } };
            }
            foreach (var (name, value) in Pairs(saved, "m_TexEnvs"))
            {
                var tex = value.FindPropertyRelative("m_Texture").objectReferenceValue as Texture;
                var scale = value.FindPropertyRelative("m_Scale").vector2Value;
                var offset = value.FindPropertyRelative("m_Offset").vector2Value;
                var tp = new Obj();
                if (tex is Texture2D t2) tp.Add("texture", Texture(t2));
                else if (tex != null) Drop(tex.GetType().Name + " material texture");
                tp.Add("tiling", new[] { scale.x, scale.y });
                tp.Add("offset", new[] { offset.x, offset.y });
                props[name] = new Obj { { "texture", tp } };
            }
            var properties = new Obj();
            foreach (var kv in props) properties.Add(kv.Key, kv.Value);

            var state = RenderState(shader, floats);
            var record = new Obj { { "shader", family }, { "shaderName", shaderName } };
            if (ShaderVersion(shader, m) is string version) record.Add("version", version);
            record.Add("properties", properties);
            record.Add("keywords", m.shaderKeywords.OrderBy(k => k, StringComparer.Ordinal).ToArray());
            record.Add("renderQueue", m.renderQueue);
            record.Add("cull", state.cull);
            if (state.blend != null) record.Add("blend", state.blend);
            record.Add("depthWrite", state.zwrite);
            record.Add("depthTest", state.ztest);

            var fallback = new Obj { { "name", m.name } };
            var pbr = new Obj();
            if (m.HasProperty("_Color"))
            {
                var c = m.GetColor("_Color");
                pbr.Add("baseColorFactor", new[] { Mathf.Clamp01(c.r), Mathf.Clamp01(c.g), Mathf.Clamp01(c.b), Mathf.Clamp01(c.a) });
            }
            if (m.HasProperty("_MainTex") && m.GetTexture("_MainTex") is Texture2D main) pbr.Add("baseColorTexture", new Obj { { "index", Texture(main) } });
            pbr.Add("metallicFactor", 0f);
            fallback.Add("pbrMetallicRoughness", pbr);
            if (state.cull == "off") fallback.Add("doubleSided", true);
            if (state.blend != null) fallback.Add("alphaMode", "BLEND");
            return (record, fallback);
        }

        static IEnumerable<(string, SerializedProperty)> Pairs(SerializedProperty saved, string list)
        {
            var array = saved?.FindPropertyRelative(list);
            if (array == null) yield break;
            for (int i = 0; i < array.arraySize; i++)
            {
                var e = array.GetArrayElementAtIndex(i);
                yield return (e.FindPropertyRelative("first").stringValue, e.FindPropertyRelative("second"));
            }
        }

        static readonly Regex VersionNumber = new Regex(@"\d+\.\d+(\.\d+)?");

        static string ShaderVersion(Shader shader, Material m)
        {
            if (shader == null) return null;
            int i = shader.FindPropertyIndex("shader_master_label");
            if (i >= 0 && VersionNumber.Match(shader.GetPropertyDescription(i)) is Match match && match.Success) return match.Value;
            if (m.HasProperty("_lilToonVersion")) return ((int)m.GetFloat("_lilToonVersion")).ToString();
            return null;
        }

        sealed class State
        {
            public string cull = "back", ztest = "lessEqual";
            public bool zwrite = true;
            public Obj blend;
        }

        static readonly string[] BlendFactors = { "zero", "one", "dstColor", "srcColor", "oneMinusDstColor", "srcAlpha", "oneMinusSrcColor", "dstAlpha", "oneMinusDstAlpha", "srcAlphaSaturate", "oneMinusSrcAlpha" };
        static readonly string[] BlendOps = { "add", "subtract", "reverseSubtract", "min", "max" };
        static readonly string[] Compare = { "always", "never", "less", "equal", "lessEqual", "greater", "notEqual", "greaterEqual", "always" };

        // The state of the pass that draws the material, as Unity parsed it from the shader (subshader-level
        // state included), each value bound to a property read from the material's floats.
        State RenderState(Shader shader, Dictionary<string, float> floats)
        {
            var state = new State();
            var pass = ParsedPass(shader);
            if (pass == null)
            {
                Drop("render state of shader " + (shader != null ? shader.name : "none") + " not readable");
                return state;
            }
            int Value(string field, int unread)
            {
                var p = pass.FindPropertyRelative("m_State." + field);
                var val = p?.FindPropertyRelative("val");
                if (val == null) { Drop("render state field " + field + " not readable"); return unread; }
                var name = p.FindPropertyRelative("name")?.stringValue;
                if (!string.IsNullOrEmpty(name) && name != "<noninit>" && floats.TryGetValue(name, out float bound)) return (int)bound;
                return (int)val.floatValue;
            }
            int Ranged(string field, int unread, int max)
            {
                int v = Value(field, unread);
                if (v >= 0 && v <= max) return v;
                Drop("render state " + field + " out of range");
                return unread;
            }
            if (Value("rtBlend0.colMask", 15) != 15) Drop("colour write mask");
            if (Value("offsetFactor", 0) != 0 || Value("offsetUnits", 0) != 0) Drop("depth offset");
            if (Value("alphaToMask", 0) != 0) Drop("alpha to coverage");
            if (Value("stencilOp.comp", 8) != 8 || Value("stencilOp.pass", 0) != 0 || Value("stencilOpFront.comp", 8) != 8 || Value("stencilOpFront.pass", 0) != 0
                || Value("stencilOpBack.comp", 8) != 8 || Value("stencilOpBack.pass", 0) != 0) Drop("stencil");
            state.cull = new[] { "off", "front", "back" }[Ranged("culling", 2, 2)];
            state.zwrite = Value("zWrite", 1) != 0;
            state.ztest = Compare[Ranged("zTest", 4, 8)];
            string Factor(string field, int unread) => BlendFactors[Ranged("rtBlend0." + field, unread, 10)];
            string Op(string field)
            {
                int v = Value("rtBlend0." + field, 0);
                if (v > 4) Drop("logical or advanced blend op");
                else if (v < 0) Drop("render state rtBlend0." + field + " out of range");
                return BlendOps[v >= 0 && v <= 4 ? v : 0];
            }
            var b = new Obj
            {
                { "srcColor", Factor("srcBlend", 1) }, { "dstColor", Factor("destBlend", 0) }, { "srcAlpha", Factor("srcBlendAlpha", 1) }, { "dstAlpha", Factor("destBlendAlpha", 0) },
                { "colorOp", Op("blendOp") }, { "alphaOp", Op("blendOpAlpha") },
            };
            bool opaque = b.All(kv => kv.Key.StartsWith("src") ? (string)kv.Value == "one" : kv.Key.StartsWith("dst") ? (string)kv.Value == "zero" : (string)kv.Value == "add");
            if (!opaque) state.blend = b;
            return state;
        }

        static SerializedProperty ParsedPass(Shader shader)
        {
            if (shader == null) return null;
            var data = ShaderUtil.GetShaderData(shader);
            int sub = data != null ? data.ActiveSubshaderIndex : -1;
            var subshader = sub >= 0 ? data.ActiveSubshader : null;
            if (subshader == null || subshader.PassCount == 0) return null;
            string Mode(int i) => subshader.GetPass(i).FindTagValue(new ShaderTagId("LightMode")).name;
            var passes = Enumerable.Range(0, subshader.PassCount);
            int chosen = passes.Where(i => Mode(i) == "ForwardBase" || Mode(i) == "UniversalForward").DefaultIfEmpty(
                passes.Where(i => string.IsNullOrEmpty(Mode(i)) || Mode(i) == "Always" || Mode(i) == "SRPDefaultUnlit").DefaultIfEmpty(0).First()).First();
            var pass = new SerializedObject(shader).FindProperty($"m_ParsedForm.m_SubShaders.Array.data[{sub}].m_Passes.Array.data[{chosen}]");
            return pass?.FindPropertyRelative("m_State.culling") != null ? pass : null;
        }

        int Texture(Texture2D t)
        {
            if (textureIndex.TryGetValue(t, out int i)) return i;
            var path = AssetDatabase.GetAssetPath(t);
            var ext = System.IO.Path.GetExtension(path).ToLowerInvariant();
            var importer = AssetImporter.GetAtPath(path) as TextureImporter;
            if (importer != null ? !importer.sRGBTexture && importer.textureType != TextureImporterType.NormalMap : !GraphicsFormatUtility.IsSRGBFormat(t.graphicsFormat))
                Drop("linear (non-sRGB) texture");
            int image;
            if ((ext == ".png" || ext == ".jpg" || ext == ".jpeg") && File.Exists(path) && AssetDatabase.IsMainAsset(t) && !(importer != null && importer.convertToNormalmap))
                image = gltf.Image(File.ReadAllBytes(path), ext == ".png" ? "image/png" : "image/jpeg", t.name);
            else
                image = gltf.Image(Encode(t), "image/png", t.name);
            gltf.Samplers.Add(new Obj
            {
                { "magFilter", t.filterMode == FilterMode.Point ? 9728 : 9729 },
                { "minFilter", t.filterMode == FilterMode.Point ? (t.mipmapCount > 1 ? 9984 : 9728) : (t.mipmapCount > 1 ? 9987 : 9729) },
                { "wrapS", Wrap(t.wrapModeU) }, { "wrapT", Wrap(t.wrapModeV) },
            });
            gltf.Textures.Add(new Obj { { "source", image }, { "sampler", gltf.Samplers.Count - 1 }, { "name", t.name } });
            i = gltf.Textures.Count - 1;
            textureIndex[t] = i;
            return i;
        }

        // An imported normal map holds x in alpha (or red) and y in green; Unity's UnpackNormal rebuilds z.
        static Color32[] Unpacked(Color32[] packed)
        {
            var o = new Color32[packed.Length];
            for (int i = 0; i < packed.Length; i++)
            {
                float x = packed[i].r / 255f * (packed[i].a / 255f) * 2 - 1;
                float y = packed[i].g / 255f * 2 - 1;
                float z = Mathf.Sqrt(Mathf.Clamp01(1 - x * x - y * y));
                o[i] = new Color32((byte)Mathf.RoundToInt((x + 1) * 127.5f), (byte)Mathf.RoundToInt((y + 1) * 127.5f), (byte)Mathf.RoundToInt((z + 1) * 127.5f), 255);
            }
            return o;
        }

        static int Wrap(TextureWrapMode w) => w switch { TextureWrapMode.Clamp => 33071, TextureWrapMode.Mirror => 33648, _ => 10497 };

        // A texture with no PNG or JPEG source file is drawn into an 8-bit render target and stored as PNG.
        byte[] Encode(Texture2D t)
        {
            var importer = AssetImporter.GetAtPath(AssetDatabase.GetAssetPath(t)) as TextureImporter;
            bool normal = importer != null && importer.textureType == TextureImporterType.NormalMap;
            bool linear = normal || (importer != null ? !importer.sRGBTexture : !GraphicsFormatUtility.IsSRGBFormat(t.graphicsFormat));
            if (GraphicsFormatUtility.IsHDRFormat(t.graphicsFormat)) Drop("HDR texture stored as 8-bit PNG");
            var rt = RenderTexture.GetTemporary(t.width, t.height, 0, RenderTextureFormat.ARGB32, linear ? RenderTextureReadWrite.Linear : RenderTextureReadWrite.sRGB);
            var previous = RenderTexture.active;
            try
            {
                Graphics.Blit(t, rt);
                RenderTexture.active = rt;
                var copy = new Texture2D(t.width, t.height, TextureFormat.RGBA32, false, linear);
                copy.ReadPixels(new Rect(0, 0, t.width, t.height), 0, 0);
                if (normal) copy.SetPixels32(Unpacked(copy.GetPixels32()));
                copy.Apply();
                var png = copy.EncodeToPNG();
                UnityEngine.Object.DestroyImmediate(copy);
                return png;
            }
            finally
            {
                RenderTexture.active = previous;
                RenderTexture.ReleaseTemporary(rt);
            }
        }
    }
}
