using System;
using System.Collections.Generic;
using System.Linq;
using UnityEditor;
using UnityEditor.Animations;
using UnityEngine;
using UnityEngine.Animations;
using VRC.Dynamics;
using VRC.SDK3.Avatars.Components;
using VRC.SDK3.Avatars.ScriptableObjects;
using VRC.SDKBase;

namespace AvatarWorkshop.Exporter
{
    sealed partial class Export
    {
        readonly Dictionary<AnimationClip, int> clipIndex = new Dictionary<AnimationClip, int>();
        readonly List<object> clipRecords = new List<object>();
        readonly Dictionary<StateMachineBehaviour, int> behaviourIndex = new Dictionary<StateMachineBehaviour, int>();
        readonly List<object> behaviourRecords = new List<object>();
        readonly HashSet<string> animatorParameters = new HashSet<string>();
        readonly List<Action> pendingClips = new List<Action>();

        static string LayerKey(VRCAvatarDescriptor.AnimLayerType t) => t switch
        {
            VRCAvatarDescriptor.AnimLayerType.Base => "base",
            VRCAvatarDescriptor.AnimLayerType.Additive => "additive",
            VRCAvatarDescriptor.AnimLayerType.Gesture => "gesture",
            VRCAvatarDescriptor.AnimLayerType.Action => "action",
            VRCAvatarDescriptor.AnimLayerType.FX => "fx",
            VRCAvatarDescriptor.AnimLayerType.Sitting => "sitting",
            VRCAvatarDescriptor.AnimLayerType.TPose => "tPose",
            VRCAvatarDescriptor.AnimLayerType.IKPose => "ikPose",
            _ => null,
        };

        static readonly string[] LayerOrder = { "base", "additive", "gesture", "action", "fx", "sitting", "tPose", "ikPose" };

        Obj Layers()
        {
            var layers = new SortedDictionary<int, KeyValuePair<string, Obj>>();
            foreach (var l in Playable.Layers(descriptor))
            {
                var key = LayerKey(l.type);
                if (key == null) continue;
                var record = new Obj();
                if (Playable.Controller(l) is RuntimeAnimatorController c) record.Add("controller", Controller(c));
                if (Playable.Mask(l) is AvatarMask mask) record.Add("mask", Mask(mask));
                layers[Array.IndexOf(LayerOrder, key)] = new KeyValuePair<string, Obj>(key, record);
            }
            // A clip record numbers the clip its additive reference pose names, which appends to the list.
            for (int i = 0; i < pendingClips.Count; i++) pendingClips[i]();
            var o = new Obj();
            foreach (var kv in layers.Values) o.Add(kv.Key, kv.Value);
            return o;
        }

        Obj Mask(AvatarMask mask)
        {
            var parts = new List<object>();
            for (var p = AvatarMaskBodyPart.Root; p < AvatarMaskBodyPart.LastBodyPart; p++)
                if (mask.GetHumanoidBodyPartActive(p)) parts.Add(p.ToString());
            var transforms = new List<object>();
            for (int i = 0; i < mask.transformCount; i++)
            {
                var path = mask.GetTransformPath(i);
                var t = string.IsNullOrEmpty(path) ? root : root.Find(path);
                if (t == null || !nodeIndex.ContainsKey(t)) { Drop("avatar mask path with no node"); continue; }
                transforms.Add(new Obj { { "node", nodeIndex[t] }, { "active", mask.GetTransformActive(i) } });
            }
            return new Obj { { "bodyParts", parts }, { "transforms", transforms } };
        }

        sealed class ControllerIds
        {
            public readonly Dictionary<AnimatorState, int> states = new Dictionary<AnimatorState, int>();
            public readonly Dictionary<AnimatorStateMachine, int> machines = new Dictionary<AnimatorStateMachine, int>();
            public Dictionary<AnimationClip, AnimationClip> overrides = new Dictionary<AnimationClip, AnimationClip>();
            public readonly HashSet<AnimatorState> layerStates = new HashSet<AnimatorState>();
            public readonly HashSet<AnimatorStateMachine> layerMachines = new HashSet<AnimatorStateMachine>();
        }

