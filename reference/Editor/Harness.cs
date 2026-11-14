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
using UnityEditor.SceneManagement;
using UnityEngine;
using UnityEngine.Animations;
using UnityEngine.LowLevel;
using UnityEngine.PlayerLoop;
using UnityEngine.Playables;
using UnityEngine.Rendering;
using VRC.Dynamics;
using VRC.SDK3.Avatars.Components;
using VRC.SDK3.Avatars.ScriptableObjects;
using VRC.SDKBase.Editor.BuildPipeline;
using Control = VRC.SDK3.Avatars.ScriptableObjects.VRCExpressionsMenu.Control;

namespace AvatarWorkshop.Reference
{
    public static class Harness
    {
        public const int Rate = 60;

        internal static Recorder recorder;
        static (bool enabled, EnterPlayModeOptions options)? playModeSettings;

        public static void Run()
        {
            try { Begin(); }
            catch (Exception e) { Fail(e); }
        }

        internal static string Env(string name)
        {
            var v = Environment.GetEnvironmentVariable(name);
            return string.IsNullOrEmpty(v) ? null : v;
        }

        static void Begin()
        {
            var output = Env("AW_OUT") ?? throw new Exception("AW_OUT is required");
            var scenePath = Env("AW_SCENE");
            var avatarName = Env("AW_AVATAR");
            if (scenePath == "curves")
            {
                if (avatarName != null || Env("AW_STEPS") != null) throw new Exception("the curve probe plays no avatar and no steps");
                CurveProbe.Record(output);
                Finish(0);
                return;
            }
            Fixture.Kind? kind = scenePath == null ? Fixture.Kind.Fixture : scenePath == "probe" ? Fixture.Kind.Probe : scenePath == "human" ? Fixture.Kind.Human
                : scenePath == "blend" ? Fixture.Kind.Blend : scenePath == "writedefaults" ? Fixture.Kind.WriteDefaults
                : scenePath == "layering" ? Fixture.Kind.Layering : scenePath == "sampletime" ? Fixture.Kind.SampleTime
                : scenePath == "rotation" ? Fixture.Kind.Rotation : (Fixture.Kind?)null;
            if (kind.HasValue) scenePath = Fixture.Build(kind.Value);

            EditorSceneManager.OpenScene(scenePath, OpenSceneMode.Single);
            var descriptors = UnityEngine.Object.FindObjectsOfType<VRCAvatarDescriptor>(true);
            var chosen = descriptors.Where(d => avatarName == null ? d.gameObject.activeInHierarchy : d.gameObject.name == avatarName).ToList();
            if (chosen.Count != 1) throw new Exception($"scene {scenePath} has {chosen.Count} avatars named '{avatarName}' among {descriptors.Length}");
            var avatar = chosen[0].gameObject;
            avatar.SetActive(true);
            var migrated = AvatarWorkshop.Exporter.Exporter.MigrateDynamicBones(avatar);
            foreach (var go in avatar.scene.GetRootGameObjects())
                if (go != avatar && !avatar.transform.IsChildOf(go.transform)) go.SetActive(false);
            for (var p = avatar.transform.parent; p != null; p = p.parent) p.gameObject.SetActive(true);

            var emu = new GameObject("AWEmulator").AddComponent<LyumaAv3Emulator>();
            emu.DisableMirrorClone = true;
            emu.DisableShadowClone = true;
            emu.HaveEyesFollowMouse = false;
            emu.RunPreprocessAvatarHook = true;
            emu.SelectAvatarOnStartup = false;

            playModeSettings = (EditorSettings.enterPlayModeOptionsEnabled, EditorSettings.enterPlayModeOptions);
            EditorSettings.enterPlayModeOptionsEnabled = true;
            EditorSettings.enterPlayModeOptions = EnterPlayModeOptions.DisableDomainReload;
            recorder = new Recorder(output, avatar.name, kind, migrated);
            EditorApplication.playModeStateChanged += OnPlayMode;
            EditorApplication.EnterPlaymode();
        }

        static void OnPlayMode(PlayModeStateChange change)
        {
            if (change != PlayModeStateChange.EnteredPlayMode) return;
            try
            {
                Time.captureDeltaTime = 1f / Rate;
                Time.fixedDeltaTime = 1f / Rate;
                Time.maximumDeltaTime = 1f / Rate;
                UnityEngine.Random.InitState(1);
                Install();
            }
            catch (Exception e) { Fail(e); }
        }

        struct InputsStage { }
        struct PoseStage { }
        struct RecordStage { }
        struct DriversStage { }
        struct DynamicsStage { }

        static void Install()
        {
            var loop = PlayerLoop.GetCurrentPlayerLoop();
            var dynamics = false;
            for (int i = 0; i < loop.subSystemList.Length; i++)
            {
                var sys = loop.subSystemList[i];
                var list = sys.subSystemList.ToList();
                if (sys.type == typeof(Update))
                    list.Insert(0, new PlayerLoopSystem { type = typeof(InputsStage), updateDelegate = () => Guard(recorder.Inputs) });
                if (sys.type == typeof(PreLateUpdate))
                {
                    int anim = list.FindIndex(s => s.type == typeof(PreLateUpdate.DirectorUpdateAnimationBegin));
                    if (anim < 0) throw new Exception("PreLateUpdate.DirectorUpdateAnimationBegin is not in the player loop");
                    list.Insert(anim, new PlayerLoopSystem { type = typeof(DynamicsStage), updateDelegate = () => Guard(recorder.Dynamics) });
                    int at = list.FindIndex(s => s.type == typeof(PreLateUpdate.ScriptRunBehaviourLateUpdate));
                    list.Insert(at < 0 ? 0 : at, new PlayerLoopSystem { type = typeof(PoseStage), updateDelegate = () => Guard(recorder.Pose) });
                    int sdk = list.FindIndex(s => s.type?.Name == "VRCAvatarDynamicsPreSchedule");
                    if (sdk >= 0)
                    {
                        list.Insert(sdk, new PlayerLoopSystem { type = typeof(DriversStage), updateDelegate = () => Guard(recorder.Drivers) });
                        dynamics = true;
                    }
                }
                if (sys.type == typeof(PostLateUpdate))
                    list.Add(new PlayerLoopSystem { type = typeof(RecordStage), updateDelegate = () => Guard(recorder.Record) });
                sys.subSystemList = list.ToArray();
                loop.subSystemList[i] = sys;
            }
            if (!dynamics) throw new Exception("the VRChat SDK's VRCAvatarDynamicsPreSchedule stage is not in the player loop");
            PlayerLoop.SetPlayerLoop(loop);
        }

        static void Guard(Action a)
        {
            if (recorder == null || recorder.Finished) return;
            try { a(); }
            catch (Exception e) { Fail(e); }
        }

        internal static void Finish(int code)
        {
            PlayerLoop.SetPlayerLoop(PlayerLoop.GetDefaultPlayerLoop());
            if (playModeSettings.HasValue)
            {
                EditorSettings.enterPlayModeOptionsEnabled = playModeSettings.Value.enabled;
                EditorSettings.enterPlayModeOptions = playModeSettings.Value.options;
            }
            EditorApplication.Exit(code);
        }

        internal static void Fail(Exception e)
        {
            Debug.LogError("[aw-reference] FAILED " + e);
            var output = Env("AW_OUT");
            if (output != null) { Directory.CreateDirectory(output); File.WriteAllText(Path.Combine(output, "error.txt"), e.ToString()); }
            if (recorder != null) recorder.Finished = true;
            Finish(1);
        }
    }

    // Runs last in the build preprocess the emulator runs on the recorded avatar, so the export is the build the
    // recording plays, with whatever names and ids build tools generated for it.
    class SameBuildExport : IVRCSDKPreprocessAvatarCallback
    {
        public int callbackOrder => int.MaxValue;

        public bool OnPreprocessAvatar(GameObject avatar)
        {
            var recorder = Harness.recorder;
            if (recorder == null || !EditorApplication.isPlayingOrWillChangePlaymode) return true;
            try { recorder.WriteBuild(avatar); }
            catch (Exception e) { Harness.Fail(e); return false; }
            return true;
        }
    }

    // The humanoid values the animator produced, read from its output stream: muscles the layers mixed, before the
    // skeleton is posed from them and before anything moves its bones afterwards.
    struct HumanOutput : IAnimationJob
    {
        public NativeArray<MuscleHandle> Handles;
        public NativeArray<float> Values;
        public NativeArray<int> Runs;

        public void ProcessRootMotion(AnimationStream stream) { }

        public void ProcessAnimation(AnimationStream stream)
        {
            if (!stream.isHumanStream) return;
            var h = stream.AsHuman();
            int n = 0;
            for (int i = 0; i < Handles.Length; i++) Values[n++] = h.GetMuscle(Handles[i]);
            n = Put(n, h.bodyLocalPosition, h.bodyLocalRotation);
            foreach (AvatarIKGoal g in Enum.GetValues(typeof(AvatarIKGoal))) n = Put(n, h.GetGoalLocalPosition(g), h.GetGoalLocalRotation(g));
            Runs[0]++;
        }

