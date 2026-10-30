using System.Collections.Generic;
using System.Linq;
using UnityEditor.Animations;
using UnityEngine;
using VRC.Dynamics;
using VRC.SDK3.Avatars.Components;
using VRC.SDK3.Avatars.ScriptableObjects;
using VRC.SDK3.Dynamics.Constraint.Components;
using VRC.SDKBase;

namespace AvatarWorkshop.Reference
{
    public static class Coverage
    {
        public static readonly string[] Required =
        {
            "physbone.immobile.all_motion", "physbone.immobile.world",
            "physbone.limit.none", "physbone.limit.angle", "physbone.limit.hinge", "physbone.limit.polar",
            "physbone.gravity_falloff", "physbone.stretch", "physbone.squish", "physbone.curve",
            "physbone.version.1_0", "physbone.version.1_1", "physbone.integration.simplified", "physbone.integration.advanced",
            "physbone.multichild.ignore", "physbone.multichild.first", "physbone.multichild.average", "physbone.endpoint",
            "collider.sphere", "collider.capsule", "collider.plane", "collider.inside",
            "contact.sender", "contact.receiver",
            "constraint.parent", "constraint.position", "constraint.rotation", "constraint.scale", "constraint.aim", "constraint.look_at", "constraint.freeze_to_world",
            "menu.button", "menu.toggle", "menu.submenu", "menu.two_axis", "menu.four_axis", "menu.radial",
            "transition.fixed_duration", "transition.normalized_duration",
            "blendtree.simple_1d", "blendtree.simple_directional_2d", "blendtree.freeform_directional_2d", "blendtree.freeform_cartesian_2d", "blendtree.direct",
            "driver.set", "driver.add", "driver.random", "driver.copy", "driver.local_only",
            "write_defaults.on", "write_defaults.off",
        };

        public static SortedDictionary<string, int> Count(GameObject root)
        {
            var c = new SortedDictionary<string, int>();
            foreach (var k in Required) c[k] = 0;
            void Add(string k) => c[k] = c.TryGetValue(k, out var n) ? n + 1 : 1;

            foreach (var pb in root.GetComponentsInChildren<VRCPhysBoneBase>(true))
            {
                if (pb.immobile > 0) Add(pb.immobileType == VRCPhysBoneBase.ImmobileType.World ? "physbone.immobile.world" : "physbone.immobile.all_motion");
                Add("physbone.limit." + pb.limitType.ToString().ToLowerInvariant());
                if (pb.gravity != 0 && pb.gravityFalloff > 0) Add("physbone.gravity_falloff");
                if (pb.maxStretch > 0) Add("physbone.stretch");
                if (pb.maxSquish > 0) Add("physbone.squish");
                var curves = new[] { pb.pullCurve, pb.springCurve, pb.stiffnessCurve, pb.gravityCurve, pb.gravityFalloffCurve, pb.immobileCurve, pb.radiusCurve,
                    pb.maxAngleXCurve, pb.maxAngleZCurve, pb.limitRotationXCurve, pb.limitRotationYCurve, pb.limitRotationZCurve, pb.maxStretchCurve, pb.maxSquishCurve, pb.stretchMotionCurve };
                if (curves.Any(k => k != null && k.length > 0)) Add("physbone.curve");
                Add(pb.version == VRCPhysBoneBase.Version.Version_1_0 ? "physbone.version.1_0" : "physbone.version.1_1");
                Add("physbone.integration." + pb.integrationType.ToString().ToLowerInvariant());
                var start = pb.rootTransform != null ? pb.rootTransform : pb.transform;
                if (start.childCount > 1) Add("physbone.multichild." + pb.multiChildType.ToString().ToLowerInvariant());
                if (pb.endpointPosition != Vector3.zero) Add("physbone.endpoint");
            }
            foreach (var col in root.GetComponentsInChildren<VRCPhysBoneColliderBase>(true))
            {
                Add("collider." + col.shapeType.ToString().ToLowerInvariant());
                if (col.insideBounds) Add("collider.inside");
            }
            foreach (var s in root.GetComponentsInChildren<ContactSender>(true)) Add("contact.sender");
            foreach (var r in root.GetComponentsInChildren<ContactReceiver>(true)) { Add("contact.receiver"); Add("contact.receiver." + r.receiverType.ToString().ToLowerInvariant()); }
            foreach (var k in root.GetComponentsInChildren<VRCConstraintBase>(true))
            {
                switch (k)
                {
                    case VRCParentConstraint _: Add("constraint.parent"); break;
                    case VRCPositionConstraint _: Add("constraint.position"); break;
                    case VRCRotationConstraint _: Add("constraint.rotation"); break;
                    case VRCScaleConstraint _: Add("constraint.scale"); break;
                    case VRCAimConstraint _: Add("constraint.aim"); break;
                    case VRCLookAtConstraint _: Add("constraint.look_at"); break;
                }
                if (k.FreezeToWorld) Add("constraint.freeze_to_world");
            }

            var d = root.GetComponent<VRCAvatarDescriptor>();
            if (d == null) return c;
            var seen = new HashSet<VRCExpressionsMenu>();
            void Menu(VRCExpressionsMenu m)
            {
                if (m == null || !seen.Add(m)) return;
                foreach (var ctl in m.controls)
                {
                    Add("menu." + Snake(ctl.type.ToString().Replace("Puppet", "")).Replace("sub_menu", "submenu"));
                    if (ctl.type == VRCExpressionsMenu.Control.ControlType.SubMenu) Menu(ctl.subMenu);
                }
            }
            Menu(d.expressionsMenu);

            var controllers = d.baseAnimationLayers.Concat(d.specialAnimationLayers ?? new VRCAvatarDescriptor.CustomAnimLayer[0])
                .Where(l => !l.isDefault).Select(l => l.animatorController as AnimatorController).Where(x => x != null).Distinct();
            foreach (var ctrl in controllers)
                foreach (var layer in ctrl.layers)
                    Machine(layer.stateMachine, Add);
            return c;
        }

        static void Machine(AnimatorStateMachine sm, System.Action<string> add)
        {
            foreach (var t in sm.anyStateTransitions.Cast<AnimatorStateTransition>().Concat(sm.states.SelectMany(s => s.state.transitions)))
                if (t.duration > 0) add(t.hasFixedDuration ? "transition.fixed_duration" : "transition.normalized_duration");
            foreach (var cs in sm.states)
            {
                var s = cs.state;
                add(s.writeDefaultValues ? "write_defaults.on" : "write_defaults.off");
                if (s.motion is BlendTree bt) Tree(bt, add);
                foreach (var b in s.behaviours.OfType<VRC_AvatarParameterDriver>())
                {
                    if (b.localOnly) add("driver.local_only");
                    foreach (var p in b.parameters) add("driver." + p.type.ToString().ToLowerInvariant());
                }
            }
            foreach (var child in sm.stateMachines) Machine(child.stateMachine, add);
        }

        static void Tree(BlendTree bt, System.Action<string> add)
        {
            add("blendtree." + Snake(bt.blendType.ToString()).Replace("2_d", "2d").Replace("1_d", "1d"));
            foreach (var ch in bt.children) if (ch.motion is BlendTree sub) Tree(sub, add);
        }

        static string Snake(string s) => string.Concat(s.Select((ch, i) => i > 0 && (char.IsUpper(ch) || char.IsDigit(ch) && !char.IsDigit(s[i - 1])) ? "_" + char.ToLowerInvariant(ch) : char.ToLowerInvariant(ch).ToString()));
    }
}
