using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Text;
using Lyuma.Av3Emulator.Runtime;
using Unity.Collections;
using UnityEditor;
using UnityEditor.Animations;
using UnityEngine;
using UnityEngine.Animations;
using UnityEngine.Playables;
using VRC.SDK3.Avatars.Components;

namespace AvatarWorkshop.Reference
{
    // Plays each clip a recording's playables played alone, in a one-clip playable graph on the same build of the same
    // avatar, at every state time the recording's clocks held it, so a clip's own sampling can be told apart from how
    // the animator's layers and the human stream compose it.
    class ClipSampler
    {
        public bool Finished;
        readonly string output, recording;
        readonly CultureInfo inv = CultureInfo.InvariantCulture;
        int waited, attached;

        public ClipSampler(string output, string recording)
        {
            this.output = output;
            this.recording = recording;
        }

        string F(float v) => v.ToString("R", inv);
        string D(double v) => v.ToString("R", inv);

        public void Tick()
        {
            var runtime = UnityEngine.Object.FindObjectsOfType<LyumaAv3Runtime>().FirstOrDefault(r => !r.IsMirrorClone && !r.IsShadowClone && r.GetComponent<VRCAvatarDescriptor>() != null);
            if (runtime == null)
            {
                if (++waited > 600) throw new Exception("Av3Emulator never attached a runtime to the avatar");
                return;
            }
            if (++attached < 5) return;
            Finished = true;
            Sample(runtime);
            File.WriteAllText(Path.Combine(output, "done"), "\n");
            Harness.Finish(0);
        }

        // The controller each of the emulator's playables plays, in the order it builds them with no layer duplicated
        // for debugging: the Base and Additive layers, the special layers, then the other base layers.
        static AnimatorController[] Controllers(LyumaAv3Runtime runtime, List<AnimatorControllerPlayable> playables)
        {
            if (runtime.DebugDuplicateAnimator != VRCAvatarDescriptor.AnimLayerType.Base) throw new Exception("the emulator duplicates a layer for debugging");
            var d = runtime.GetComponent<VRCAvatarDescriptor>();
            bool Front(VRCAvatarDescriptor.CustomAnimLayer l) => l.type == VRCAvatarDescriptor.AnimLayerType.Base || l.type == VRCAvatarDescriptor.AnimLayerType.Additive;
            var layers = d.baseAnimationLayers.Where(Front).Concat(d.specialAnimationLayers).Concat(d.baseAnimationLayers.Where(l => !Front(l))).ToList();
            if (layers.Count != playables.Count) throw new Exception($"the emulator has {playables.Count} playables for {layers.Count} layers");
            var result = new AnimatorController[layers.Count];
            for (int i = 0; i < layers.Count; i++)
            {
                var rac = layers[i].isDefault ? LyumaAv3Runtime.animLayerToDefaultController[layers[i].type] : layers[i].animatorController;
                if (rac == null)
                {
                    if (playables[i].IsValid()) throw new Exception($"playable {i} plays a controller its layer does not name");
                    continue;
                }
                var c = rac as AnimatorController ?? throw new Exception($"playable {i} plays {rac.GetType().Name}, not an AnimatorController");
                var names = c.layers.Select(l => l.name).ToArray();
                if (!playables[i].IsValid() || playables[i].GetLayerCount() != names.Length || Enumerable.Range(0, names.Length).Any(l => playables[i].GetLayerName(l) != names[l]))
                    throw new Exception($"playable {i}'s layers are not those of the {layers[i].type} controller");
                result[i] = c;
            }
            return result;
        }