        Obj Controller(RuntimeAnimatorController runtime)
        {
            var ids = new ControllerIds();
            var controller = runtime as AnimatorController;
            if (runtime is AnimatorOverrideController ovr)
            {
                controller = ovr.runtimeAnimatorController as AnimatorController;
                var pairs = new List<KeyValuePair<AnimationClip, AnimationClip>>();
                ovr.GetOverrides(pairs);
                foreach (var kv in pairs) if (kv.Value != null) ids.overrides[kv.Key] = kv.Value;
            }
            if (controller == null)
            {
                Drop("animator controller of type " + runtime.GetType().Name);
                return new Obj { { "name", runtime.name }, { "parameters", new object[0] }, { "layers", new object[0] } };
            }
            foreach (var l in controller.layers) Number(l.stateMachine, ids);
            var parameters = controller.parameters.Select(p =>
            {
                animatorParameters.Add(p.name);
                Obj value = p.type switch
                {
                    AnimatorControllerParameterType.Float => new Obj { { "float", p.defaultFloat } },
                    AnimatorControllerParameterType.Int => new Obj { { "int", p.defaultInt } },
                    AnimatorControllerParameterType.Bool => new Obj { { "bool", p.defaultBool } },
                    _ => new Obj { { "trigger", p.defaultBool } },
                };
                return (object)new Obj { { "name", p.name }, { "default", value } };
            }).ToList();
            var layers = new List<object>();
            var all = controller.layers;
            foreach (var l in all)
            {
                var record = new Obj { { "name", l.name }, { "defaultWeight", l.defaultWeight }, { "blending", l.blendingMode == AnimatorLayerBlendingMode.Additive ? "additive" : "override" } };
                if (l.avatarMask != null) record.Add("mask", Mask(l.avatarMask));
                record.Add("ikPass", l.iKPass);
                if (l.syncedLayerIndex >= 0 && l.syncedLayerIndex < all.Length)
                {
                    var states = new List<object>();
                    foreach (var s in States(all[l.syncedLayerIndex].stateMachine))
                    {
                        var o = new Obj { { "state", ids.states[s] } };
                        if (l.GetOverrideMotion(s) is Motion m && MotionRecord(m, ids) is object mr) o.Add("motion", mr);
                        o.Add("behaviours", Behaviours(l.GetOverrideBehaviours(s)));
                        states.Add(o);
                    }
                    record.Add("content", new Obj { { "type", "synced" }, { "layer", l.syncedLayerIndex }, { "affectsTiming", l.syncedLayerAffectsTiming }, { "states", states } });
                }
                else
                {
                    ids.layerStates.Clear();
                    ids.layerStates.UnionWith(States(l.stateMachine));
                    ids.layerMachines.Clear();
                    ids.layerMachines.UnionWith(Machines(l.stateMachine));
                    record.Add("content", new Obj { { "type", "stateMachine" }, { "stateMachine", Machine(l.stateMachine, null, ids) } });
                }
                layers.Add(record);
            }
            return new Obj { { "name", runtime.name }, { "parameters", parameters }, { "layers", layers } };
        }

        static IEnumerable<AnimatorState> States(AnimatorStateMachine m)
        {
            if (m == null) yield break;
            foreach (var s in m.states) yield return s.state;
            foreach (var sub in m.stateMachines)
                foreach (var s in States(sub.stateMachine)) yield return s;
        }

        static IEnumerable<AnimatorStateMachine> Machines(AnimatorStateMachine m)
        {
            if (m == null) yield break;
            yield return m;
            foreach (var sub in m.stateMachines)
                foreach (var s in Machines(sub.stateMachine)) yield return s;
        }