        int Put(int n, Vector3 p, Quaternion q)
        {
            Values[n++] = p.x; Values[n++] = p.y; Values[n++] = p.z;
            Values[n++] = q.x; Values[n++] = q.y; Values[n++] = q.z; Values[n++] = q.w;
            return n;
        }
    }

    class Step
    {
        public string Label;
        public string Control;
        public int Frames;
        public Dictionary<string, float> Set = new Dictionary<string, float>();
        public Action<Recorder, int> Each;
        public Coverage.ProbeCase Case;
        public int First;
        public bool Render;
    }

    class Tip
    {
        public string Id;
        public Transform Leaf;
        public Vector3 Endpoint;
        public float Length;
    }

    class Recorder
    {
        public bool Finished;
        readonly string output;
        readonly string avatarName;
        readonly bool fixture, probe, human, blend, layering, sampleTime, rotation;
        bool Cased => human || blend || layering || sampleTime || rotation;
        readonly bool migratedDynamicBones;
        bool exported;
        StreamWriter timeOut;
        readonly CultureInfo inv = CultureInfo.InvariantCulture;

        int frame = -1;
        int warmup;
        LyumaAv3Runtime runtime;
        Transform root;
        Vector3 rootPos;
        Quaternion rootRot;
        readonly List<Step> steps = new List<Step>();
        readonly StringBuilder menuRows = new StringBuilder();
        int stepIndex = -1;
        int stepFrame;
        readonly List<Tip> tips = new List<Tip>();
        Transform leftArm, rightArm;
        Transform[] tracked = new Transform[0];
        Vector2 armSwing;
        Vector3 held;
        readonly Dictionary<string, float> defaults = new Dictionary<string, float>();
        readonly Dictionary<int, string> stateNames = new Dictionary<int, string>();
        readonly List<(Renderer r, string path, string prop, ShaderPropertyType type)> materialProps = new List<(Renderer, string, string, ShaderPropertyType)>();
        readonly List<(Transform t, string path)> animatedTransforms = new List<(Transform, string)>();
        List<AnimatorControllerPlayable> playables;
        AnimationLayerMixerPlayable mixer;
        (Transform t, string path)[] drivers = new (Transform, string)[0];
        (Transform t, string path)[] bones = new (Transform, string)[0];
        readonly Dictionary<string, string> lastPoses = new Dictionary<string, string>();
        readonly Dictionary<string, string> simulated = new Dictionary<string, string>();
        int posedFrame = -1;
        (int playable, string name, int id, AnimatorControllerParameterType type)[] dynamics;
        AnimatorControllerPlayable[] dynamicsPlayables;
        readonly Dictionary<string, string> lastDynamics = new Dictionary<string, string>();
        int dynamicsFrame = -1;
        (int playable, int layer)[] clockLayers;
        StreamWriter clocksOut;
        (string key, EditorCurveBinding binding)[] fields = new (string, EditorCurveBinding)[0];
        string[] unheld = new string[0];
        string[] humanKeys = new string[0];
        bool[] humanHeld = new bool[0];
        NativeArray<MuscleHandle> humanHandles;
        NativeArray<float> humanValues;
        NativeArray<int> humanRuns;
        int humanSeen;

        StreamWriter dynamicsOut, paramsOut, statesOut, tipsOut, valuesOut, stepsOut, rendersOut, trackOut, posesOut, grabsOut, inputsOut;
        readonly List<string> applied = new List<string>();
        string placed, swung;
        PhysBoneManager.Grab grab;
        readonly Dictionary<string, string> lastParams = new Dictionary<string, string>();
        readonly Dictionary<string, string> lastStates = new Dictionary<string, string>();
        Dictionary<string, string> lastValues = new Dictionary<string, string>();
        int renderCount;
        Camera camera;
        RenderTexture target;

        public Recorder(string output, string avatarName, Fixture.Kind? kind, bool migratedDynamicBones)
        {
            fixture = kind == Fixture.Kind.Fixture;
            probe = kind == Fixture.Kind.Probe;
            human = kind == Fixture.Kind.Human;
            blend = kind == Fixture.Kind.Blend;
            layering = kind == Fixture.Kind.Layering;
            sampleTime = kind == Fixture.Kind.SampleTime;
            rotation = kind == Fixture.Kind.Rotation;
            this.output = output;
            this.avatarName = avatarName;
            this.migratedDynamicBones = migratedDynamicBones;
        }

        public void WriteBuild(GameObject avatar)
        {
            // The emulator suffixes the avatar's name while the preprocess runs on it.
            if (avatar.name != avatarName && avatar.name != avatarName + "(Clone)") return;
            if (exported) throw new Exception($"the build preprocess ran twice on {avatarName}");
            exported = true;
            Directory.CreateDirectory(output);
            var name = avatar.name;
            avatar.name = avatarName;
            var clock = System.Diagnostics.Stopwatch.StartNew();
            try
            {
                var report = AvatarWorkshop.Exporter.Exporter.Write(avatar, Path.Combine(output, "avatar.glb"), Path.Combine(output, "census.json"), migratedDynamicBones);
                File.WriteAllText(Path.Combine(output, "export.txt"), string.Join("\n", report.Lines(clock.Elapsed)) + "\n");
            }
            finally { avatar.name = name; }
        }

        string F(float v) => v.ToString("R", inv);

        static string Clean(string name) => string.Join(" ", (name ?? "").Split((char[])null, StringSplitOptions.RemoveEmptyEntries));

        string Xf(Vector3 p, Quaternion q) => $"{F(p.x)},{F(p.y)},{F(p.z)},{F(q.x)},{F(q.y)},{F(q.z)},{F(q.w)}";

        public void Inputs()
        {
            frame++;
            if (runtime == null)
            {
                if (frame > 600) throw new Exception("Av3Emulator never attached a runtime to the avatar");
                runtime = UnityEngine.Object.FindObjectsOfType<LyumaAv3Runtime>().FirstOrDefault(r => !r.IsMirrorClone && !r.IsShadowClone && r.GetComponent<VRCAvatarDescriptor>() != null);
                if (runtime == null) return;
                if (!exported) throw new Exception($"no build preprocess of {avatarName} reached the export (an earlier preprocess callback may have failed; see the log)");
                mixer = (AnimationLayerMixerPlayable)typeof(LyumaAv3Runtime).GetField("playableMixer", BindingFlags.NonPublic | BindingFlags.Instance).GetValue(runtime);
                Directory.CreateDirectory(output);
                timeOut = Open("time.tsv");
                DynamicParameters();
                HumanStream();
                ClockLayers();
                return;
            }
            if (root == null)
            {
                warmup++;
                if (warmup < 5) return;
                Setup();
            }
            if (stepIndex < 0 || stepFrame >= steps[stepIndex].Frames)
            {
                stepIndex++;
                stepFrame = 0;
                if (stepIndex >= steps.Count) { Complete(); return; }
                BeginStep(steps[stepIndex]);
            }
            steps[stepIndex].Each?.Invoke(this, stepFrame);
            inputsOut.Write($"{frame.ToString(inv)}\t{placed ?? "-"}\t{swung ?? "-"}{string.Concat(applied.Select(a => "\t" + a))}\n");
            applied.Clear();
            placed = swung = null;
        }

        // Put between the emulator's mixer and its output, which only reads what passes through.
        void HumanStream()
        {
            var animator = runtime.GetComponent<Animator>();
            if (animator == null || !animator.isHuman) return;
            var graph = mixer.GetGraph();
            if (graph.GetOutputCountByType<AnimationPlayableOutput>() != 1) throw new Exception("the emulator's graph does not have one animation output");
            var output = (AnimationPlayableOutput)graph.GetOutputByType<AnimationPlayableOutput>(0);
            if (!output.GetSourcePlayable().Equals((Playable)mixer)) throw new Exception("the emulator's animation output does not play its mixer");

            var handles = new MuscleHandle[MuscleHandle.muscleHandleCount];
            MuscleHandle.GetMuscleHandles(handles);
            humanHandles = new NativeArray<MuscleHandle>(handles, Allocator.Persistent);
            var keys = handles.Select(h => "muscle:" + Fixture.Binding(h.name)).ToList();
            var mapped = handles.Select(h => Array.IndexOf(HumanTrait.MuscleName, h.name))
                .Select(m => m < 0 || animator.GetBoneTransform((HumanBodyBones)HumanTrait.BoneFromMuscle(m)) != null).ToList();
            foreach (var part in new[] { "Root" }.Concat(Enum.GetNames(typeof(AvatarIKGoal))))
                keys.AddRange(new[] { "T.x", "T.y", "T.z", "Q.x", "Q.y", "Q.z", "Q.w" }.Select(c => "muscle:" + part + c));
            humanKeys = keys.ToArray();
            humanHeld = mapped.Concat(Enumerable.Repeat(true, keys.Count - mapped.Count)).ToArray();
            humanValues = new NativeArray<float>(humanKeys.Length, Allocator.Persistent);
            humanRuns = new NativeArray<int>(1, Allocator.Persistent);
            var job = AnimationScriptPlayable.Create(graph, new HumanOutput { Handles = humanHandles, Values = humanValues, Runs = humanRuns }, 1);
            graph.Connect(mixer, 0, job, 0);
            job.SetInputWeight(0, 1);
            output.SetSourcePlayable(job);
        }

