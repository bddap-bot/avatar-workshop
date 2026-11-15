using System;
using System.Reflection;
using System.Threading;
using HarmonyLib;
using UnityEngine;

namespace AvatarWorkshop.Reference
{
    // Build tools name, number, place and identify what they generate from UnityEngine.Random, System.Random and
    // Guid.NewGuid, and a particle system seeds itself from the clock: unseeded, two recordings of one avatar differ.
    // So do jobs run on worker threads: the dynamics' first frames came out in the order the threads happened to run.
    static class Repeatable
    {
        public const int Seed = 1;
        static readonly Harmony harmony = new Harmony("avatar-workshop.reference.repeatable");
        static System.Random guidBytes;
        static int mainThread;

        public static void Begin()
        {
            Unity.Jobs.LowLevel.Unsafe.JobsUtility.JobWorkerCount = 0;
            UnityEngine.Random.InitState(Seed);
            var threadRandom = typeof(System.Random).GetField("t_threadRandom", BindingFlags.NonPublic | BindingFlags.Static)
                ?? throw new Exception("System.Random has no t_threadRandom to seed new Random() from");
            threadRandom.SetValue(null, new System.Random(Seed));
            mainThread = Thread.CurrentThread.ManagedThreadId;
            guidBytes = new System.Random(Seed);
            harmony.Patch(AccessTools.Method(typeof(Guid), nameof(Guid.NewGuid)), prefix: new HarmonyMethod(typeof(Repeatable), nameof(NewGuid)));
        }

        // Another thread's draws would interleave with the build's by timing.
        static bool NewGuid(ref Guid __result)
        {
            if (Thread.CurrentThread.ManagedThreadId != mainThread) return true;
            var b = new byte[16];
            guidBytes.NextBytes(b);
            b[7] = (byte)((b[7] & 0x0F) | 0x40);
            b[8] = (byte)((b[8] & 0x3F) | 0x80);
            __result = new Guid(b);
            return false;
        }

        public static void SeedParticles(GameObject root)
        {
            var systems = root.GetComponentsInChildren<ParticleSystem>(true);
            for (int i = 0; i < systems.Length; i++)
            {
                systems[i].useAutoRandomSeed = false;
                systems[i].randomSeed = (uint)(Seed + i);
            }
        }
    }
}