        static void Number(AnimatorStateMachine m, ControllerIds ids)
        {
            if (m == null || ids.machines.ContainsKey(m)) return;
            ids.machines[m] = ids.machines.Count;
            foreach (var s in m.states) if (!ids.states.ContainsKey(s.state)) ids.states[s.state] = ids.states.Count;
            foreach (var sub in m.stateMachines) Number(sub.stateMachine, ids);
        }

        Obj Machine(AnimatorStateMachine m, AnimatorStateMachine parent, ControllerIds ids)
        {
            var o = new Obj { { "id", ids.machines[m] }, { "name", m.name } };
            o.Add("states", m.states.Select(s => (object)StateRecord(s.state, ids)).ToList());
            o.Add("stateMachines", m.stateMachines.Select(s => (object)Machine(s.stateMachine, m, ids)).ToList());
            if (m.defaultState != null && ids.states.TryGetValue(m.defaultState, out int d)) o.Add("defaultState", d);
            o.Add("anyStateTransitions", m.anyStateTransitions.Select(t => (object)Transition(t, ids)).Where(t => t != null).ToList());
            o.Add("entryTransitions", m.entryTransitions.Select(t => (object)Entry(t, ids)).Where(t => t != null).ToList());
            var exits = parent == null ? new AnimatorTransition[0] : parent.GetStateMachineTransitions(m);
            o.Add("stateMachineTransitions", exits.Select(t => (object)Entry(t, ids)).Where(t => t != null).ToList());
            o.Add("behaviours", Behaviours(m.behaviours));
            return o;
        }

        Obj StateRecord(AnimatorState s, ControllerIds ids)
        {
            var o = new Obj { { "id", ids.states[s] }, { "name", s.name } };
            if (!string.IsNullOrEmpty(s.tag)) o.Add("tag", s.tag);
            o.Add("speed", s.speed);
            if (s.speedParameterActive && !string.IsNullOrEmpty(s.speedParameter)) o.Add("speedParameter", s.speedParameter);
            o.Add("cycleOffset", s.cycleOffset);
            if (s.cycleOffsetParameterActive && !string.IsNullOrEmpty(s.cycleOffsetParameter)) o.Add("cycleOffsetParameter", s.cycleOffsetParameter);
            o.Add("mirror", s.mirror);
            if (s.mirrorParameterActive && !string.IsNullOrEmpty(s.mirrorParameter)) o.Add("mirrorParameter", s.mirrorParameter);
            if (s.timeParameterActive && !string.IsNullOrEmpty(s.timeParameter)) o.Add("timeParameter", s.timeParameter);
            if (s.motion != null && MotionRecord(s.motion, ids) is object m) o.Add("motion", m);
            o.Add("writeDefaults", s.writeDefaultValues);
            o.Add("ikOnFeet", s.iKOnFeet);
            o.Add("behaviours", Behaviours(s.behaviours));
            o.Add("transitions", s.transitions.Select(t => (object)Transition(t, ids)).Where(t => t != null).ToList());
            return o;
        }

        Obj Destination(AnimatorTransitionBase t, ControllerIds ids)
        {
            if ((t.destinationState != null && !ids.layerStates.Contains(t.destinationState)) || (t.destinationStateMachine != null && !ids.layerMachines.Contains(t.destinationStateMachine)))
            {
                Drop("transition to another layer's state");
                return null;
            }
            if (t.destinationState != null && ids.states.TryGetValue(t.destinationState, out int s)) return new Obj { { "type", "state" }, { "id", s } };
            if (t.destinationStateMachine != null && ids.machines.TryGetValue(t.destinationStateMachine, out int m)) return new Obj { { "type", "stateMachine" }, { "id", m } };
            if (!t.isExit) Drop("transition with no destination");
            return new Obj { { "type", "exit" } };
        }