        // Each state of a layer by the name the recorder writes: the layer's name, then each sub-machine's and the state's.
        static Dictionary<string, (Motion, AnimatorState)> Motions(AnimatorController c, int layer)
        {
            var l = c.layers[layer];
            var synced = l.syncedLayerIndex >= 0;
            var machine = synced ? c.layers[l.syncedLayerIndex].stateMachine : l.stateMachine;
            var result = new Dictionary<string, (Motion, AnimatorState)>();
            void Walk(AnimatorStateMachine sm, string prefix)
            {
                foreach (var s in sm.states) result[prefix + "." + s.state.name] = (synced ? l.GetOverrideMotion(s.state) : s.state.motion, s.state);
                foreach (var s in sm.stateMachines) Walk(s.stateMachine, prefix + "." + s.stateMachine.name);
            }
            Walk(machine, l.name);
            if (synced) Walk(machine, c.layers[l.syncedLayerIndex].name);
            return result;
        }

        static void Leaves(Motion m, List<AnimationClip> into)
        {
            if (m is AnimationClip clip) { if (!into.Contains(clip)) into.Add(clip); }
            else if (m is BlendTree tree) foreach (var child in tree.children) Leaves(child.motion, into);
        }

        void Sample(LyumaAv3Runtime runtime)
        {
            var playables = (List<AnimatorControllerPlayable>)typeof(LyumaAv3Runtime).GetField("playables", BindingFlags.NonPublic | BindingFlags.Instance).GetValue(runtime);
            var mixer = (AnimationLayerMixerPlayable)typeof(LyumaAv3Runtime).GetField("playableMixer", BindingFlags.NonPublic | BindingFlags.Instance).GetValue(runtime);
            var controllers = Controllers(runtime, playables);
            var motions = new Dictionary<(int, int), Dictionary<string, (Motion, AnimatorState)>>();
            var clips = new List<AnimationClip>();
            var ids = new Dictionary<AnimationClip, int>();
            var times = new List<List<float>>();
            var timeSet = new List<HashSet<float>>();
            Directory.CreateDirectory(output);

            var states = File.ReadAllLines(Path.Combine(recording, "states.tsv")).Select(l => l.Split('\t')).ToList();
            var clockRows = File.ReadAllLines(Path.Combine(recording, "clocks.tsv"));
            var columns = clockRows[0].Split('\t').Skip(1).ToArray();
            var held = new Dictionary<string, (string layer, string state)>();
            int next = 0, played = 0;
            using (var playedOut = Open("played.tsv"))
            {
                playedOut.Write("frame\tplayable:layer\tstate\tclip\tnormalized time\tstate cycle offset\tstate mirror\n");
                foreach (var row in clockRows.Skip(1))
                {
                    var cells = row.Split('\t');
                    var frame = int.Parse(cells[0], inv);
                    for (; next < states.Count && int.Parse(states[next][0], inv) <= frame; next++)
                    {
                        var parts = states[next][1].Split(new[] { ':' }, 3);
                        held[parts[0] + ":" + parts[1]] = (parts[2], states[next][2]);
                    }
                    if (held.Count == 0) continue;
                    for (int c = 0; c < columns.Length; c++)
                    {
                        if (!held.TryGetValue(columns[c], out var at)) throw new Exception($"states.tsv holds no state of layer {columns[c]} at frame {frame}");
                        var pl = columns[c].Split(':').Select(x => int.Parse(x, inv)).ToArray();
                        var controller = controllers[pl[0]] ?? throw new Exception($"playable {pl[0]} has a clock but no controller");
                        if (controller.layers[pl[1]].name != at.layer) throw new Exception($"layer {columns[c]} is {controller.layers[pl[1]].name}, recorded as {at.layer}");
                        var names = at.state.Split(new[] { " -> " }, StringSplitOptions.None);
                        var clocks = cells[c + 1].Split('>');
                        if (names.Length != clocks.Length) throw new Exception($"layer {columns[c]} at frame {frame} holds {names.Length} states and {clocks.Length} clocks");
                        for (int k = 0; k < names.Length; k++)
                        {
                            if (names[k] == "#00000000") continue;
                            if (!motions.TryGetValue((pl[0], pl[1]), out var byName)) motions[(pl[0], pl[1])] = byName = Motions(controller, pl[1]);
                            if (!byName.TryGetValue(names[k], out var found)) throw new Exception($"layer {columns[c]} has no state {names[k]}");
                            var (motion, state) = found;
                            var offset = state.cycleOffsetParameterActive ? "parameter" : F(state.cycleOffset);
                            var mirror = state.mirrorParameterActive ? "parameter" : state.mirror ? "1" : "0";
                            var leaves = new List<AnimationClip>();
                            Leaves(motion, leaves);
                            var n = float.Parse(clocks[k], inv);
                            foreach (var clip in leaves)
                            {
                                if (!ids.TryGetValue(clip, out var id))
                                {
                                    ids[clip] = id = clips.Count;
                                    clips.Add(clip);
                                    times.Add(new List<float>());
                                    timeSet.Add(new HashSet<float>());
                                }
                                if (timeSet[id].Add(n)) times[id].Add(n);
                                playedOut.Write($"{frame}\t{columns[c]}\t{(k == 0 ? "current" : "next")}\t{id}\t{clocks[k]}\t{offset}\t{mirror}\n");
                                played++;
                            }
                        }
                    }
                }
            }
            if (played == 0) throw new Exception("the recording's playables played no clip");

            var animator = runtime.GetComponent<Animator>();
            var root = runtime.gameObject;
            runtime.enabled = false;
            mixer.GetGraph().Destroy();
            var (handles, humanKeys, humanHeld) = animator.isHuman ? HumanOutput.Keys(animator) : (new MuscleHandle[0], new string[0], new bool[0]);
            var humanIndex = Enumerable.Range(0, humanKeys.Length).ToDictionary(i => humanKeys[i], i => i);
            string Key(EditorCurveBinding b) => b.type == typeof(Animator) && b.path == ""
                ? (humanIndex.ContainsKey("muscle:" + b.propertyName) ? "muscle:" : "animator:") + b.propertyName
                : $"{b.path}|{b.type.Name}|{b.propertyName}";
            var nativeHandles = new NativeArray<MuscleHandle>(handles, Allocator.Persistent);
            var values = new NativeArray<float>(humanKeys.Length, Allocator.Persistent);
            var runs = new NativeArray<int>(1, Allocator.Persistent);
            try
            {
                using (var clipsOut = Open("clips.tsv"))
                using (var samplesOut = Open("samples.tsv"))
                {
                    clipsOut.Write("clip\tname\tlength\tloop\tcycle offset\tmirror\tcurves\n");
                    samplesOut.Write("clip\tnormalized time\tclip time\tvalues (stream,evaluate per curve as clips.tsv lists them)\n");
                    for (int id = 0; id < clips.Count; id++)
                    {
                        var clip = clips[id];
                        var bindings = AnimationUtility.GetCurveBindings(clip).OrderBy(Key, StringComparer.Ordinal).ToArray();
                        var keys = bindings.Select(Key).ToArray();
                        var curves = bindings.Select(b => AnimationUtility.GetEditorCurve(clip, b)).ToArray();
                        var stream = keys.Select(k => humanIndex.TryGetValue(k, out var i) ? i : -1).ToArray();
                        var settings = AnimationUtility.GetAnimationClipSettings(clip);
                        clipsOut.Write($"{id}\t{clip.name.Replace('\t', ' ')}\t{F(clip.length)}\t{(clip.isLooping ? 1 : 0)}\t{F(settings.cycleOffset)}\t{(settings.mirror ? 1 : 0)}{string.Concat(keys.Select(k => "\t" + k))}\n");
                        var human = animator.isHuman && clip.humanMotion || stream.Any(i => i >= 0);
                        var graph = PlayableGraph.Create("AWClip");
                        try
                        {
                            graph.SetTimeUpdateMode(DirectorUpdateMode.Manual);
                            var outputPlayable = AnimationPlayableOutput.Create(graph, "clip", animator);
                            var cp = AnimationClipPlayable.Create(graph, clip);
                            cp.SetApplyFootIK(false);
                            cp.SetApplyPlayableIK(false);
                            if (human)
                            {
                                var job = AnimationScriptPlayable.Create(graph, new HumanOutput { Handles = nativeHandles, Values = values, Runs = runs }, 1);
                                graph.Connect(cp, 0, job, 0);
                                job.SetInputWeight(0, 1);
                                outputPlayable.SetSourcePlayable(job);
                            }
                            else outputPlayable.SetSourcePlayable(cp);
                            foreach (var n in times[id])
                            {
                                double t = (double)n * clip.length;
                                double wrapped = clip.length <= 0 ? 0 : clip.isLooping ? t - Math.Floor(t / clip.length) * clip.length : Math.Max(0, Math.Min(t, clip.length));
                                // Twice, so the previous time is this one too and the graph sees no motion between samples.
                                cp.SetTime(t);
                                cp.SetTime(t);
                                int before = runs[0];
                                graph.Evaluate(0);
                                if (human && runs[0] == before) throw new Exception($"clip {id}'s graph held no humanoid values at normalized time {F(n)}");
                                var line = new StringBuilder($"{id}\t{F(n)}\t{D(t)}");
                                for (int b = 0; b < bindings.Length; b++)
                                {
                                    var hi = stream[b];
                                    line.Append('\t').Append(hi >= 0 ? humanHeld[hi] ? F(values[hi]) : "-" : AnimationUtility.GetFloatValue(root, bindings[b], out var v) ? F(v) : "-").Append(',').Append(curves[b] == null ? "-" : F(curves[b].Evaluate((float)wrapped)));
                                }
                                samplesOut.Write(line.Append('\n').ToString());
                            }
                        }
                        finally { graph.Destroy(); }
                    }
                }
            }
            finally
            {
                nativeHandles.Dispose();
                values.Dispose();
                runs.Dispose();
            }
            Check();
            Debug.Log($"[aw-reference] sampled {clips.Count} clips at {times.Sum(t => t.Count)} times, {played} plays, into {output}");
        }

