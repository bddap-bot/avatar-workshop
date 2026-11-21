using System;
using System.Collections.Generic;
using System.Linq;
using UnityEngine;
using UnityEngine.Experimental.Rendering;

namespace AvatarWorkshop.Exporter
{
    sealed partial class Export
    {
        readonly Dictionary<Cubemap, int> cubemapIndex = new Dictionary<Cubemap, int>();
        readonly List<object> cubemapRecords = new List<object>();
        static (bool x, bool y)? faceFlip;

        void MaterialTexture(Obj property, Texture texture)
        {
            if (texture is Texture2D t) property.Add("texture", Texture(t));
            else if (texture is Cubemap c) property.Add("cubemap", Cubemap(c));
            else if (texture != null) Drop(texture.GetType().Name + " material texture");
        }

        int Cubemap(Cubemap c)
        {
            if (cubemapIndex.TryGetValue(c, out int i)) return i;
            int w = c.width;
            var faces = Enumerable.Range(0, 6).Select(f => Face(c, f)).ToArray();
            var images = new List<object>();
            for (int f = 0; f < 6; f++)
                images.Add(gltf.Image(Png16(faces[f], w), "image/png", $"{c.name} {(CubemapFace)f}"));
            cubemapRecords.Add(new Obj
            {
                { "faces", images },
                { "filter", c.filterMode switch { FilterMode.Point => "point", FilterMode.Trilinear => "trilinear", _ => "bilinear" } },
            });
            i = cubemapRecords.Count - 1;
            cubemapIndex[c] = i;
            return i;
        }

        // A 16-bit RGBA PNG of texels as half-float bits, which keep an HDR cubemap's range and its darks alike, in
        // Unity's row order, its first row last as Unity's own PNG encoder writes a texture.
        // ImageConversion.EncodeArrayToPNG writes 8 bits whatever format it is given (measured).
        static byte[] Png16(Color[] texels, int w)
        {
            int h = texels.Length / w;
            var raw = new byte[h * (1 + w * 8)];
            for (int y = 0; y < h; y++)
            {
                int at = y * (1 + w * 8) + 1;
                for (int x = 0; x < w; x++)
                {
                    var t = texels[(h - 1 - y) * w + x];
                    foreach (var v in new[] { t.r, t.g, t.b, t.a })
                    {
                        ushort q = Mathf.FloatToHalf(v);
                        raw[at++] = (byte)(q >> 8);
                        raw[at++] = (byte)q;
                    }
                }
            }
            var png = new System.IO.MemoryStream();
            png.Write(new byte[] { 0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a }, 0, 8);
            var header = new System.IO.MemoryStream();
            BigEndian(header, (uint)w);
            BigEndian(header, (uint)h);
            header.Write(new byte[] { 16, 6, 0, 0, 0 }, 0, 5);
            Chunk(png, "IHDR", header.ToArray());
            Chunk(png, "IDAT", Zlib(raw));
            Chunk(png, "IEND", new byte[0]);
            return png.ToArray();
        }

        static byte[] Zlib(byte[] data)
        {
            var o = new System.IO.MemoryStream();
            o.WriteByte(0x78);
            o.WriteByte(0x9c);
            using (var deflate = new System.IO.Compression.DeflateStream(o, System.IO.Compression.CompressionLevel.Optimal, true))
                deflate.Write(data, 0, data.Length);
            uint a = 1, b = 0;
            foreach (var d in data) { a = (a + d) % 65521; b = (b + a) % 65521; }
            BigEndian(o, (b << 16) | a);
            return o.ToArray();
        }

        static void Chunk(System.IO.Stream o, string kind, byte[] data)
        {
            BigEndian(o, (uint)data.Length);
            var typed = System.Text.Encoding.ASCII.GetBytes(kind).Concat(data).ToArray();
            o.Write(typed, 0, typed.Length);
            uint crc = 0xffffffff;
            foreach (var d in typed)
            {
                crc ^= d;
                for (int k = 0; k < 8; k++) crc = (crc & 1) != 0 ? (crc >> 1) ^ 0xedb88320 : crc >> 1;
            }
            BigEndian(o, ~crc);
        }

        static void BigEndian(System.IO.Stream o, uint v)
        {
            o.WriteByte((byte)(v >> 24));
            o.WriteByte((byte)(v >> 16));
            o.WriteByte((byte)(v >> 8));
            o.WriteByte((byte)v);
        }

        // A face's linear texels in Cubemap.GetPixels order: the GPU's read turned by the flip measured on a cubemap of
        // known texels.
        static Color[] Face(Texture cube, int face)
        {
            var flip = faceFlip ??= MeasureFlip();
            var read = GpuFace(cube, face);
            int w = cube.width;
            var o = new Color[read.Length];
            for (int y = 0; y < w; y++)
                for (int x = 0; x < w; x++)
                    o[y * w + x] = read[(flip.y ? w - 1 - y : y) * w + (flip.x ? w - 1 - x : x)];
            return o;
        }

        // ConvertTexture reads nothing on some devices (measured: zeros under Mesa's OpenGL), so a face is copied as
        // it is stored into a 2D texture, which Blit decodes from any format and colour space.
        static Color[] GpuFace(Texture cube, int face)
        {
            int w = cube.width;
            var stored = new Texture2D(w, w, cube.graphicsFormat, TextureCreationFlags.None);
            var rt = RenderTexture.GetTemporary(new RenderTextureDescriptor(w, w, RenderTextureFormat.ARGBFloat, 0) { sRGB = false });
            var read = new Texture2D(w, w, TextureFormat.RGBAFloat, false, true);
            var previous = RenderTexture.active;
            try
            {
                Graphics.CopyTexture(cube, face, 0, stored, 0, 0);
                Graphics.Blit(stored, rt);
                RenderTexture.active = rt;
                read.ReadPixels(new Rect(0, 0, w, w), 0, 0);
                read.Apply();
                return read.GetPixels();
            }
            finally
            {
                RenderTexture.active = previous;
                RenderTexture.ReleaseTemporary(rt);
                UnityEngine.Object.DestroyImmediate(read);
                UnityEngine.Object.DestroyImmediate(stored);
            }
        }

        static (bool, bool) MeasureFlip()
        {
            const int w = 4;
            var probe = new Cubemap(w, TextureFormat.RGBAFloat, false, true);
            try
            {
                var known = new Color[6][];
                for (int f = 0; f < 6; f++)
                {
                    known[f] = Enumerable.Range(0, w * w).Select(p => new Color((p % w + 1) / 8f, (p / w + 1) / 8f, (f + 1) / 8f, 1)).ToArray();
                    probe.SetPixels(known[f], (CubemapFace)f);
                }
                probe.Apply(false);
                foreach (var (x, y) in new[] { (false, false), (false, true), (true, false), (true, true) })
                {
                    bool all = true;
                    for (int f = 0; f < 6 && all; f++)
                    {
                        var read = GpuFace(probe, f);
                        for (int py = 0; py < w && all; py++)
                            for (int px = 0; px < w && all; px++)
                            {
                                var a = read[(y ? w - 1 - py : py) * w + (x ? w - 1 - px : px)];
                                var b = known[f][py * w + px];
                                all = Mathf.Abs(a.r - b.r) < 1e-3f && Mathf.Abs(a.g - b.g) < 1e-3f && Mathf.Abs(a.b - b.b) < 1e-3f;
                            }
                    }
                    if (all) return (x, y);
                }
                throw new InvalidOperationException("this graphics device reads cubemap faces in no orientation Cubemap.GetPixels agrees with");
            }
            finally { UnityEngine.Object.DestroyImmediate(probe); }
        }
    }
}