        void ClockLayers()
        {
            var layers = new List<(int, int)>();
            for (int i = 0; i < playables.Count; i++)
                if (playables[i].IsValid())
                    for (int l = 0; l < playables[i].GetLayerCount(); l++) layers.Add((i, l));
            clockLayers = layers.ToArray();
            clocksOut = Open("clocks.tsv");
            clocksOut.Write("frame\t" + string.Join("\t", clockLayers.Select(x => $"{x.playable}:{x.layer}")) + "\n");
        }

        void Clocks(string f)
        {
            var line = new StringBuilder(f);
            foreach (var (i, l) in clockLayers)
            {
                var pl = playables[i];
                if (!pl.IsValid()) throw new Exception($"playable {i} is gone at frame {f}");
                line.Append('\t').Append(Clock(pl.GetCurrentAnimatorStateInfo(l), i, l, f));
                if (pl.IsInTransition(l)) line.Append('>').Append(Clock(pl.GetNextAnimatorStateInfo(l), i, l, f));
            }
            clocksOut.Write(line.Append('\n').ToString());
        }

        string Clock(AnimatorStateInfo info, int playable, int layer, string f)
        {
            if (float.IsNaN(info.normalizedTime) || float.IsInfinity(info.normalizedTime))
                throw new Exception($"layer {playable}:{layer} holds no state time at frame {f}: {info.normalizedTime}");
            return F(info.normalizedTime);
        }

        // Taken from the playables just before the animator evaluates: contacts and PhysBones write the emulator's
        // parameter list at another point in the frame than the animator reads it.
        void DynamicParameters()
        {
            var names = new SortedSet<string>(StringComparer.Ordinal);
            foreach (var r in runtime.GetComponentsInChildren<ContactReceiver>(true))
                if (!string.IsNullOrEmpty(r.parameter)) names.Add(r.parameter);
            foreach (var pb in runtime.GetComponentsInChildren<VRCPhysBoneBase>(true))
                if (!string.IsNullOrEmpty(pb.parameter))
                    foreach (var suffix in new[] { "_IsGrabbed", "_IsPosed", "_Angle", "_Stretch", "_Squish" }) names.Add(pb.parameter + suffix);
            playables = (List<AnimatorControllerPlayable>)typeof(LyumaAv3Runtime).GetField("playables", BindingFlags.NonPublic | BindingFlags.Instance).GetValue(runtime);
            var found = new List<(int, string, int, AnimatorControllerParameterType)>();
            for (int i = 0; i < playables.Count; i++)
            {
                if (!playables[i].IsValid()) continue;
                for (int k = 0; k < playables[i].GetParameterCount(); k++)
                {
                    var p = playables[i].GetParameter(k);
                    if (names.Contains(p.name)) found.Add((i, p.name, p.nameHash, p.type));
                }
            }
            dynamics = found.OrderBy(d => d.Item1).ThenBy(d => d.Item2, StringComparer.Ordinal).ToArray();
            dynamicsPlayables = playables.ToArray();
            dynamicsOut = Open("dynamics.tsv");
        }

        public void Dynamics()
        {
            if (dynamicsOut == null || Finished) return;
            var f = frame.ToString(inv);
            if (!playables.SequenceEqual(dynamicsPlayables)) throw new Exception($"the emulator rebuilt its playables at frame {f}");
            foreach (var (i, name, id, type) in dynamics)
            {
                var pl = playables[i];
                if (!pl.IsValid()) throw new Exception($"playable {i}, which holds {name}, is gone at frame {f}");
                var value = type == AnimatorControllerParameterType.Float ? F(pl.GetFloat(id))
                    : type == AnimatorControllerParameterType.Int ? pl.GetInteger(id).ToString(inv)
                    : pl.GetBool(id) ? "1" : "0";
                var key = $"{i}:{name}";
                if (lastDynamics.TryGetValue(key, out var old) && old == value) continue;
                dynamicsOut.Write($"{f}\t{key}\t{value}\n");
                lastDynamics[key] = value;
            }
            dynamicsFrame = frame;
        }

        public void Pose()
        {
            if (root == null || stepIndex < 0 || Finished || armSwing == Vector2.zero) return;
            foreach (var (bone, sign) in new[] { (leftArm, -1f), (rightArm, 1f) })
            {
                if (bone == null) continue;
                var q = root.rotation * Quaternion.Euler(0, sign * armSwing.x, sign * armSwing.y) * Quaternion.Inverse(root.rotation);
                bone.rotation = q * bone.rotation;
            }
        }

        public void Drivers()
        {
            if (root == null || stepIndex < 0 || Finished) return;
            var f = frame.ToString(inv);
            void Write(string space, string path, string pose)
            {
                var key = space + "\t" + path;
                if (lastPoses.TryGetValue(key, out var old) && old == pose) return;
                posesOut.Write($"{f}\t{space}\t{path}\t{pose}\n");
                lastPoses[key] = pose;
            }
            foreach (var (t, path) in drivers)
            {
                if (t == null) throw new Exception($"driver transform {path} is gone at frame {f}");
                var s = t.lossyScale;
                Write("root", path, $"{Xf(Local(t.position), Quaternion.Inverse(root.rotation) * t.rotation)},{F(s.x)},{F(s.y)},{F(s.z)}");
            }
            foreach (var (t, path) in bones)
            {
                if (t == null) throw new Exception($"PhysBone transform {path} is gone at frame {f}");
                var pose = LocalPose(t);
                if (simulated.TryGetValue(path, out var own) && own == pose) continue;
                Write("local", path, pose);
            }
            posedFrame = frame;
        }

        string LocalPose(Transform t)
        {
            var s = t.localScale;
            return $"{Xf(t.localPosition, t.localRotation)},{F(s.x)},{F(s.y)},{F(s.z)}";
        }

        public void Record()
        {
            if (timeOut == null || Finished) return;
            var f = frame.ToString(inv);
            if (!mixer.IsValid()) throw new Exception($"the emulator's playable graph is gone at frame {f}");
            timeOut.Write($"{f}\t{mixer.GetTime().ToString("R", inv)}\n");
            if (dynamicsFrame != frame) throw new Exception($"no animator values of the contact and PhysBone parameters were taken before the animator ran at frame {f}");
            Clocks(f);
            if (root == null || stepIndex < 0) return;
            if (posedFrame != frame) throw new Exception($"no driver poses were taken before the SDK's dynamics at frame {f}");
            if (applied.Count > 0 || placed != null || swung != null) throw new Exception($"scripted inputs were applied after frame {f}'s were logged");
            foreach (var (t, path) in bones) simulated[path] = LocalPose(t);
            foreach (var (name, value) in Parameters())
                if (!lastParams.TryGetValue(name, out var old) || old != value) { paramsOut.Write($"{f}\t{name}\t{value}\n"); lastParams[name] = value; }
            foreach (var (layer, value) in States())
                if (!lastStates.TryGetValue(layer, out var old) || old != value) { statesOut.Write($"{f}\t{layer}\t{value}\n"); lastStates[layer] = value; }
            foreach (var tip in tips)
            {
                if (tip.Leaf == null) continue;
                var p = Local(tip.Leaf.TransformPoint(tip.Endpoint));
                tipsOut.Write($"{f}\t{tip.Id}\t{F(p.x)}\t{F(p.y)}\t{F(p.z)}\n");
            }
            var line = new StringBuilder(f);
            line.Append('\t').Append(Xf(root.position, root.rotation));
            foreach (var b in tracked)
                line.Append('\t').Append(b == null ? "-" : Xf(Local(b.position), Quaternion.Inverse(root.rotation) * b.rotation));
            trackOut.Write(line.Append('\n').ToString());
            var values = Values();
            foreach (var kv in values)
                if (!lastValues.TryGetValue(kv.Key, out var old) || old != kv.Value) valuesOut.Write($"{f}\t{kv.Key}\t{kv.Value}\n");
            foreach (var k in lastValues.Keys.Where(k => !values.ContainsKey(k))) valuesOut.Write($"{f}\t{k}\t-\n");
            lastValues = values;

            var step = steps[stepIndex];
            if (step.Case != null) Played(step);
            if (stepFrame == step.Frames - 1 && step.Render) Render(step);
            stepFrame++;
        }

