using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Text.RegularExpressions;
using UnityEditor;
using UnityEditor.Animations;
using UnityEngine;
using VRC.SDK3.Avatars.Components;
using VRC.SDK3.Avatars.ScriptableObjects;

namespace AvatarWorkshop.Reference
{
    // The write-defaults fixture: the fixture's body under controllers built from AW_WD, so which layer and state
    // writes defaults, and what each animates, is the recording's one variable. It has no menu: AW_STEPS plays it.
    //   AW_WD  layers, ';' between: Name[@slot][*weight][/seconds]:state|state...
    //          @slot: the controller it joins, FX unless B (Base), A (Additive), G (Gesture) or X (Action); a slot
    //          no layer names keeps the SDK's default. *weight: the layer's, which Unity ignores on a controller's
    //          first layer. /seconds: how long its transitions take.
    //          state: d (writes defaults) or f (does not), then - (no motion), e (an empty clip) or Prop=v+Prop=v
    //          Prop: Hat (the object's active flag), HatR (its renderer), Color (Body's _Color.r), else a blendshape
    //          two states switch on the bool Name, more on the int Name; one state has no parameter
    public static partial class Fixture
    {
        static readonly Dictionary<string, VRCAvatarDescriptor.AnimLayerType> Slots = new Dictionary<string, VRCAvatarDescriptor.AnimLayerType>
        {
            ["B"] = VRCAvatarDescriptor.AnimLayerType.Base, ["A"] = VRCAvatarDescriptor.AnimLayerType.Additive, ["G"] = VRCAvatarDescriptor.AnimLayerType.Gesture,
            ["X"] = VRCAvatarDescriptor.AnimLayerType.Action, ["FX"] = VRCAvatarDescriptor.AnimLayerType.FX,
        };

        static void WriteDefaults(VRCAvatarDescriptor d)
        {
            var spec = Environment.GetEnvironmentVariable("AW_WD");
            if (string.IsNullOrEmpty(spec)) throw new Exception("AW_WD is required");
            if (string.IsNullOrEmpty(Environment.GetEnvironmentVariable("AW_STEPS"))) throw new Exception("AW_STEPS is required: the write-defaults fixture has no menu to walk");
            AssetDatabase.CreateFolder(Dir, "Clips");
            var k = new Clips(Dir + "/Clips");
            var empty = k.Make("Empty", c => { });
            var controllers = new Dictionary<string, Ctl>();
            var parameters = new List<(string name, bool isInt)>();
            var names = new HashSet<string>();
            int clip = 0;
            foreach (var layerSpec in spec.Split(';'))
            {
                var m = Regex.Match(layerSpec.Trim(), @"^(\w+)((?:@\w+|\*[\d.]+|/[\d.]+)*):(.+)$");
                if (!m.Success) throw new Exception("bad AW_WD layer: " + layerSpec);
                var name = m.Groups[1].Value;
                if (!names.Add(name)) throw new Exception("AW_WD names two layers " + name);
                var options = m.Groups[2].Value;
                var where = Regex.Match(options, @"@(\w+)") is var w && w.Success ? w.Groups[1].Value : "FX";
                if (!Slots.ContainsKey(where)) throw new Exception($"AW_WD layer {name} names the slot {where}");
                float Option(string mark, float fallback) => Regex.Match(options, Regex.Escape(mark) + @"([\d.]+)") is var o && o.Success ? float.Parse(o.Groups[1].Value, CultureInfo.InvariantCulture) : fallback;
                var states = m.Groups[3].Value.Split('|');
                if (!controllers.TryGetValue(where, out var ctl)) controllers[where] = ctl = new Ctl("WD" + where);
                if (states.Length > 1)
                {
                    parameters.Add((name, states.Length > 2));
                    ctl.C.AddParameter(name, states.Length > 2 ? AnimatorControllerParameterType.Int : AnimatorControllerParameterType.Bool);
                }
                var layer = ctl.Layer(name);
                var weight = Option("*", 1);
                if (weight != 1)
                {
                    var layers = ctl.C.layers;
                    layers[layers.Length - 1].defaultWeight = weight;
                    ctl.C.layers = layers;
                }
                var made = new List<AnimatorState>();
                for (int i = 0; i < states.Length; i++)
                {
                    var s = layer.stateMachine.AddState("S" + i);
                    var parsed = Regex.Match(states[i].Trim(), @"^([df])(-|e|\w+=[-\d.]+(?:\+\w+=[-\d.]+)*)$");
                    if (!parsed.Success) throw new Exception($"bad AW_WD state in layer {name}: {states[i]}");
                    s.writeDefaultValues = parsed.Groups[1].Value == "d";
                    var motion = parsed.Groups[2].Value;
                    s.motion = motion == "-" ? null : motion == "e" ? empty : k.Make($"C{clip++}_{name}_{i}", c =>
                    {
                        foreach (var set in motion.Split('+'))
                        {
                            var kv = set.Split('=');
                            var v = float.Parse(kv[1], CultureInfo.InvariantCulture);
                            switch (kv[0])
                            {
                                case "Hat": Const(c, "Hat", typeof(GameObject), "m_IsActive", v); break;
                                case "HatR": Const(c, "Hat", typeof(SkinnedMeshRenderer), "m_Enabled", v); break;
                                case "Color": Const(c, "Body", typeof(SkinnedMeshRenderer), "material._Color.r", v); break;
                                default:
                                    if (!Faces.Contains(kv[0])) throw new Exception($"AW_WD layer {name} animates {kv[0]}, which Body has no blendshape for");
                                    Shape(c, kv[0], v); break;
                            }
                        }
                    });
                    made.Add(s);
                }
                layer.stateMachine.defaultState = made[0];
                var duration = Option("/", 0);
                if (made.Count == 2)
                {
                    Go(made[0], made[1], duration).AddCondition(AnimatorConditionMode.If, 0, name);
                    Go(made[1], made[0], duration).AddCondition(AnimatorConditionMode.IfNot, 0, name);
                }
                else if (made.Count > 2)
                    for (int i = 0; i < made.Count; i++)
                    {
                        var tr = layer.stateMachine.AddAnyStateTransition(made[i]);
                        tr.hasExitTime = false; tr.duration = duration; tr.hasFixedDuration = true; tr.canTransitionToSelf = false;
                        tr.AddCondition(AnimatorConditionMode.Equals, i, name);
                    }
            }

            var expression = ScriptableObject.CreateInstance<VRCExpressionParameters>();
            expression.parameters = parameters.Select(p => P(p.name, p.isInt ? VRCExpressionParameters.ValueType.Int : VRCExpressionParameters.ValueType.Bool, 0, saved: false)).ToArray();
            AssetDatabase.CreateAsset(expression, $"{Dir}/Parameters.asset");
            d.customExpressions = true;
            d.expressionParameters = expression;
            d.customizeAnimationLayers = true;
            d.baseAnimationLayers = Slots.Select(slot => controllers.TryGetValue(slot.Key, out var c) ? Custom(slot.Value, c.C)
                : new VRCAvatarDescriptor.CustomAnimLayer { type = slot.Value, isDefault = true }).ToArray();
            d.specialAnimationLayers = new[]
            {
                new VRCAvatarDescriptor.CustomAnimLayer { type = VRCAvatarDescriptor.AnimLayerType.Sitting, isDefault = true },
                new VRCAvatarDescriptor.CustomAnimLayer { type = VRCAvatarDescriptor.AnimLayerType.TPose, isDefault = true },
                new VRCAvatarDescriptor.CustomAnimLayer { type = VRCAvatarDescriptor.AnimLayerType.IKPose, isDefault = true },
            };
        }
    }
}