        static List<object> Conditions(AnimatorTransitionBase t) => t.conditions.Select(c => (object)new Obj
        {
            { "mode", c.mode switch { AnimatorConditionMode.If => "if", AnimatorConditionMode.IfNot => "ifNot", AnimatorConditionMode.Greater => "greater", AnimatorConditionMode.Less => "less", AnimatorConditionMode.Equals => "equals", _ => "notEqual" } },
            { "parameter", c.parameter }, { "threshold", c.threshold },
        }).ToList();

        Obj Transition(AnimatorStateTransition t, ControllerIds ids)
        {
            if (!(Destination(t, ids) is Obj destination)) return null;
            var o = new Obj { { "destination", destination }, { "conditions", Conditions(t) } };
            if (t.hasExitTime) o.Add("exitTime", t.exitTime);
            o.Add("duration", t.hasFixedDuration ? new Obj { { "fixed", t.duration } } : new Obj { { "normalized", t.duration } });
            o.Add("offset", t.offset);
            o.Add("interruptionSource", t.interruptionSource switch
            {
                TransitionInterruptionSource.Source => "source", TransitionInterruptionSource.Destination => "destination",
                TransitionInterruptionSource.SourceThenDestination => "sourceThenDestination", TransitionInterruptionSource.DestinationThenSource => "destinationThenSource",
                _ => "none",
            });
            o.Add("orderedInterruption", t.orderedInterruption);
            o.Add("canTransitionToSelf", t.canTransitionToSelf);
            o.Add("mute", t.mute);
            o.Add("solo", t.solo);
            return o;
        }

        Obj Entry(AnimatorTransition t, ControllerIds ids) => Destination(t, ids) is Obj destination
            ? new Obj { { "destination", destination }, { "conditions", Conditions(t) }, { "mute", t.mute }, { "solo", t.solo } }
            : null;

        object MotionRecord(Motion m, ControllerIds ids)
        {
            switch (m)
            {
                case AnimationClip clip:
                    if (ids.overrides.TryGetValue(clip, out var replaced)) clip = replaced;
                    return new Obj { { "clip", Clip(clip) } };
                case BlendTree tree:
                    return new Obj { { "blendTree", BlendTreeRecord(tree, ids) } };
                default:
                    Drop("motion of type " + m.GetType().Name);
                    return null;
            }
        }

        Obj BlendTreeRecord(BlendTree tree, ControllerIds ids)
        {
            Obj Child(ChildMotion c, string kind)
            {
                var o = new Obj();
                if (c.motion != null && MotionRecord(c.motion, ids) is object m) o.Add("motion", m);
                if (kind == "1d") o.Add("threshold", c.threshold);
                else if (kind == "2d") o.Add("position", new[] { c.position.x, c.position.y });
                else o.Add("directBlendParameter", c.directBlendParameter ?? "");
                o.Add("timeScale", c.timeScale);
                o.Add("cycleOffset", c.cycleOffset);
                o.Add("mirror", c.mirror);
                return o;
            }
            Obj algorithm;
            switch (tree.blendType)
            {
                case BlendTreeType.Simple1D:
                    algorithm = new Obj { { "type", "simple1D" }, { "blendParameter", tree.blendParameter }, { "children", tree.children.Select(c => (object)Child(c, "1d")).ToList() } };
                    break;
                case BlendTreeType.Direct:
                    var normalized = new SerializedObject(tree).FindProperty("m_NormalizedBlendValues")?.boolValue ?? false;
                    algorithm = new Obj { { "type", "direct" }, { "normalizedBlendValues", normalized }, { "children", tree.children.Select(c => (object)Child(c, "direct")).ToList() } };
                    break;
                default:
                    var type = tree.blendType switch { BlendTreeType.SimpleDirectional2D => "simpleDirectional2D", BlendTreeType.FreeformDirectional2D => "freeformDirectional2D", _ => "freeformCartesian2D" };
                    algorithm = new Obj { { "type", type }, { "blendParameter", tree.blendParameter }, { "blendParameterY", tree.blendParameterY }, { "children", tree.children.Select(c => (object)Child(c, "2d")).ToList() } };
                    break;
            }
            return new Obj { { "name", tree.name }, { "algorithm", algorithm } };
        }