        void Setup()
        {
            root = runtime.transform;
            rootPos = root.position;
            rootRot = root.rotation;
            placed = Xf(rootPos, rootRot);
            swung = "0,0";
            var animator = runtime.GetComponent<Animator>();
            if (animator != null && animator.isHuman)
            {
                leftArm = animator.GetBoneTransform(HumanBodyBones.LeftUpperArm);
                rightArm = animator.GetBoneTransform(HumanBodyBones.RightUpperArm);
                tracked = new[] { HumanBodyBones.Head, HumanBodyBones.LeftHand, HumanBodyBones.RightHand, HumanBodyBones.Hips }.Select(animator.GetBoneTransform).ToArray();
            }
            var descriptor = runtime.GetComponent<VRCAvatarDescriptor>();
            if (descriptor.expressionParameters != null)
                foreach (var p in descriptor.expressionParameters.parameters)
                    if (!string.IsNullOrEmpty(p.name)) defaults[p.name] = p.defaultValue;

            var controllers = descriptor.baseAnimationLayers.Concat(descriptor.specialAnimationLayers ?? new VRCAvatarDescriptor.CustomAnimLayer[0])
                .Select(l => l.animatorController).Where(c => c != null).Distinct().ToList();
            foreach (var c in Resources.FindObjectsOfTypeAll<AnimatorController>())
                foreach (var layer in c.layers) Names(layer.stateMachine, layer.name);
            var bindings = controllers.SelectMany(c => c.animationClips).Distinct().SelectMany(AnimationUtility.GetCurveBindings).ToList();
            foreach (var g in bindings.Where(b => b.propertyName.StartsWith("material.")).GroupBy(b => (b.path, b.type, Base(b.propertyName))))
            {
                var t = root.Find(g.Key.path);
                var r = t == null ? null : t.GetComponent(g.Key.type) as Renderer;
                if (r == null) continue;
                var prop = g.Key.Item3;
                var shader = r.sharedMaterials.Where(m => m != null && m.shader.FindPropertyIndex(prop) >= 0).Select(m => m.shader).FirstOrDefault();
                if (shader == null) continue;
                materialProps.Add((r, g.Key.path, prop, shader.GetPropertyType(shader.FindPropertyIndex(prop))));
            }
            if (animator != null && animator.isHuman && animator.avatar.humanDescription.hasTranslationDoF && bindings.Any(b => b.type == typeof(Animator) && b.propertyName.Contains("TDOF")))
                throw new Exception("the avatar has translation DoF: its TDOF curves hold values the animator's output stream cannot read");
            fields = bindings.Where(b => typeof(Component).IsAssignableFrom(b.type)).Select(b => (b, c: root.Find(b.path)?.GetComponent(b.type)))
                .Where(x => x.c is VRCConstraintBase || x.c is IConstraint || x.c is ContactSender || x.c is ContactReceiver)
                .Select(x => (FieldKey(x.c, x.b.propertyName), x.b))
                .GroupBy(x => x.Item1).Select(g => g.First())
                .OrderBy(x => x.Item1, StringComparer.Ordinal).ToArray();
            unheld = fields.Where(f => !AnimationUtility.GetFloatValue(root.gameObject, f.binding, out _)).Select(f => f.key)
                .Concat(Resources.FindObjectsOfTypeAll<RuntimeAnimatorController>().SelectMany(c => c.animationClips).SelectMany(AnimationUtility.GetCurveBindings)
                    .Where(b => b.type == typeof(Animator) && b.path == "").Select(b => "muscle:" + b.propertyName))
                .Distinct().Except(humanKeys).ToArray();
            var unheldSet = new HashSet<string>(unheld);
            fields = fields.Where(f => !unheldSet.Contains(f.key)).ToArray();
            foreach (var path in bindings.Where(b => b.type == typeof(Transform)).Select(b => b.path).Distinct().OrderBy(p => p, StringComparer.Ordinal))
            {
                var t = root.Find(path);
                if (t != null) animatedTransforms.Add((t, path));
            }

            var physBones = root.GetComponentsInChildren<VRCPhysBoneBase>(true).OrderBy(p => PathOf(p.transform), StringComparer.Ordinal).ToList();
            foreach (var pb in physBones)
                Tips(pb, physBones);
            Inputs(physBones);

            Directory.CreateDirectory(output);
            Directory.CreateDirectory(Path.Combine(output, "render"));
            paramsOut = Open("params.tsv");
            statesOut = Open("states.tsv");
            tipsOut = Open("tips.tsv");
            valuesOut = Open("values.tsv");
            stepsOut = Open("steps.tsv");
            rendersOut = Open("renders.tsv");
            trackOut = Open("track.tsv");
            posesOut = Open("poses.tsv");
            grabsOut = Open("grabs.tsv");
            inputsOut = Open("inputs.tsv");
            trackOut.Write("frame\troot (world)\thead\tleft hand\tright hand\thips (each from the root position and rotation, its scale ignored, in metres: px,py,pz,qx,qy,qz,qw)\n");
            File.WriteAllText(Path.Combine(output, "chains.tsv"),
                "tip\tlength\n" + string.Concat(tips.Select(t => $"{t.Id}\t{F(t.Length)}\n")));
            var coverage = Coverage.Count(root.gameObject);
            File.WriteAllText(Path.Combine(output, "coverage.json"), Json(coverage) + "\n");
            var missing = Coverage.Required.Where(k => coverage[k] == 0).ToList();
            if (fixture && missing.Count > 0) throw new Exception("fixture lacks " + string.Join(", ", missing));
            if (Cased)
            {
                var (name, listed, compositions) = human ? ("composition", Coverage.Compositions, Coverage.CountCompositions(descriptor))
                    : blend ? ("blend duration", Coverage.BlendCompositions, Coverage.CountBlendCompositions(descriptor))
                    : layering ? ("layering", Coverage.LayeringCompositions, Coverage.CountLayeringCompositions(descriptor))
                    : rotation ? ("rotation", Coverage.RotationCompositions, Coverage.CountRotationCompositions(descriptor))
                    : ("clip sample-time", Coverage.SampleTimeCompositions, Coverage.CountSampleTimeCompositions(descriptor));
                File.WriteAllText(Path.Combine(output, "compositions.json"), Json(compositions) + "\n");
                var absent = listed.Where(k => compositions[k] == 0).ToList();
                if (absent.Count > 0) throw new Exception($"the {name} probe lacks {string.Join(", ", absent)}");
            }
            SetupCamera();
            Schedule(descriptor);
            File.WriteAllText(Path.Combine(output, "menu.tsv"), menuRows.ToString());
            File.WriteAllText(Path.Combine(output, "meta.json"), Json(new Dictionary<string, object>
            {
                ["avatar"] = avatarName,
                ["fixture"] = fixture,
                ["human"] = human,
                ["blend"] = blend,
                ["layering"] = layering,
                ["sample_time"] = sampleTime,
                ["rotation"] = rotation,
                ["root_scale"] = new[] { root.lossyScale.x, root.lossyScale.y, root.lossyScale.z },
                ["unity"] = Application.unityVersion,
                ["dynamic_bones_migrated"] = migratedDynamicBones,
                ["rate"] = Harness.Rate,
                ["steps"] = steps.Count,
                ["frames"] = steps.Sum(s => s.Frames),
                ["tips"] = tips.Count,
                ["physbones"] = root.GetComponentsInChildren<VRCPhysBoneBase>(true).Length,
                ["contacts"] = root.GetComponentsInChildren<ContactBase>(true).Length,
                ["constraints"] = root.GetComponentsInChildren<VRCConstraintBase>(true).Length,
                ["drivers"] = drivers.Length,
                ["bones"] = bones.Length,
                ["packages"] = Packages(),
            }));
        }

        static string Base(string property)
        {
            var p = property.Substring("material.".Length);
            int dot = p.LastIndexOf('.');
            return dot > 0 && p.Length - dot == 2 && "rgbaxyzw".Contains(p[dot + 1]) ? p.Substring(0, dot) : p;
        }

        string FieldKey(Component c, string field)
        {
            switch (c)
            {
                case ContactBase contact:
                    Transform Anchor(ContactBase o) => o.rootTransform != null && o.rootTransform.IsChildOf(root) ? o.rootTransform : o.transform;
                    var anchor = Anchor(contact);
                    bool sender = contact is ContactSender;
                    var peers = root.GetComponentsInChildren<ContactBase>(true)
                        .Where(o => o.shapeType != ContactBase.ShapeType.Box && (o is ContactSender) == sender && Anchor(o) == anchor).ToList();
                    return $"{(sender ? "sender" : "receiver")}:{PathOf(anchor)}#{peers.IndexOf(contact)}:{field}";
                default:
                    var all = c.GetComponents<VRCConstraintBase>().Cast<Component>().Concat(c.GetComponents<IConstraint>().Cast<Component>()).ToList();
                    return $"constraint:{PathOf(c.transform)}#{all.IndexOf(c)}:{field}";
            }
        }

        void Names(AnimatorStateMachine sm, string prefix)
        {
            foreach (var s in sm.states) stateNames[Animator.StringToHash(prefix + "." + s.state.name)] = prefix + "." + s.state.name;
            foreach (var s in sm.stateMachines) Names(s.stateMachine, prefix + "." + s.stateMachine.name);
        }

        static Dictionary<string, string> Packages()
        {
            var d = new Dictionary<string, string>();
            foreach (var name in new[] { "com.vrchat.avatars", "com.vrchat.base", "lyuma.av3emulator", "com.avatar-workshop.reference" })
            {
                var info = UnityEditor.PackageManager.PackageInfo.FindForAssetPath("Packages/" + name);
                d[name] = info?.version ?? "absent";
            }
            return d;
        }

        StreamWriter Open(string name) => new StreamWriter(Path.Combine(output, name), false, new UTF8Encoding(false)) { NewLine = "\n" };

