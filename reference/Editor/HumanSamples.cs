using System;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;

namespace AvatarWorkshop.Reference
{
    // Poses the fixture through Unity's HumanPoseHandler and writes what each pose leaves on the humanoid bones:
    // every muscle alone at eight values, pairs of muscles, then random poses with the body moved and turned.
    // usage: Unity -batchmode -projectPath <project> -executeMethod AvatarWorkshop.Reference.HumanSamples.Run, with
    // AW_OUT naming the output directory, in a project that has the VRChat SDK.
    public static class HumanSamples
    {
        static readonly CultureInfo inv = CultureInfo.InvariantCulture;
        static readonly float[] Sweep = { -1f, -0.5f, -0.25f, -0.1f, 0.1f, 0.25f, 0.5f, 1f };
        const int Random = 400;

        static string F(float v) => v.ToString("R", inv);
        static string Xf(Vector3 p, Quaternion q) => $"{F(p.x)},{F(p.y)},{F(p.z)},{F(q.x)},{F(q.y)},{F(q.z)},{F(q.w)}";

        public static void Run()
        {
            var output = Harness.Env("AW_OUT") ?? throw new Exception("AW_OUT is required");
            try
            {
                Directory.CreateDirectory(output);
                EditorSceneManager.OpenScene(Fixture.Build(Fixture.Kind.Fixture), OpenSceneMode.Single);
                var animator = UnityEngine.Object.FindObjectsOfType<Animator>(true).Single(a => a.avatar != null && a.avatar.isHuman);
                var root = animator.transform;
                root.SetPositionAndRotation(Vector3.zero, Quaternion.identity);
                var handler = new HumanPoseHandler(animator.avatar, root);
                var rest = new HumanPose();
                handler.GetHumanPose(ref rest);
                var bones = Enumerable.Range(0, (int)HumanBodyBones.LastBone).Select(b => ((HumanBodyBones)b, animator.GetBoneTransform((HumanBodyBones)b)))
                    .Where(x => x.Item2 != null).ToArray();
                var rnd = new System.Random(12345);
                int count = HumanTrait.MuscleCount;
                int sweeps = count * Sweep.Length, pairs = count * Sweep.Length;
                var w = new StringBuilder("sample\tkind\tmuscles (in HumanTrait.MuscleName order)\tbody\tbone local poses (HumanBodyBones name=px,py,pz,qx,qy,qz,qw)\n");
                for (int s = 0; s < sweeps + pairs + Random; s++)
                {
                    var pose = new HumanPose { muscles = new float[count], bodyPosition = rest.bodyPosition, bodyRotation = rest.bodyRotation };
                    string kind;
                    if (s < sweeps)
                    {
                        kind = "sweep";
                        pose.muscles[s / Sweep.Length] = Sweep[s % Sweep.Length];
                    }
                    else if (s < sweeps + pairs)
                    {
                        kind = "pair";
                        int p = (s - sweeps) / Sweep.Length;
                        pose.muscles[p] = ((s - sweeps) % Sweep.Length - 3.5f) / 3.5f;
                        pose.muscles[(p * 7 + 3) % count == p ? (p + 1) % count : (p * 7 + 3) % count] = 0.6f;
                    }
                    else
                    {
                        kind = "random";
                        for (int m = 0; m < count; m++)
                            pose.muscles[m] = rnd.NextDouble() < 0.15 ? (float)(rnd.NextDouble() * 2 - 1) : 0f;
                        pose.bodyPosition += new Vector3((float)(rnd.NextDouble() - 0.5), (float)(rnd.NextDouble() - 0.5), (float)(rnd.NextDouble() - 0.5)) * 0.4f;
                        pose.bodyRotation = Quaternion.Euler((float)(rnd.NextDouble() * 60 - 30), (float)(rnd.NextDouble() * 360), (float)(rnd.NextDouble() * 60 - 30));
                    }
                    handler.SetHumanPose(ref pose);
                    w.Append($"{s}\t{kind}\t{string.Join(",", pose.muscles.Select(F))}\t{Xf(pose.bodyPosition, pose.bodyRotation)}\t");
                    w.Append(string.Join("\t", bones.Select(b => $"{b.Item1}={Xf(b.Item2.localPosition, b.Item2.localRotation)}"))).Append('\n');
                }
                File.WriteAllText(Path.Combine(output, "humansamples.tsv"), w.ToString());
                File.WriteAllText(Path.Combine(output, "muscles.tsv"), string.Concat(HumanTrait.MuscleName.Select(n => Fixture.Binding(n) + "\n")));
                EditorApplication.Exit(0);
            }
            catch (Exception e)
            {
                File.WriteAllText(Path.Combine(output, "error.txt"), e.ToString());
                EditorApplication.Exit(1);
            }
        }
    }
}