        List<object> Behaviours(StateMachineBehaviour[] behaviours)
        {
            var list = new List<object>();
            foreach (var b in behaviours ?? new StateMachineBehaviour[0])
            {
                if (b == null) { Drop("missing state machine behaviour"); continue; }
                if (!behaviourIndex.TryGetValue(b, out int i))
                {
                    var record = BehaviourRecord(b);
                    if (record == null) { Drop(b.GetType().Name); continue; }
                    i = behaviourRecords.Count;
                    behaviourIndex[b] = i;
                    behaviourRecords.Add(record);
                }
                list.Add(i);
            }
            return list;
        }

        static string Tracking(VRC_AnimatorTrackingControl.TrackingType t) => t switch
        {
            VRC_AnimatorTrackingControl.TrackingType.Tracking => "tracking",
            VRC_AnimatorTrackingControl.TrackingType.Animation => "animation",
            _ => "noChange",
        };

        static string Blendable(int layer) => layer switch { 0 => "action", 1 => "fx", 2 => "gesture", _ => "additive" };

        Obj BehaviourRecord(StateMachineBehaviour b)
        {
            switch (b)
            {
                case VRC_AvatarParameterDriver d:
                    var entries = new List<object>();
                    foreach (var p in d.parameters)
                    {
                        if (p.preventRepeats) Drop("parameter driver preventRepeats");
                        switch (p.type)
                        {
                            case VRC_AvatarParameterDriver.ChangeType.Set: entries.Add(new Obj { { "type", "set" }, { "name", p.name ?? "" }, { "value", p.value } }); break;
                            case VRC_AvatarParameterDriver.ChangeType.Add: entries.Add(new Obj { { "type", "add" }, { "name", p.name ?? "" }, { "value", p.value } }); break;
                            case VRC_AvatarParameterDriver.ChangeType.Random: entries.Add(new Obj { { "type", "random" }, { "name", p.name ?? "" }, { "valueMin", p.valueMin }, { "valueMax", p.valueMax }, { "chance", p.chance } }); break;
                            default:
                                var copy = new Obj { { "type", "copy" }, { "source", p.source ?? "" }, { "name", p.name ?? "" } };
                                if (p.convertRange) copy.Add("convertRange", new Obj { { "sourceMin", p.sourceMin }, { "sourceMax", p.sourceMax }, { "destMin", p.destMin }, { "destMax", p.destMax } });
                                entries.Add(copy);
                                break;
                        }
                    }
                    return new Obj { { "type", "parameterDriver" }, { "localOnly", d.localOnly }, { "parameters", entries } };
                case VRC_AnimatorTrackingControl t:
                    return new Obj
                    {
                        { "type", "trackingControl" }, { "head", Tracking(t.trackingHead) }, { "leftHand", Tracking(t.trackingLeftHand) }, { "rightHand", Tracking(t.trackingRightHand) },
                        { "hip", Tracking(t.trackingHip) }, { "leftFoot", Tracking(t.trackingLeftFoot) }, { "rightFoot", Tracking(t.trackingRightFoot) },
                        { "leftFingers", Tracking(t.trackingLeftFingers) }, { "rightFingers", Tracking(t.trackingRightFingers) }, { "eyes", Tracking(t.trackingEyes) }, { "mouth", Tracking(t.trackingMouth) },
                    };
                case VRC_AnimatorLocomotionControl l:
                    return new Obj { { "type", "locomotionControl" }, { "disableLocomotion", l.disableLocomotion } };
                case VRC_PlayableLayerControl p:
                    return new Obj { { "type", "playableLayerControl" }, { "layer", Blendable((int)p.layer) }, { "goalWeight", p.goalWeight }, { "blendDuration", p.blendDuration } };
                case VRC_AnimatorLayerControl a:
                    return new Obj { { "type", "animatorLayerControl" }, { "playable", Blendable((int)a.playable) }, { "layer", a.layer }, { "goalWeight", a.goalWeight }, { "blendDuration", a.blendDuration } };
                case VRC_AnimatorTemporaryPoseSpace t:
                    return new Obj { { "type", "temporaryPoseSpace" }, { "enterPoseSpace", t.enterPoseSpace }, { "fixedDelay", t.fixedDelay }, { "delayTime", t.delayTime } };
                default:
                    return null;
            }
        }