        readonly Dictionary<Transform, string> paths = new Dictionary<Transform, string>();

        string PathOf(Transform t)
        {
            if (paths.TryGetValue(t, out var known)) return known;
            return paths[t] = PathFrom(t);
        }

        string PathFrom(Transform t)
        {
            var parts = new List<string>();
            for (var x = t; x != null && x != root; x = x.parent) parts.Add(Segment(x));
            parts.Reverse();
            return string.Join("/", parts);
        }

        static string Segment(Transform t)
        {
            if (t.parent == null) return t.name;
            int twin = 0;
            for (int i = 0; i < t.GetSiblingIndex(); i++) if (t.parent.GetChild(i).name == t.name) twin++;
            return twin == 0 ? t.name : $"{t.name}#{twin}";
        }

        Vector3 Local(Vector3 world) => Quaternion.Inverse(root.rotation) * (world - root.position);

        static List<Transform> Chain(VRCPhysBoneBase pb, List<VRCPhysBoneBase> physBones, out Dictionary<Transform, List<Transform>> children)
        {
            var start = pb.GetRootTransform();
            var others = new HashSet<Transform>(physBones.Where(o => o != pb).Select(o => o.GetRootTransform()));
            var ignored = new HashSet<Transform>((pb.ignoreTransforms ?? new List<Transform>()).Where(x => x != null));
            var order = new List<Transform>();
            var kids = new Dictionary<Transform, List<Transform>>();
            void Walk(Transform t)
            {
                order.Add(t);
                kids[t] = Enumerable.Range(0, t.childCount).Select(t.GetChild)
                    .Where(c => !ignored.Contains(c) && !(pb.ignoreOtherPhysBones && others.Contains(c))).ToList();
                foreach (var c in kids[t]) Walk(c);
            }
            Walk(start);
            children = kids;
            return order;
        }

        void Tips(VRCPhysBoneBase pb, List<VRCPhysBoneBase> physBones)
        {
            var start = pb.GetRootTransform();
            Chain(pb, physBones, out var kids);
            void Walk(Transform t, float length)
            {
                if (kids[t].Count == 0)
                {
                    var end = pb.endpointPosition;
                    tips.Add(new Tip
                    {
                        Id = PathOf(start) + "|" + PathOf(t),
                        Leaf = t,
                        Endpoint = end,
                        Length = length + (end == Vector3.zero ? 0 : (Local(t.TransformPoint(end)) - Local(t.position)).magnitude),
                    });
                    return;
                }
                if (probe && t != start) tips.Add(new Tip { Id = PathOf(start) + "|" + PathOf(t), Leaf = t, Length = length });
                foreach (var k in kids[t]) Walk(k, length + (Local(k.position) - Local(t.position)).magnitude);
            }
            Walk(start, 0);
        }

        void Inputs(List<VRCPhysBoneBase> physBones)
        {
            var roles = new Dictionary<Transform, SortedSet<string>>();
            void Role(Transform t, string role)
            {
                if (t == null) return;
                if (!roles.TryGetValue(t, out var set)) roles[t] = set = new SortedSet<string>(StringComparer.Ordinal);
                set.Add(role);
            }
            var bone = new HashSet<Transform>();
            foreach (var pb in physBones)
            {
                var start = pb.GetRootTransform();
                Role(start.parent, "physbone-parent");
                if (start.root != start) Role(start.root, "physbone-scene-root");
                bone.UnionWith(Chain(pb, physBones, out _));
                foreach (var c in pb.colliders ?? new List<VRCPhysBoneColliderBase>())
                    if (c != null) Role(c.GetRootTransform(), "collider");
            }
            var targets = new HashSet<Transform>();
            foreach (var c in root.GetComponentsInChildren<VRCPhysBoneColliderBase>(true)) Role(c.GetRootTransform(), "collider");
            foreach (var c in root.GetComponentsInChildren<ContactBase>(true)) Role(c.GetRootTransform(), "contact");
            foreach (var c in root.GetComponentsInChildren<VRCConstraintBase>(true))
            {
                var target = c.GetEffectiveTargetTransform();
                targets.Add(target);
                Role(target, "constraint-target");
                Role(target.parent, "constraint-parent");
                for (int i = 0; i < c.Sources.Count; i++) Role(c.Sources[i].SourceTransform, "constraint-source");
                if (c is VRC.Dynamics.ManagedTypes.VRCWorldUpConstraintBase up) Role(up.WorldUpTransform, "constraint-world-up");
            }
            foreach (var t in roles.Keys.ToList())
                for (var x = t; x != null; x = x.parent)
                    if (bone.Contains(x) || (x != t && targets.Contains(x))) { roles[t].Add("moved-by-dynamics"); break; }
            foreach (var t in bone) Role(t, "physbone-bone");
            drivers = roles.Keys.Where(t => roles[t].Any(r => r != "physbone-bone")).Select(t => (t, PathOf(t))).OrderBy(x => x.Item2, StringComparer.Ordinal).ToArray();
            bones = bone.Select(t => (t, PathOf(t))).OrderBy(x => x.Item2, StringComparer.Ordinal).ToArray();
            var dupes = roles.Keys.GroupBy(PathOf).Where(g => g.Count() > 1).Select(g => g.Key).ToList();
            if (dupes.Count > 0) throw new Exception("transforms share a path: " + string.Join(", ", dupes));
            File.WriteAllText(Path.Combine(output, "drivers.tsv"), "transform\troles\n" +
                string.Concat(roles.Keys.Select(t => (PathOf(t), string.Join(",", roles[t]))).OrderBy(x => x.Item1, StringComparer.Ordinal).Select(x => $"{x.Item1}\t{x.Item2}\n")));
        }

        IEnumerable<(string, string)> Parameters()
        {
            foreach (var p in runtime.Bools) yield return (p.name, p.value ? "1" : "0");
            foreach (var p in runtime.Ints) yield return (p.name, p.value.ToString(inv));
            foreach (var p in runtime.Floats) yield return (p.name, F(p.value));
        }

        IEnumerable<(string, string)> States()
        {
            for (int i = 0; i < playables.Count; i++)
            {
                var pl = playables[i];
                if (!pl.IsValid()) continue;
                for (int l = 0; l < pl.GetLayerCount(); l++)
                {
                    var cur = pl.GetCurrentAnimatorStateInfo(l);
                    var s = StateName(cur.fullPathHash);
                    if (pl.IsInTransition(l)) s += " -> " + StateName(pl.GetNextAnimatorStateInfo(l).fullPathHash);
                    yield return ($"{i}:{l}:{pl.GetLayerName(l)}", s);
                }
            }
        }

        string StateName(int hash) => stateNames.TryGetValue(hash, out var n) ? n : "#" + hash.ToString("x8", inv);

