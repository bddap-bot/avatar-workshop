using System;
using System.Collections.Generic;
using System.IO;
using System.Text;

namespace AvatarWorkshop.Exporter
{
    public sealed class Gltf
    {
        const int ArrayBuffer = 34962, ElementArrayBuffer = 34963;
        public const int Float = 5126, UnsignedShort = 5123, UnsignedInt = 5125;

        readonly MemoryStream bin = new MemoryStream();
        public readonly List<object> BufferViews = new List<object>();
        public readonly List<object> Accessors = new List<object>();
        public readonly List<object> Nodes = new List<object>();
        public readonly List<object> Meshes = new List<object>();
        public readonly List<object> Skins = new List<object>();
        public readonly List<object> Materials = new List<object>();
        public readonly List<object> Textures = new List<object>();
        public readonly List<object> Images = new List<object>();
        public readonly List<object> Samplers = new List<object>();

        int View(byte[] bytes, int? target, int? stride = null)
        {
            Pad();
            var view = new Obj { { "buffer", 0 }, { "byteOffset", (int)bin.Position }, { "byteLength", bytes.Length } };
            if (stride.HasValue) view.Add("byteStride", stride.Value);
            if (target.HasValue) view.Add("target", target.Value);
            bin.Write(bytes, 0, bytes.Length);
            BufferViews.Add(view);
            return BufferViews.Count - 1;
        }

        void Pad()
        {
            while (bin.Position % 4 != 0) bin.WriteByte(0);
        }

        static byte[] Bytes(float[] values)
        {
            var b = new byte[values.Length * 4];
            Buffer.BlockCopy(values, 0, b, 0, b.Length);
            return b;
        }

        static string Type(int components) => components switch { 1 => "SCALAR", 2 => "VEC2", 3 => "VEC3", 4 => "VEC4", 16 => "MAT4", _ => throw new ArgumentException("components") };

        static (float[] min, float[] max) Bounds(float[] values, int components)
        {
            var min = new float[components];
            var max = new float[components];
            for (int c = 0; c < components; c++) { min[c] = float.PositiveInfinity; max[c] = float.NegativeInfinity; }
            for (int i = 0; i < values.Length; i++)
            {
                int c = i % components;
                if (values[i] < min[c]) min[c] = values[i];
                if (values[i] > max[c]) max[c] = values[i];
            }
            return (min, max);
        }

        public int Floats(float[] values, int components, bool bounds = false, bool vertex = true)
        {
            int count = values.Length / components;
            var accessor = new Obj
            {
                { "bufferView", View(Bytes(values), vertex ? ArrayBuffer : (int?)null) },
                { "componentType", Float }, { "count", count }, { "type", Type(components) },
            };
            if (bounds && count > 0)
            {
                var (min, max) = Bounds(values, components);
                accessor.Add("min", min);
                accessor.Add("max", max);
            }
            Accessors.Add(accessor);
            return Accessors.Count - 1;
        }

        public int Joints(ushort[] values)
        {
            var b = new byte[values.Length * 2];
            Buffer.BlockCopy(values, 0, b, 0, b.Length);
            Accessors.Add(new Obj { { "bufferView", View(b, ArrayBuffer) }, { "componentType", UnsignedShort }, { "count", values.Length / 4 }, { "type", "VEC4" } });
            return Accessors.Count - 1;
        }

        public int Indices(int[] values)
        {
            var b = new byte[values.Length * 4];
            Buffer.BlockCopy(values, 0, b, 0, b.Length);
            Accessors.Add(new Obj { { "bufferView", View(b, ElementArrayBuffer) }, { "componentType", UnsignedInt }, { "count", values.Length }, { "type", "SCALAR" } });
            return Accessors.Count - 1;
        }

        // A morph target delta: only the vertices that move are stored.
        public int Sparse(float[] dense, int components, bool bounds)
        {
            int count = dense.Length / components;
            var moved = new List<int>();
            for (int v = 0; v < count; v++)
                for (int c = 0; c < components; c++)
                    if (dense[v * components + c] != 0) { moved.Add(v); break; }
            var accessor = new Obj { { "componentType", Float }, { "count", count }, { "type", Type(components) } };
            if (bounds && count > 0)
            {
                var (min, max) = Bounds(dense, components);
                accessor.Add("min", min);
                accessor.Add("max", max);
            }
            if (moved.Count > 0)
            {
                var indices = moved.ToArray();
                var ib = new byte[indices.Length * 4];
                Buffer.BlockCopy(indices, 0, ib, 0, ib.Length);
                var values = new float[moved.Count * components];
                for (int i = 0; i < moved.Count; i++)
                    Array.Copy(dense, moved[i] * components, values, i * components, components);
                accessor.Add("sparse", new Obj
                {
                    { "count", moved.Count },
                    { "indices", new Obj { { "bufferView", View(ib, null) }, { "componentType", UnsignedInt } } },
                    { "values", new Obj { { "bufferView", View(Bytes(values), null) } } },
                });
            }
            Accessors.Add(accessor);
            return Accessors.Count - 1;
        }

        public int Image(byte[] bytes, string mimeType, string name)
        {
            Images.Add(new Obj { { "name", name }, { "bufferView", View(bytes, null) }, { "mimeType", mimeType } });
            return Images.Count - 1;
        }

        public void WriteGlb(Obj root, string path)
        {
            Pad();
            var asset = new Obj { { "version", "2.0" }, { "generator", "Avatar Workshop Unity exporter " + Exporter.Version } };
            var doc = new Obj { { "asset", asset }, { "scene", 0 }, { "scenes", new[] { new Obj { { "nodes", new[] { 0 } } } } } };
            void Put(string key, List<object> items) { if (items.Count > 0) doc.Add(key, items); }
            Put("nodes", Nodes);
            Put("meshes", Meshes);
            Put("skins", Skins);
            Put("materials", Materials);
            Put("textures", Textures);
            Put("images", Images);
            Put("samplers", Samplers);
            Put("accessors", Accessors);
            Put("bufferViews", BufferViews);
            if (bin.Length > 0) doc.Add("buffers", new[] { new Obj { { "byteLength", (int)bin.Length } } });
            doc.Add("extensionsUsed", new[] { "AW_avatar" });
            doc.Add("extensions", new Obj { { "AW_avatar", root } });

            var json = Encoding.UTF8.GetBytes(Json.Write(doc));
            int jsonLength = (json.Length + 3) / 4 * 4;
            long binLength = bin.Length;
            long total = 12 + 8 + jsonLength + (binLength > 0 ? 8 + binLength : 0);
            if (total > uint.MaxValue) throw new InvalidOperationException("larger than a glTF binary can hold");
            using var file = new FileStream(path, FileMode.Create, FileAccess.Write);
            var w = new BinaryWriter(file);
            w.Write(0x46546C67u);
            w.Write(2u);
            w.Write((uint)total);
            w.Write((uint)jsonLength);
            w.Write(0x4E4F534Au);
            w.Write(json);
            for (int i = json.Length; i < jsonLength; i++) w.Write((byte)' ');
            if (binLength > 0)
            {
                w.Write((uint)binLength);
                w.Write(0x004E4942u);
                w.Flush();
                bin.Position = 0;
                bin.CopyTo(file);
            }
            w.Flush();
        }
    }
}
