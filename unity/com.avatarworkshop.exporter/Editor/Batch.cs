using System;
using System.Linq;
using UnityEditor;
using UnityEditor.SceneManagement;
using UnityEngine;
using VRC.SDK3.Avatars.Components;

namespace AvatarWorkshop.Exporter
{
    // Unity -batchmode -projectPath <project> -executeMethod AvatarWorkshop.Exporter.Batch.Run -awOut <file.glb>
    //   [-awScene <scene path>] [-awAvatar <avatar object name>]
    // Without -awAvatar the scene must hold exactly one active avatar.
    public static class Batch
    {
        public static void Run()
        {
            int code = 1;
            try
            {
                var args = Environment.GetCommandLineArgs();
                string Arg(string name)
                {
                    int i = Array.IndexOf(args, name);
                    return i >= 0 && i + 1 < args.Length ? args[i + 1] : null;
                }
                var output = Arg("-awOut") ?? throw new ArgumentException("-awOut <file.glb> is required");
                if (Arg("-awScene") is string scene) EditorSceneManager.OpenScene(scene, OpenSceneMode.Single);
                var avatar = Find(Arg("-awAvatar"));
                var report = Exporter.Export(avatar, output);
                Debug.Log($"[aw] exported {avatar.name} to {report.Path}: {report.Nodes} nodes" + (report.MigratedDynamicBones ? ", DynamicBones migrated" : ""));
                foreach (var kv in report.Unsupported) Debug.Log($"[aw] unsupported {kv.Value} × {kv.Key}");
                code = 0;
            }
            catch (Exception e)
            {
                Debug.LogError("[aw] FAILED " + e);
            }
            EditorApplication.Exit(code);
        }

        [MenuItem("Tools/Avatar Workshop/Export Avatar")]
        static void ExportSelected()
        {
            var avatar = Selection.activeGameObject;
            if (avatar == null || avatar.GetComponent<VRCAvatarDescriptor>() == null)
            {
                EditorUtility.DisplayDialog("Export Avatar", "Select an avatar's root object (the one with the VRC Avatar Descriptor).", "OK");
                return;
            }
            var path = EditorUtility.SaveFilePanel("Export Avatar", "", avatar.name + ".glb", "glb");
            if (string.IsNullOrEmpty(path)) return;
            var report = Exporter.Export(avatar, path);
            var lines = report.Unsupported.Select(kv => $"{kv.Value} × {kv.Key}");
            EditorUtility.DisplayDialog("Export Avatar", $"Wrote {path}.\n\nNot represented:\n" + (report.Unsupported.Count == 0 ? "nothing" : string.Join("\n", lines)), "OK");
        }

        static GameObject Find(string name)
        {
            var all = Resources.FindObjectsOfTypeAll<VRCAvatarDescriptor>()
                .Where(d => !EditorUtility.IsPersistent(d) && d.gameObject.scene.IsValid()).ToList();
            var chosen = name != null
                ? all.Where(d => d.gameObject.name == name).ToList()
                : all.Where(d => d.gameObject.activeInHierarchy).ToList();
            if (chosen.Count != 1)
                throw new Exception($"{chosen.Count} avatars match {(name ?? "the active ones")}; scene avatars: {string.Join(", ", all.Select(d => d.gameObject.name))}");
            return chosen[0].gameObject;
        }
    }
}
