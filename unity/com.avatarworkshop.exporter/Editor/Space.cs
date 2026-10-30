using UnityEngine;

namespace AvatarWorkshop.Exporter
{
    // Unity is left-handed and glTF right-handed: mirroring X maps one onto the other, so positions and
    // directions negate X, and rotations (quaternion or Euler) negate Y and Z.
    public static class Space
    {
        public static float[] P(Vector3 v) => new[] { -v.x, v.y, v.z };
        public static float[] Q(Quaternion q) => new[] { q.x, -q.y, -q.z, q.w };
        public static float[] E(Vector3 euler) => new[] { euler.x, -euler.y, -euler.z };
        public static float[] S(Vector3 v) => new[] { v.x, v.y, v.z };

        static readonly Matrix4x4 Mirror = Matrix4x4.Scale(new Vector3(-1, 1, 1));

        public static Matrix4x4 MirrorMatrix(Matrix4x4 m) => Mirror * m * Mirror;

        public static float[] M(Matrix4x4 m)
        {
            m = MirrorMatrix(m);
            var a = new float[16];
            for (int c = 0; c < 4; c++)
                for (int r = 0; r < 4; r++)
                    a[c * 4 + r] = m[r, c];
            return a;
        }
    }
}