        // Reads the files back, so what fails is what an analysis would read.
        void Check()
        {
            var curves = File.ReadLines(Path.Combine(output, "clips.tsv")).Skip(1).Select(l => l.Split('\t')).ToDictionary(c => c[0], c => c.Length - 6);
            var sampled = new HashSet<string>();
            foreach (var cells in File.ReadLines(Path.Combine(output, "samples.tsv")).Skip(1).Select(l => l.Split('\t')))
            {
                if (!curves.TryGetValue(cells[0], out var n)) throw new Exception($"samples.tsv samples clip {cells[0]}, which clips.tsv lacks");
                if (cells.Length != 3 + n) throw new Exception($"clip {cells[0]} at {cells[1]} has {cells.Length - 3} values for {n} curves");
                sampled.Add(cells[0] + "\t" + cells[1]);
            }
            foreach (var cells in File.ReadLines(Path.Combine(output, "played.tsv")).Skip(1).Select(l => l.Split('\t')))
            {
                var n = float.Parse(cells[4], inv);
                if (!sampled.Contains(cells[3] + "\t" + F(n))) throw new Exception($"clip {cells[3]}, played on layer {cells[1]} at frame {cells[0]}, lacks a sample at normalized time {cells[4]}");
            }
            var clipsSampled = new HashSet<string>(sampled.Select(s => s.Substring(0, s.IndexOf('\t'))));
            var unsampled = curves.Keys.Where(c => !clipsSampled.Contains(c)).ToList();
            if (unsampled.Count > 0) throw new Exception("clips without a sample: " + string.Join(", ", unsampled));
        }

        StreamWriter Open(string name) => new StreamWriter(Path.Combine(output, name), false, new UTF8Encoding(false)) { NewLine = "\n" };
    }
}