        Dictionary<string, string> Values()
        {
            var v = new Dictionary<string, string>();
            foreach (var t in root.GetComponentsInChildren<Transform>(true))
            {
                if (t == root) continue;
                v["active:" + PathOf(t)] = t.gameObject.activeSelf ? "1" : "0";
            }
            foreach (var r in root.GetComponentsInChildren<Renderer>(true))
            {
                var path = PathOf(r.transform);
                v["enabled:" + path] = r.enabled ? "1" : "0";
                var mats = r.sharedMaterials;
                for (int i = 0; i < mats.Length; i++) v[$"slot:{path}:{i}"] = mats[i] == null ? "null" : mats[i].name;
                if (r is SkinnedMeshRenderer smr && smr.sharedMesh != null)
                    for (int i = 0; i < smr.sharedMesh.blendShapeCount; i++)
                        v[$"shape:{path}:{smr.sharedMesh.GetBlendShapeName(i)}"] = F(smr.GetBlendShapeWeight(i));
            }
            foreach (var g in root.GetComponentsInChildren<VRCPhysBoneBase>(true).GroupBy(p => p.transform))
            {
                int k = 0;
                foreach (var p in g) v[$"physbone:{PathOf(g.Key)}#{k++}"] = p.enabled ? "1" : "0";
            }
            var block = new MaterialPropertyBlock();
            foreach (var (r, path, prop, type) in materialProps)
            {
                r.GetPropertyBlock(block);
                var mat = r.sharedMaterials.FirstOrDefault(m => m != null && m.shader.FindPropertyIndex(prop) >= 0);
                string value;
                if (mat == null) { v[$"mat:{path}:{prop}"] = "-"; continue; }
                switch (type)
                {
                    case ShaderPropertyType.Color:
                        var c = block.HasColor(prop) || block.HasVector(prop) ? block.GetColor(prop) : mat.GetColor(prop);
                        value = $"{F(c.r)},{F(c.g)},{F(c.b)},{F(c.a)}"; break;
                    case ShaderPropertyType.Vector:
                        var x = block.HasVector(prop) ? block.GetVector(prop) : mat.GetVector(prop);
                        value = $"{F(x.x)},{F(x.y)},{F(x.z)},{F(x.w)}"; break;
                    case ShaderPropertyType.Texture:
                        value = mat.GetTexture(prop)?.name ?? "null"; break;
                    default:
                        value = F(block.HasFloat(prop) ? block.GetFloat(prop) : mat.GetFloat(prop)); break;
                }
                v[$"mat:{path}:{prop}"] = value;
            }
            for (int i = 0; i < playables.Count; i++)
            {
                var pl = playables[i];
                if (!pl.IsValid()) continue;
                v[$"playable:{i}"] = F(mixer.GetInputWeight(i + 1));
                for (int l = 0; l < pl.GetLayerCount(); l++) v[$"weight:{i}:{l}:{pl.GetLayerName(l)}"] = F(pl.GetLayerWeight(l));
                for (int k = 0; k < pl.GetParameterCount(); k++)
                {
                    var p = pl.GetParameter(k);
                    if (p.type == AnimatorControllerParameterType.Float && pl.IsParameterControlledByCurve(p.nameHash)) v[$"curve:{i}:{p.name}"] = F(pl.GetFloat(p.nameHash));
                }
            }
            if (humanKeys.Length > 0)
            {
                if (humanRuns[0] == humanSeen) throw new Exception($"the animator's output stream held no humanoid values at frame {frame}");
                humanSeen = humanRuns[0];
                for (int i = 0; i < humanKeys.Length; i++) v[humanKeys[i]] = humanHeld[i] ? F(humanValues[i]) : "-";
            }
            foreach (var (key, binding) in fields)
            {
                if (!AnimationUtility.GetFloatValue(root.gameObject, binding, out var field)) throw new Exception($"{key} has no value at frame {frame}");
                v[key] = F(AvatarWorkshop.Exporter.Exporter.MirroredField(binding.propertyName) ? -field : field);
            }
            foreach (var key in unheld) v[key] = "-";
            var ik = runtime.IKTrackingOutputData;
            foreach (var (part, type) in new[] { ("head", ik.trackingHead), ("leftHand", ik.trackingLeftHand), ("rightHand", ik.trackingRightHand), ("hip", ik.trackingHip),
                ("leftFoot", ik.trackingLeftFoot), ("rightFoot", ik.trackingRightFoot), ("leftFingers", ik.trackingLeftFingers), ("rightFingers", ik.trackingRightFingers),
                ("eyes", ik.trackingEyesAndEyelids), ("mouth", ik.trackingMouthAndJaw) })
                v["tracking:" + part] = type.ToString();
            v["locomotion:disabled"] = runtime.LocomotionIsDisabled ? "1" : "0";
            v["view"] = $"{F(ik.ViewPosition.x)},{F(ik.ViewPosition.y)},{F(ik.ViewPosition.z)}";
            foreach (var (t, path) in animatedTransforms)
            {
                var p = t.localPosition; var e = t.localRotation; var s = t.localScale;
                v[$"xform:{path}"] = $"{F(p.x)},{F(p.y)},{F(p.z)},{F(e.x)},{F(e.y)},{F(e.z)},{F(e.w)},{F(s.x)},{F(s.y)},{F(s.z)}";
            }
            return v;
        }

        void Place(Vector3 position, Quaternion rotation)
        {
            placed = Xf(position, rotation);
            root.SetPositionAndRotation(position, rotation);
        }

        void Swing(Vector2 arms)
        {
            swung = $"{F(arms.x)},{F(arms.y)}";
            armSwing = arms;
        }

        void Set(string name, float value)
        {
            var i = runtime.Ints.Find(p => p.name == name);
            if (i != null) i.value = (int)value;
            var f = runtime.Floats.Find(p => p.name == name);
            if (f != null) { f.value = value; f.exportedValue = value; }
            var b = runtime.Bools.Find(p => p.name == name);
            if (b != null) b.value = value != 0;
        }

        void Apply(string name, float v)
        {
            applied.Add($"{Escape(name)}={F(v)}");
            switch (name)
            {
                case "GestureLeft": runtime.GestureLeftIdx = (int)v; runtime.GestureLeft = (LyumaAv3Runtime.GestureIndex)(int)v; break;
                case "GestureRight": runtime.GestureRightIdx = (int)v; runtime.GestureRight = (LyumaAv3Runtime.GestureIndex)(int)v; break;
                case "GestureLeftWeight": runtime.GestureLeftWeight = v; break;
                case "GestureRightWeight": runtime.GestureRightWeight = v; break;
                case "Viseme": runtime.VisemeIdx = (int)v; break;
                case "Voice": runtime.Voice = v; break;
                case "Grounded": runtime.Grounded = v != 0; break;
                case "Seated": runtime.Seated = v != 0; break;
                case "AFK": runtime.AFK = v != 0; break;
                case "VelocityX": runtime.Velocity.x = v; break;
                case "VelocityY": runtime.Velocity.y = v; break;
                case "VelocityZ": runtime.Velocity.z = v; break;
                case "AngularY": runtime.AngularY = v; break;
                case "Upright": runtime.Upright = v; break;
                case "TrackingType": runtime.TrackingTypeIdx = (int)v; break;
                case "VRMode": runtime.VRMode = v != 0; break;
                case "InStation": runtime.InStation = v != 0; break;
                case "IsOnFriendsList": runtime.IsOnFriendsList = v != 0; break;
                default: Set(name, v); break;
            }
        }

        static readonly (string name, float rest)[] Builtins =
        {
            ("GestureLeft", 0), ("GestureRight", 0), ("GestureLeftWeight", 0), ("GestureRightWeight", 0), ("Viseme", 0), ("Voice", 0),
            ("Grounded", 1), ("Seated", 0), ("AFK", 0), ("VelocityX", 0), ("VelocityY", 0), ("VelocityZ", 0), ("AngularY", 0), ("Upright", 1),
            ("TrackingType", 3), ("VRMode", 1), ("InStation", 0), ("IsOnFriendsList", 0),
        };

        void Schedule(VRCAvatarDescriptor descriptor)
        {
            Step S(string label, int frames, bool render = false, string control = null) =>
                Add(new Step { Label = label, Frames = frames, Render = render, Control = control });

            var rest = S("rest", probe ? 900 : 90, render: true);
            foreach (var (n, v) in Builtins) rest.Set[n] = v;
            var script = Harness.Env("AW_STEPS");
            if (script != null)
            {
                if (Cased) throw new Exception("AW_STEPS cannot replace a probe's cases");
                int hold = int.Parse(Harness.Env("AW_STEP_FRAMES") ?? "20", inv);
                foreach (var text in script.Split(';'))
                {
                    var st = S(text.Trim(), hold, render: true);
                    foreach (var set in text.Split(new[] { ',' }, StringSplitOptions.RemoveEmptyEntries))
                    {
                        var pair = set.Split('=');
                        if (pair.Length != 2) throw new Exception("bad AW_STEPS entry: " + set);
                        st.Set[pair[0].Trim()] = float.Parse(pair[1], inv);
                    }
                }
                return;
            }
            if (Cased)
            {
                Compositions(descriptor);
                if (sampleTime) S("hold", Fixture.SampleTimeHold);
                S("final", 60, render: true);
                return;
            }

            var motion = S("motion", 6 * Harness.Rate);
            motion.Each = (r, k) =>
            {
                float t = k / (float)Harness.Rate;
                const float tau = Mathf.PI * 2;
                var offset = new Vector3(0.25f * Mathf.Sin(tau * t / 3), 0.04f * Mathf.Sin(tau * t / 0.75f), 0.4f * Mathf.Sin(tau * t / 2));
                r.Place(r.rootPos + r.rootRot * offset, r.rootRot * Quaternion.Euler(0, 45 * Mathf.Sin(tau * t / 6), 0));
                r.Swing(new Vector2(40 * Mathf.Sin(tau * t / 1.5f), 70 * Mathf.Sin(tau * t / 2)));
            };
            var stop = S("settle", 2 * Harness.Rate, render: true);
            stop.Each = (r, k) => { if (k == 0) { r.Place(r.rootPos, r.rootRot); r.Swing(Vector2.zero); } };

            if (probe) return;
            if (descriptor.expressionsMenu != null) Menu(descriptor.expressionsMenu, "", "", new HashSet<VRCExpressionsMenu>(), new Dictionary<string, float>());

            foreach (var n in new[] { "GestureLeft", "GestureRight" })
                for (int g = 1; g < 8; g++)
                {
                    var st = S($"builtin {n}={g}", 30, render: true, control: "builtin/" + n);
                    st.Set[n] = g; st.Set[n + "Weight"] = 1;
                    Reset(st.Set.Keys.ToList());
                }
            for (int v = 1; v < 15; v++)
            {
                var st = S($"builtin Viseme={v}", 12, render: true, control: "builtin/Viseme");
                st.Set["Viseme"] = v; st.Set["Voice"] = 0.6f;
            }
            Reset(new List<string> { "Viseme", "Voice" });
            foreach (var (n, v) in new (string, float)[] { ("Grounded", 0), ("Seated", 1), ("AFK", 1), ("VelocityZ", 3), ("VelocityX", -2), ("VelocityY", -4), ("AngularY", 120), ("Upright", 0.4f), ("TrackingType", 6), ("TrackingType", 1), ("VRMode", 0), ("InStation", 1), ("IsOnFriendsList", 1) })
            {
                var st = S($"builtin {n}={F(v)}", 45, render: true, control: "builtin/" + n);
                st.Set[n] = v;
                Reset(new List<string> { n });
            }
            if (fixture)
            {
                Grab(S("grab Tail", 60), "Tail.4", new Vector3(0.15f, 0.12f, -0.1f), pose: true);
                S("posed Tail", 60);
                Grab(S("grab Tail again", 45), "Tail.4", new Vector3(-0.1f, 0, 0), pose: false);
                S("released Tail", 60);
                Grab(S("grab Antenna", 30), "Antenna.0.0", new Vector3(0, 0.05f, 0.05f), pose: false);
                Grab(S("grab EarL", 45), "EarL.3", new Vector3(-0.08f, 0.1f, 0), pose: false);
                S("released EarL", 45);
            }
            S("final", 60, render: true);
        }