        // Clips are numbered when first referenced; their curves are written once every controller's
        // parameters are known, since an Animator curve is a parameter or a muscle by that list.
        int Clip(AnimationClip clip)
        {
            if (clipIndex.TryGetValue(clip, out int i)) return i;
            i = clipRecords.Count;
            clipIndex[clip] = i;
            clipRecords.Add(null);
            pendingClips.Add(() => clipRecords[i] = ClipRecord(clip));
            return i;
        }

        Obj ClipRecord(AnimationClip clip)
        {
            var curves = new List<object>();
            foreach (var binding in AnimationUtility.GetCurveBindings(clip))
            {
                var (target, negate) = Binding(binding);
                if (target == null) continue;
                var curve = AnimationUtility.GetEditorCurve(clip, binding);
                if (curve == null) continue;
                curves.Add(new Obj { { "binding", target }, { "curve", Curve(negate ? Negated(curve) : curve) } });
            }
            var slots = new List<object>();
            foreach (var binding in AnimationUtility.GetObjectReferenceCurveBindings(clip))
            {
                var t = Path(binding.path);
                const string prefix = "m_Materials.Array.data[";
                if (t != null && Drawn(binding.type) && binding.propertyName.StartsWith(prefix) && t.GetComponent(binding.type) != null
                    && int.TryParse(binding.propertyName.Substring(prefix.Length).TrimEnd(']'), out int slot))
                {
                    if (slot >= ((Renderer)t.GetComponent(binding.type)).sharedMaterials.Length) { Drop("animated material slot past the renderer's slots"); continue; }
                    var keys = AnimationUtility.GetObjectReferenceCurve(clip, binding).Select(k => (object)new Obj { { "time", k.time }, { "material", Material(k.value as Material) } }).ToList();
                    slots.Add(new Obj { { "renderer", nodeIndex[t] }, { "slot", slot }, { "keys", keys } });
                }
                else Drop(t == null ? "animation curve on a missing object" : "animated object reference " + binding.type.Name + "." + binding.propertyName.Split('.')[0]);
            }
            var settings = AnimationUtility.GetAnimationClipSettings(clip);
            var o = new Obj { { "name", clip.name }, { "length", clip.length }, { "loop", settings.loopTime }, { "cycleOffset", settings.cycleOffset } };
            if (settings.hasAdditiveReferencePose)
            {
                if (settings.additiveReferencePoseClip == null) Drop("clip additive reference pose with no clip");
                else o.Add("additiveReferencePose", new Obj { { "clip", Clip(settings.additiveReferencePoseClip) }, { "time", settings.additiveReferencePoseTime } });
            }
            if (clip.humanMotion) o.Add("humanoid", HumanoidClip(settings));
            o.Add("curves", curves);
            o.Add("slotCurves", slots);
            return o;
        }

        static Obj HumanoidClip(AnimationClipSettings s) => new Obj
        {
            { "mirror", s.mirror }, { "loopPose", s.loopBlend },
            { "rotation", new Obj { { "bakeIntoPose", s.loopBlendOrientation }, { "basedUpon", s.keepOriginalOrientation ? "original" : "bodyOrientation" }, { "offsetDegrees", s.orientationOffsetY } } },
            { "height", new Obj { { "bakeIntoPose", s.loopBlendPositionY }, { "basedUpon", s.keepOriginalPositionY ? "original" : s.heightFromFeet ? "feet" : "centerOfMass" }, { "offset", s.level } } },
            { "xz", new Obj { { "bakeIntoPose", s.loopBlendPositionXZ }, { "basedUpon", s.keepOriginalPositionXZ ? "original" : "centerOfMass" } } },
        };

