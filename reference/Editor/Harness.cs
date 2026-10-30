using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Text;
using Lyuma.Av3Emulator.Runtime;
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
using Control = VRC.SDK3.Avatars.ScriptableObjects.VRCExpressionsMenu.Control;

namespace AvatarWorkshop.Reference
{
    public static class Harness
    {
        public const int Rate = 60;

        static Recorder recorder;
        static (bool enabled, EnterPlayModeOptions options)? playModeSettings;

        public static void Run()
        {
            try { Begin(); }
            catch (Exception e) { Fail(e); }
        }

        static string Env(string name)
        {
            var v = Environment.GetEnvironmentVariable(name);
            return string.IsNullOrEmpty(v) ? null : v;
        }

        static void Begin()
        {
            var output = Env("AW_OUT") ?? throw new Exception("AW_OUT is required");
            var scenePath = Env("AW_SCENE");
            var avatarName = Env("AW_AVATAR");
            var fixture = scenePath == null;
            if (fixture)
            {
                Fixture.Build();
                scenePath = Fixture.ScenePath;
            }

            EditorSceneManager.OpenScene(scenePath, OpenSceneMode.Single);
            var descriptors = UnityEngine.Object.FindObjectsOfType<VRCAvatarDescriptor>(true);
            var chosen = descriptors.Where(d => avatarName == null ? d.gameObject.activeInHierarchy : d.gameObject.name == avatarName).ToList();
            if (chosen.Count != 1) throw new Exception($"scene {scenePath} has {chosen.Count} avatars named '{avatarName}' among {descriptors.Length}");
            var avatar = chosen[0].gameObject;
            avatar.SetActive(true);
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
            recorder = new Recorder(output, avatar.name, fixture);
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

        static void Install()
        {
            var loop = PlayerLoop.GetCurrentPlayerLoop();
            for (int i = 0; i < loop.subSystemList.Length; i++)
            {
                var sys = loop.subSystemList[i];
                var list = sys.subSystemList.ToList();
                if (sys.type == typeof(Update))
                    list.Insert(0, new PlayerLoopSystem { type = typeof(InputsStage), updateDelegate = () => Guard(recorder.Inputs) });
                if (sys.type == typeof(PreLateUpdate))
                {
                    int at = list.FindIndex(s => s.type == typeof(PreLateUpdate.ScriptRunBehaviourLateUpdate));
                    list.Insert(at < 0 ? 0 : at, new PlayerLoopSystem { type = typeof(PoseStage), updateDelegate = () => Guard(recorder.Pose) });
                }
                if (sys.type == typeof(PostLateUpdate))
                    list.Add(new PlayerLoopSystem { type = typeof(RecordStage), updateDelegate = () => Guard(recorder.Record) });
                sys.subSystemList = list.ToArray();
                loop.subSystemList[i] = sys;
            }
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

        static void Fail(Exception e)
        {
            Debug.LogError("[aw-reference] FAILED " + e);
            var output = Env("AW_OUT");
            if (output != null) { Directory.CreateDirectory(output); File.WriteAllText(Path.Combine(output, "error.txt"), e.ToString()); }
            if (recorder != null) recorder.Finished = true;
            Finish(1);
        }
    }

    class Step
    {
        public string Label;
        public string Control;
        public int Frames;
        public Dictionary<string, float> Set = new Dictionary<string, float>();
        public Action<Recorder, int> Each;
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
        readonly bool fixture;
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
        readonly Dictionary<string, float> defaults = new Dictionary<string, float>();
        readonly Dictionary<int, string> stateNames = new Dictionary<int, string>();
        readonly List<(Renderer r, string path, string prop, ShaderPropertyType type)> materialProps = new List<(Renderer, string, string, ShaderPropertyType)>();
        readonly List<(Transform t, string path)> animatedTransforms = new List<(Transform, string)>();
        List<AnimatorControllerPlayable> playables;

        StreamWriter paramsOut, statesOut, tipsOut, valuesOut, stepsOut, rendersOut, trackOut;
        readonly Dictionary<string, string> lastParams = new Dictionary<string, string>();
        readonly Dictionary<string, string> lastStates = new Dictionary<string, string>();
        Dictionary<string, string> lastValues = new Dictionary<string, string>();
        int renderCount;
        Camera camera;
        RenderTexture target;

        public Recorder(string output, string avatarName, bool fixture)
        {
            this.output = output;
            this.avatarName = avatarName;
            this.fixture = fixture;
        }

        string F(float v) => v.ToString("R", inv);

        string Xf(Vector3 p, Quaternion q) => $"{F(p.x)},{F(p.y)},{F(p.z)},{F(q.x)},{F(q.y)},{F(q.z)},{F(q.w)}";

        public void Inputs()
        {
            frame++;
            if (runtime == null)
            {
                if (frame > 600) throw new Exception("Av3Emulator never attached a runtime to the avatar");
                runtime = UnityEngine.Object.FindObjectsOfType<LyumaAv3Runtime>().FirstOrDefault(r => !r.IsMirrorClone && !r.IsShadowClone && r.GetComponent<VRCAvatarDescriptor>() != null);
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

        public void Record()
        {
            if (root == null || stepIndex < 0 || Finished) return;
            var f = frame.ToString(inv);
            foreach (var (name, value) in Parameters())
                if (!lastParams.TryGetValue(name, out var old) || old != value) { paramsOut.Write($"{f}\t{name}\t{value}\n"); lastParams[name] = value; }
            foreach (var (layer, value) in States())
                if (!lastStates.TryGetValue(layer, out var old) || old != value) { statesOut.Write($"{f}\t{layer}\t{value}\n"); lastStates[layer] = value; }
            foreach (var tip in tips)
            {
                if (tip.Leaf == null) continue;
                var p = root.InverseTransformPoint(tip.Leaf.TransformPoint(tip.Endpoint));
                tipsOut.Write($"{f}\t{tip.Id}\t{F(p.x)}\t{F(p.y)}\t{F(p.z)}\n");
            }
            var line = new StringBuilder(f);
            line.Append('\t').Append(Xf(root.position, root.rotation));
            foreach (var b in tracked)
                line.Append('\t').Append(b == null ? "-" : Xf(root.InverseTransformPoint(b.position), Quaternion.Inverse(root.rotation) * b.rotation));
            trackOut.Write(line.Append('\n').ToString());
            var values = Values();
            foreach (var kv in values)
                if (!lastValues.TryGetValue(kv.Key, out var old) || old != kv.Value) valuesOut.Write($"{f}\t{kv.Key}\t{kv.Value}\n");
            foreach (var k in lastValues.Keys.Where(k => !values.ContainsKey(k))) valuesOut.Write($"{f}\t{k}\t-\n");
            lastValues = values;

            var step = steps[stepIndex];
            if (stepFrame == step.Frames - 1 && step.Render) Render(step);
            stepFrame++;
        }

        void Setup()
        {
            root = runtime.transform;
            rootPos = root.position;
            rootRot = root.rotation;
            var animator = runtime.GetComponent<Animator>();
            if (animator != null && animator.isHuman)
            {
                leftArm = animator.GetBoneTransform(HumanBodyBones.LeftUpperArm);
                rightArm = animator.GetBoneTransform(HumanBodyBones.RightUpperArm);
                tracked = new[] { HumanBodyBones.Head, HumanBodyBones.LeftHand, HumanBodyBones.RightHand, HumanBodyBones.Hips }.Select(animator.GetBoneTransform).ToArray();
            }
            playables = (List<AnimatorControllerPlayable>)typeof(LyumaAv3Runtime).GetField("playables", BindingFlags.NonPublic | BindingFlags.Instance).GetValue(runtime);
            var descriptor = runtime.GetComponent<VRCAvatarDescriptor>();
            if (descriptor.expressionParameters != null)
                foreach (var p in descriptor.expressionParameters.parameters)
                    if (!string.IsNullOrEmpty(p.name)) defaults[p.name] = p.defaultValue;

            var controllers = descriptor.baseAnimationLayers.Concat(descriptor.specialAnimationLayers ?? new VRCAvatarDescriptor.CustomAnimLayer[0])
                .Select(l => l.animatorController as AnimatorController).Where(c => c != null).Distinct().ToList();
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
            foreach (var path in bindings.Where(b => b.type == typeof(Transform)).Select(b => b.path).Distinct().OrderBy(p => p, StringComparer.Ordinal))
            {
                var t = root.Find(path);
                if (t != null) animatedTransforms.Add((t, path));
            }

            foreach (var pb in root.GetComponentsInChildren<VRCPhysBoneBase>(true).OrderBy(p => PathOf(p.transform), StringComparer.Ordinal))
                Tips(pb);

            Directory.CreateDirectory(output);
            Directory.CreateDirectory(Path.Combine(output, "render"));
            paramsOut = Open("params.tsv");
            statesOut = Open("states.tsv");
            tipsOut = Open("tips.tsv");
            valuesOut = Open("values.tsv");
            stepsOut = Open("steps.tsv");
            rendersOut = Open("renders.tsv");
            trackOut = Open("track.tsv");
            trackOut.Write("frame\troot (world)\thead\tleft hand\tright hand\thips (each in root space: px,py,pz,qx,qy,qz,qw)\n");
            File.WriteAllText(Path.Combine(output, "chains.tsv"),
                "tip\tlength\n" + string.Concat(tips.Select(t => $"{t.Id}\t{F(t.Length)}\n")));
            var coverage = Coverage.Count(root.gameObject);
            File.WriteAllText(Path.Combine(output, "coverage.json"), Json(coverage) + "\n");
            var missing = Coverage.Required.Where(k => coverage[k] == 0).ToList();
            if (fixture && missing.Count > 0) throw new Exception("fixture lacks " + string.Join(", ", missing));
            SetupCamera();
            Schedule(descriptor);
            File.WriteAllText(Path.Combine(output, "menu.tsv"), menuRows.ToString());
            File.WriteAllText(Path.Combine(output, "meta.json"), Json(new Dictionary<string, object>
            {
                ["avatar"] = avatarName,
                ["fixture"] = fixture,
                ["unity"] = Application.unityVersion,
                ["rate"] = Harness.Rate,
                ["steps"] = steps.Count,
                ["frames"] = steps.Sum(s => s.Frames),
                ["tips"] = tips.Count,
                ["physbones"] = root.GetComponentsInChildren<VRCPhysBoneBase>(true).Length,
                ["contacts"] = root.GetComponentsInChildren<ContactBase>(true).Length,
                ["constraints"] = root.GetComponentsInChildren<VRCConstraintBase>(true).Length,
                ["packages"] = Packages(),
            }));
        }

        static string Base(string property)
        {
            var p = property.Substring("material.".Length);
            int dot = p.LastIndexOf('.');
            return dot > 0 && p.Length - dot == 2 && "rgbaxyzw".Contains(p[dot + 1]) ? p.Substring(0, dot) : p;
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

        string PathOf(Transform t)
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

        Vector3 Local(Vector3 world) => root.InverseTransformPoint(world);

        void Tips(VRCPhysBoneBase pb)
        {
            var start = pb.rootTransform != null ? pb.rootTransform : pb.transform;
            var ignored = new HashSet<Transform>((pb.ignoreTransforms ?? new List<Transform>()).Where(x => x != null));
            void Walk(Transform t, float length)
            {
                var kids = Enumerable.Range(0, t.childCount).Select(t.GetChild).Where(c => !ignored.Contains(c)).ToList();
                if (kids.Count == 0)
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
                foreach (var k in kids) Walk(k, length + (Local(k.position) - Local(t.position)).magnitude);
            }
            Walk(start, 0);
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
            foreach (var (t, path) in animatedTransforms)
            {
                var p = t.localPosition; var e = t.localRotation; var s = t.localScale;
                v[$"xform:{path}"] = $"{F(p.x)},{F(p.y)},{F(p.z)},{F(e.x)},{F(e.y)},{F(e.z)},{F(e.w)},{F(s.x)},{F(s.y)},{F(s.z)}";
            }
            return v;
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

            var rest = S("rest", 90, render: true);
            foreach (var (n, v) in Builtins) rest.Set[n] = v;

            var motion = S("motion", 6 * Harness.Rate);
            motion.Each = (r, k) =>
            {
                float t = k / (float)Harness.Rate;
                const float tau = Mathf.PI * 2;
                var offset = new Vector3(0.25f * Mathf.Sin(tau * t / 3), 0.04f * Mathf.Sin(tau * t / 0.75f), 0.4f * Mathf.Sin(tau * t / 2));
                r.root.SetPositionAndRotation(r.rootPos + r.rootRot * offset, r.rootRot * Quaternion.Euler(0, 45 * Mathf.Sin(tau * t / 6), 0));
                r.armSwing = new Vector2(40 * Mathf.Sin(tau * t / 1.5f), 70 * Mathf.Sin(tau * t / 2));
            };
            var stop = S("settle", 2 * Harness.Rate, render: true);
            stop.Each = (r, k) => { if (k == 0) { r.root.SetPositionAndRotation(r.rootPos, r.rootRot); r.armSwing = Vector2.zero; } };

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
            S("final", 60, render: true);
        }

        Step Add(Step s) { steps.Add(s); return s; }

        void Reset(List<string> names)
        {
            var st = Add(new Step { Label = "reset", Frames = 45 });
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
                var cpath = $"{path}{index++:00}:{c.name}";
                var cnames = names + c.name;
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
            foreach (var kv in s.Set) Apply(kv.Key, kv.Value);
            stepsOut.Write($"{stepIndex}\t{frame}\t{s.Frames}\t{s.Control ?? "-"}\t{s.Label}\t{string.Join(",", s.Set.Select(kv => kv.Key + "=" + F(kv.Value)))}\n");
        }

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
            var center = root.InverseTransformPoint(bounds.center);
            const float fov = 30;
            var distance = height * 0.5f * 1.15f / Mathf.Tan(fov * 0.5f * Mathf.Deg2Rad) + bounds.extents.z;
            var camPos = new Vector3(center.x, center.y, center.z + distance);
            var go = new GameObject("AWCamera");
            go.transform.position = root.TransformPoint(camPos);
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
                ["space"] = "avatar root, Unity axes (left-handed, +Z forward, metres)",
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
            foreach (var w in new[] { paramsOut, statesOut, tipsOut, valuesOut, stepsOut, rendersOut, trackOut }) w?.Dispose();
            File.WriteAllText(Path.Combine(output, "done"), frame.ToString(inv) + "\n");
            Debug.Log($"[aw-reference] recorded {frame} frames, {steps.Count} steps, {tips.Count} tips, {renderCount} renders into {output}");
            Harness.Finish(0);
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