        // Each case of a probe, with the blend parameters ramped through it (BlendX 0 to 1, BlendU -1 to 1, BlendY up
        // and back down); a probe with a Mirror parameter plays each unmirrored then mirrored, and a case the Sitting
        // playable plays toggles Seated every 8 frames so the playable's 0.25 s blend never completes.
        void Compositions(VRCAvatarDescriptor descriptor)
        {
            var cases = Coverage.Cases(descriptor);
            if (cases.Count == 0) throw new Exception("the probe has no cases");
            bool mirrors = defaults.ContainsKey(Fixture.MirrorParameter), pulses = defaults.ContainsKey(Fixture.PulseParameter);
            var inputs = new List<string> { Fixture.CaseParameter };
            if (mirrors) inputs.Add(Fixture.MirrorParameter);
            if (pulses) inputs.Add(Fixture.PulseParameter);
            inputs.AddRange(Fixture.Ramps);
            if (cases.Any(c => c.Sitting)) inputs.Add("Seated");
            foreach (var c in cases)
                for (int m = 0; m < (mirrors ? 2 : 1); m++)
                {
                    var st = Add(new Step { Label = $"case {c.Id}{(mirrors ? $" mirror={m}" : "")}: {string.Join("; ", c.States.Select(x => x.label))}", Frames = 150, Case = c });
                    st.Set[Fixture.CaseParameter] = c.Id;
                    if (mirrors) st.Set[Fixture.MirrorParameter] = m;
                    st.Each = (r, k) =>
                    {
                        float t = k / (float)(st.Frames - 1);
                        r.Apply(Fixture.BlendX, t); r.Apply(Fixture.BlendU, 2 * t - 1); r.Apply(Fixture.BlendY, Mathf.Sin(Mathf.PI * t));
                        if (c.Sitting) r.Apply("Seated", k / 8 % 2 == 0 ? 1 : 0);
                        if (pulses && (k == Fixture.PulseOn || k == Fixture.PulseOff)) r.Apply(Fixture.PulseParameter, k == Fixture.PulseOn ? 1 : 0);
                    };
                    Reset(inputs, 20);
                }
        }

        // Fails a probe case whose states the animators never entered, or whose partial playable held a
        // weight strictly between 0 and 1 on fewer than 90% of its frames (a playable blending in passes through
        // partial weights on its way to full).
        readonly HashSet<int> caseStates = new HashSet<int>();
        readonly Dictionary<VRCAvatarDescriptor.AnimLayerType, int> casePartial = new Dictionary<VRCAvatarDescriptor.AnimLayerType, int>();

        void Played(Step step)
        {
            if (stepFrame == 0) { caseStates.Clear(); casePartial.Clear(); }
            foreach (var pl in playables.Where(p => p.IsValid()))
                for (int l = 0; l < pl.GetLayerCount(); l++)
                {
                    caseStates.Add(pl.GetCurrentAnimatorStateInfo(l).fullPathHash);
                    if (pl.IsInTransition(l)) caseStates.Add(pl.GetNextAnimatorStateInfo(l).fullPathHash);
                }
            foreach (var type in step.Case.Partial)
            {
                var w = mixer.GetInputWeight(PlayableIndex(type) + 1);
                if (w > 0 && w < 1) casePartial[type] = casePartial.TryGetValue(type, out var n) ? n + 1 : 1;
            }
            if (stepFrame < step.Frames - 1) return;
            var unplayed = step.Case.States.Where(x => !caseStates.Contains(x.hash)).Select(x => x.label).ToList();
            if (unplayed.Count > 0) throw new Exception($"{step.Label}: never entered {string.Join(", ", unplayed)}");
            var full = step.Case.Partial.Where(type => (casePartial.TryGetValue(type, out var n) ? n : 0) < 0.9f * step.Frames).ToList();
            if (full.Count > 0) throw new Exception($"{step.Label}: the {string.Join(", ", full)} playable held a partial weight on fewer than 90% of its frames");
        }

        int PlayableIndex(VRCAvatarDescriptor.AnimLayerType type)
        {
            var field = type == VRCAvatarDescriptor.AnimLayerType.Action ? "actionIndex" : type == VRCAvatarDescriptor.AnimLayerType.Additive ? "additiveIndex" : "sittingIndex";
            var i = (int)typeof(LyumaAv3Runtime).GetField(field, BindingFlags.NonPublic | BindingFlags.Instance).GetValue(runtime);
            if (i < 0) throw new Exception($"the emulator has no {type} playable");
            return i;
        }

        // Grabs the bone nearest a ray at `bone` as the editor's grab helper does, moves the held point by
        // `move` (root axes) over the step, and lets go on its last frame, posing the chain if `pose`.
        void Grab(Step st, string bone, Vector3 move, bool pose)
        {
            st.Each = (r, k) =>
            {
                var f = r.frame.ToString(r.inv);
                if (k == 0)
                {
                    var t = r.root.GetComponentsInChildren<Transform>(true).First(x => x.name == bone);
                    var at = t.parent.position + (t.position - t.parent.position) * 0.75f;
                    var from = at + r.root.rotation * new Vector3(0, 0, 0.5f);
                    var player = VRC.SDKBase.Networking.LocalPlayer;
                    r.grab = PhysBoneManager.Inst.AttemptGrab(player != null && player.IsValid() ? player.playerId : -1, new Ray(from, at - from), out var origin);
                    var o = r.Local(origin);
                    r.grabsOut.Write($"{f}\t{(r.grab == null ? "miss" : "grab")}\t{bone}\t{r.F(o.x)}\t{r.F(o.y)}\t{r.F(o.z)}\n");
                    if (r.grab != null) r.held = r.grab.GlobalPosition;
                    return;
                }
                if (r.grab == null) return;
                if (k == st.Frames - 1)
                {
                    PhysBoneManager.Inst.ReleaseGrab(r.grab, pose);
                    r.grab = null;
                    r.grabsOut.Write($"{f}\t{(pose ? "pose" : "release")}\t{bone}\t-\t-\t-\n");
                    return;
                }
                float s = Mathf.SmoothStep(0, 1, Mathf.Min(1, k / (st.Frames * 0.6f)));
                r.grab.GlobalPosition = r.held + r.root.rotation * (move * s);
                var p = r.Local(r.grab.GlobalPosition);
                r.grabsOut.Write($"{f}\thold\t{bone}\t{r.F(p.x)}\t{r.F(p.y)}\t{r.F(p.z)}\n");
            };
        }

        Step Add(Step s) { steps.Add(s); return s; }

        void Reset(List<string> names, int frames = 45)
        {
            var st = Add(new Step { Label = "reset", Frames = frames });
            foreach (var n in names) st.Set[n] = Rest(n);
        }

        float Rest(string name)
        {
            foreach (var (n, v) in Builtins) if (n == name) return v;
            return defaults.TryGetValue(name, out var d) ? d : 0;
        }