        Transform Path(string path)
        {
            var t = string.IsNullOrEmpty(path) ? root : root.Find(path);
            return t != null && nodeIndex.ContainsKey(t) ? t : null;
        }

        static readonly string[] Axes = { "x", "y", "z", "w" };

        static bool Drawn(Type t) => typeof(SkinnedMeshRenderer).IsAssignableFrom(t) || typeof(MeshRenderer).IsAssignableFrom(t);

        (Obj, bool) Binding(EditorCurveBinding b)
        {
            var t = Path(b.path);
            if (t == null) { Drop("animation curve on a missing object"); return (null, false); }
            int node = nodeIndex[t];
            string prop = b.propertyName;
            string Component(string p) { int dot = p.LastIndexOf('.'); return dot < 0 ? null : p.Substring(dot + 1); }
            if (b.type == typeof(GameObject) && prop == "m_IsActive") return (new Obj { { "type", "nodeActive" }, { "node", node } }, false);
            if (b.type == typeof(Transform))
            {
                string kind = prop.StartsWith("m_LocalPosition.") ? "translation" : prop.StartsWith("m_LocalRotation.") ? "rotation"
                    : prop.StartsWith("localEulerAngles") || prop.StartsWith("m_LocalEulerAngles") ? "eulerRotation" : prop.StartsWith("m_LocalScale.") ? "scale" : null;
                var axis = Component(prop);
                if (kind != null && Array.IndexOf(Axes, axis) >= 0)
                {
                    bool negate = kind == "translation" ? axis == "x" : (kind == "rotation" || kind == "eulerRotation") && (axis == "y" || axis == "z");
                    return (new Obj { { "type", "transform" }, { "node", node }, { "property", kind }, { "component", axis } }, negate);
                }
            }
            if (Drawn(b.type) && t.GetComponent(b.type) != null)
            {
                if (prop == "m_Enabled") return (new Obj { { "type", "rendererEnabled" }, { "node", node } }, false);
                if (prop.StartsWith("blendShape.") && b.type == typeof(SkinnedMeshRenderer)) return (new Obj { { "type", "blendShape" }, { "node", node }, { "name", prop.Substring("blendShape.".Length) } }, false);
                if (prop.StartsWith("material.")) return (new Obj { { "type", "materialProperty" }, { "node", node }, { "property", prop.Substring("material.".Length) } }, false);
            }
            if (b.type == typeof(Animator) && t == root)
            {
                if (animatorParameters.Contains(prop)) return (new Obj { { "type", "animatorParameter" }, { "name", prop } }, false);
                return (new Obj { { "type", "muscle" }, { "name", prop } }, false);
            }
            var component = typeof(Component).IsAssignableFrom(b.type) ? t.GetComponent(b.type) : null;
            if (component is Component c)
            {
                bool negate = Exporter.MirroredField(prop);
                if (physboneIndex.TryGetValue(c, out int pi)) return (new Obj { { "type", "physBone" }, { "index", pi }, { "field", prop } }, negate);
                if (constraintIndex.TryGetValue(c, out int ci)) return (new Obj { { "type", "constraint" }, { "index", ci }, { "field", prop } }, negate);
                if (senderIndex.TryGetValue(c, out int si)) return (new Obj { { "type", "contactSender" }, { "index", si }, { "field", prop } }, negate);
                if (receiverIndex.TryGetValue(c, out int ri)) return (new Obj { { "type", "contactReceiver" }, { "index", ri }, { "field", prop } }, negate);
            }
            Drop(component == null && b.type != typeof(GameObject) ? "animation curve on a missing component" : "animated " + b.type.Name + "." + prop.Split('.')[0]);
            return (null, false);
        }
    }
}
