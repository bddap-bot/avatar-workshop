using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text;
using UnityEditor;
using UnityEngine;
using UnityEngine.Animations;
using UnityEngine.Playables;

namespace AvatarWorkshop.Reference
{
    public static class CurveProbe
    {
        public const int Seed = 11, RandomCurves = 80, Samples = 400;

        public static void Record(string output)
        {
            var inv = CultureInfo.InvariantCulture;
            var curves = new List<(Keyframe[] keys, bool loop)>();
            foreach (var e in Fixture.SampleTimeExponents)
            {
                float l = Mathf.Pow(10, e);
                curves.Add((new[] { new Keyframe(0, 0, 1, 1), new Keyframe(l, l, 1, 1) }, true));
            }
            var rng = new System.Random(Seed);
            float[] lengths = { 1, 2.5f, 3, 7, 10, 13.7f, 60, 100, 0.37f, 1000, 12345, 1e5f, 1e7f };
            for (var i = 0; i < RandomCurves; i++)
            {
                var length = rng.Next(4) == 0 ? (float)(0.1 + 50 * rng.NextDouble()) : lengths[rng.Next(lengths.Length)];
                var times = new List<float> { 0, length };
                for (var k = rng.Next(4); k > 0; k--) times.Add((float)(length * rng.NextDouble()));
                times.Sort();
                var keys = new Keyframe[times.Count];
                for (var k = 0; k < keys.Length; k++)
                    keys[k] = new Keyframe(times[k], R(rng, 3), R(rng, 2), R(rng, 2));
                curves.Add((keys, i % 2 == 0));
            }

            Directory.CreateDirectory(output);
            var table = new StringBuilder();
            var samples = new StringBuilder("curve\ttime\tclip\tevaluate\n");
            var go = new GameObject("CurveProbe");
            var animator = go.AddComponent<Animator>();
            for (var c = 0; c < curves.Count; c++)
            {
                var (keys, loop) = curves[c];
                var curve = new AnimationCurve(keys);
                var clip = new AnimationClip();
                AnimationUtility.SetEditorCurve(clip, EditorCurveBinding.FloatCurve("", typeof(Transform), "m_LocalPosition.x"), curve);
                var settings = AnimationUtility.GetAnimationClipSettings(clip);
                settings.loopTime = loop;
                AnimationUtility.SetAnimationClipSettings(clip, settings);
                table.Append(c).Append('\t').Append(loop ? "loop" : "once");
                foreach (var k in keys)
                    table.Append('\t').Append(string.Join(",", new[] { k.time, k.value, k.inTangent, k.outTangent }.Select(v => v.ToString("R", inv))));
                table.Append('\n');

                var graph = PlayableGraph.Create("CurveProbe" + c);
                graph.SetTimeUpdateMode(DirectorUpdateMode.Manual);
                var playable = AnimationClipPlayable.Create(graph, clip);
                AnimationPlayableOutput.Create(graph, "out", animator).SetSourcePlayable(playable);
                graph.Evaluate(0);
                var span = keys[keys.Length - 1].time * (loop ? 3 : 1.5f);
                for (var s = 0; s < Samples; s++)
                {
                    var t = (float)(span * rng.NextDouble());
                    playable.SetTime(t);
                    graph.Evaluate(0);
                    samples.Append(c).Append('\t').Append(t.ToString("R", inv)).Append('\t')
                        .Append(go.transform.localPosition.x.ToString("R", inv)).Append('\t').Append(curve.Evaluate(t).ToString("R", inv)).Append('\n');
                }
                graph.Destroy();
            }
            UnityEngine.Object.DestroyImmediate(go);
            File.WriteAllText(Path.Combine(output, "curves.tsv"), table.ToString());
            File.WriteAllText(Path.Combine(output, "samples.tsv"), samples.ToString());
            File.WriteAllText(Path.Combine(output, "done"), curves.Count.ToString(inv) + "\n");
        }

        static float R(System.Random rng, float range) => (float)Math.Round(range * (2 * rng.NextDouble() - 1), 3);
    }
}