        void Menu(VRCExpressionsMenu menu, string path, string names, HashSet<VRCExpressionsMenu> seen, Dictionary<string, float> held)
        {
            if (!seen.Add(menu)) return;
            int index = 0;
            foreach (var c in menu.controls)
            {
                var cpath = $"{path}{index++:00}:{Clean(c.name)}";
                var cnames = names + Clean(c.name);
                menuRows.Append($"{cpath}\t{cnames}\t{c.type}\t{c.parameter?.name}\n");
                var open = new Dictionary<string, float>(held);
                var p = c.parameter?.name;
                if (!string.IsNullOrEmpty(p) && c.type != Control.ControlType.Toggle && c.type != Control.ControlType.Button) open[p] = c.value;
                string Sub(int i) => c.subParameters != null && c.subParameters.Length > i ? c.subParameters[i]?.name : null;
                switch (c.type)
                {
                    case Control.ControlType.Toggle:
                    case Control.ControlType.Button:
                        if (string.IsNullOrEmpty(p)) break;
                        var on = Mathf.Approximately(Rest(p), c.value) ? 0 : c.value;
                        var hold = c.type == Control.ControlType.Button ? 20 : 45;
                        Sweep(cpath, $"{c.type} {cpath} {p}={F(on)}", hold, open, new Dictionary<string, float> { [p] = on });
                        Reset(open.Keys.Append(p).ToList());
                        break;
                    case Control.ControlType.RadialPuppet:
                        if (string.IsNullOrEmpty(Sub(0))) break;
                        foreach (var v in new[] { 0f, 0.25f, 0.5f, 0.75f, 1f })
                            Sweep(cpath, $"Radial {cpath} {Sub(0)}={F(v)}", 30, open, new Dictionary<string, float> { [Sub(0)] = v });
                        Reset(open.Keys.Append(Sub(0)).ToList());
                        break;
                    case Control.ControlType.TwoAxisPuppet:
                        foreach (var (x, y) in new[] { (0f, 1f), (1f, 0f), (0f, -1f), (-1f, 0f) })
                        {
                            var set = new Dictionary<string, float>();
                            if (!string.IsNullOrEmpty(Sub(0))) set[Sub(0)] = x;
                            if (!string.IsNullOrEmpty(Sub(1))) set[Sub(1)] = y;
                            if (set.Count > 0) Sweep(cpath, $"TwoAxis {cpath} ({F(x)},{F(y)})", 30, open, set);
                        }
                        Reset(open.Keys.Concat(new[] { Sub(0), Sub(1) }.Where(s => !string.IsNullOrEmpty(s))).ToList());
                        break;
                    case Control.ControlType.FourAxisPuppet:
                        for (int i = 0; i < 4; i++)
                        {
                            if (string.IsNullOrEmpty(Sub(i))) continue;
                            var set = Enumerable.Range(0, 4).Select(Sub).Where(s => !string.IsNullOrEmpty(s)).Distinct().ToDictionary(s => s, s => 0f);
                            set[Sub(i)] = 1;
                            Sweep(cpath, $"FourAxis {cpath} {Sub(i)}=1", 30, open, set);
                        }
                        Reset(open.Keys.Concat(Enumerable.Range(0, 4).Select(Sub).Where(s => !string.IsNullOrEmpty(s))).Distinct().ToList());
                        break;
                    case Control.ControlType.SubMenu:
                        if (c.subMenu != null) Menu(c.subMenu, cpath + "/", cnames + "/", seen, open);
                        break;
                }
            }
            seen.Remove(menu);
        }

        void Sweep(string control, string label, int frames, Dictionary<string, float> open, Dictionary<string, float> set)
        {
            var st = Add(new Step { Label = label, Control = control, Frames = frames, Render = true });
            foreach (var kv in open) st.Set[kv.Key] = kv.Value;
            foreach (var kv in set) st.Set[kv.Key] = kv.Value;
        }

        void BeginStep(Step s)
        {
            s.First = frame;
            foreach (var kv in s.Set) Apply(kv.Key, kv.Value);
            stepsOut.Write($"{stepIndex}\t{frame}\t{s.Frames}\t{s.Control ?? "-"}\t{Clean(s.Label)}\t{string.Join(",", s.Set.Select(kv => kv.Key + "=" + F(kv.Value)))}\n");
        }

        static string Escape(string s) => s.Replace("\\", "\\\\").Replace("\n", "\\n").Replace("\t", "\\t").Replace("\r", "\\r");

        static string Q(string s) => "\"" + s.Replace("\\", "\\\\").Replace("\"", "\\\"").Replace("\n", "\\n").Replace("\t", "\\t") + "\"";

        void SetupCamera()
        {
            foreach (var l in UnityEngine.Object.FindObjectsOfType<Light>()) l.enabled = false;
            RenderSettings.ambientMode = UnityEngine.Rendering.AmbientMode.Flat;
            RenderSettings.ambientLight = new Color(0.35f, 0.35f, 0.35f);
            RenderSettings.skybox = null;
            RenderSettings.fog = false;
            var renderers = root.GetComponentsInChildren<SkinnedMeshRenderer>().Where(r => r.enabled && r.gameObject.activeInHierarchy).Cast<Renderer>().ToList();
            var bounds = renderers.Count > 0 ? renderers[0].bounds : new Bounds(root.position + Vector3.up, Vector3.one);
            foreach (var r in renderers) bounds.Encapsulate(r.bounds);
            var height = Mathf.Max(bounds.size.y, 0.5f);
            var center = Local(bounds.center);
            const float fov = 30;
            var distance = height * 0.5f * 1.15f / Mathf.Tan(fov * 0.5f * Mathf.Deg2Rad) + bounds.extents.z;
            var camPos = new Vector3(center.x, center.y, center.z + distance);
            var go = new GameObject("AWCamera");
            go.transform.position = root.position + root.rotation * camPos;
            go.transform.rotation = root.rotation * Quaternion.Euler(0, 180, 0);
            camera = go.AddComponent<Camera>();
            camera.fieldOfView = fov;
            camera.nearClipPlane = 0.05f;
            camera.farClipPlane = 50;
            camera.clearFlags = CameraClearFlags.SolidColor;
            camera.backgroundColor = new Color(0.5f, 0.5f, 0.5f, 1);
            camera.enabled = false;
            const int w = 384, h = 640;
            target = new RenderTexture(w, h, 24, RenderTextureFormat.ARGB32) { antiAliasing = 1 };
            camera.targetTexture = target;
            camera.aspect = w / (float)h;
            var light = new GameObject("AWLight").AddComponent<Light>();
            light.type = LightType.Directional;
            light.color = Color.white;
            light.intensity = 1;
            light.shadows = LightShadows.None;
            var lightRot = Quaternion.Euler(40, 160, 0);
            light.transform.rotation = root.rotation * lightRot;
            File.WriteAllText(Path.Combine(output, "camera.json"), Json(new Dictionary<string, object>
            {
                ["space"] = "avatar root position and rotation, unscaled: Unity axes (left-handed, +Z forward), metres",
                ["position"] = new[] { camPos.x, camPos.y, camPos.z },
                ["rotation_euler"] = new[] { 0f, 180f, 0f },
                ["fov_vertical"] = fov,
                ["width"] = w, ["height"] = h,
                ["background"] = new[] { 0.5f, 0.5f, 0.5f },
                ["ambient"] = new[] { 0.35f, 0.35f, 0.35f },
                ["light_euler"] = new[] { 40f, 160f, 0f },
                ["light_intensity"] = 1f,
            }));
        }

        void Render(Step s)
        {
            camera.Render();
            var prev = RenderTexture.active;
            RenderTexture.active = target;
            var tex = new Texture2D(target.width, target.height, TextureFormat.RGB24, false);
            tex.ReadPixels(new Rect(0, 0, target.width, target.height), 0, 0);
            tex.Apply();
            RenderTexture.active = prev;
            var name = $"render/{renderCount++:0000}.png";
            File.WriteAllBytes(Path.Combine(output, name), tex.EncodeToPNG());
            UnityEngine.Object.DestroyImmediate(tex);
            rendersOut.Write($"{name}\t{frame}\t{stepIndex}\t{s.Control ?? "-"}\t{s.Label}\n");
        }

        void Complete()
        {
            Finished = true;
            foreach (var w in new[] { paramsOut, statesOut, tipsOut, valuesOut, stepsOut, rendersOut, trackOut, posesOut, grabsOut, inputsOut, timeOut, dynamicsOut, clocksOut }) w?.Dispose();
            CheckInputs();
            File.WriteAllText(Path.Combine(output, "done"), frame.ToString(inv) + "\n");
            Debug.Log($"[aw-reference] recorded {frame} frames, {steps.Count} steps, {tips.Count} tips, {renderCount} renders into {output}");
            Harness.Finish(0);
        }

        // Reads the log back, so what fails is the file a replay would read.
        void CheckInputs()
        {
            var rows = File.ReadAllLines(Path.Combine(output, "inputs.tsv")).Select(l => l.Split('\t')).ToArray();
            int first = steps[0].First;
            for (int f = first; f < frame; f++)
                if (f - first >= rows.Length || rows[f - first][0] != f.ToString(inv)) throw new Exception($"inputs.tsv lacks the scripted inputs of frame {f}");
            if (rows.Length != frame - first) throw new Exception($"inputs.tsv holds {rows.Length} rows for {frame - first} frames");
            foreach (var s in steps)
            {
                var missing = s.Set.Select(kv => Escape(kv.Key) + "=" + F(kv.Value)).Except(rows[s.First - first].Skip(3)).ToList();
                if (missing.Count > 0) throw new Exception($"inputs.tsv lacks {string.Join(", ", missing)} at frame {s.First}, where step '{s.Label}' sets them");
            }
        }

        static string Json(object o)
        {
            switch (o)
            {
                case null: return "null";
                case string s: return Q(s);
                case bool b: return b ? "true" : "false";
                case int i: return i.ToString(CultureInfo.InvariantCulture);
                case float f: return f.ToString("R", CultureInfo.InvariantCulture);
                case float[] a: return "[" + string.Join(",", a.Select(x => Json(x))) + "]";
                case IDictionary<string, string> d: return "{" + string.Join(",", d.Select(kv => Q(kv.Key) + ":" + Q(kv.Value))) + "}";
                case IDictionary<string, int> d: return "{" + string.Join(",", d.Select(kv => Q(kv.Key) + ":" + Json(kv.Value))) + "}";
                case Dictionary<string, object> d: return "{" + string.Join(",", d.Select(kv => Q(kv.Key) + ":" + Json(kv.Value))) + "}";
                default: return Q(o.ToString());
            }
        }
    }
}
