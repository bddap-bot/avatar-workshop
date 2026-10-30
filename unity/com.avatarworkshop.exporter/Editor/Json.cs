using System;
using System.Collections;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Text;

namespace AvatarWorkshop.Exporter
{
    public sealed class Obj : IEnumerable<KeyValuePair<string, object>>
    {
        readonly List<KeyValuePair<string, object>> fields = new List<KeyValuePair<string, object>>();

        public void Add(string key, object value) => fields.Add(new KeyValuePair<string, object>(key, value));

        public object this[string key]
        {
            set
            {
                int i = fields.FindIndex(f => f.Key == key);
                if (i < 0) Add(key, value);
                else fields[i] = new KeyValuePair<string, object>(key, value);
            }
        }

        public IEnumerator<KeyValuePair<string, object>> GetEnumerator() => fields.GetEnumerator();
        IEnumerator IEnumerable.GetEnumerator() => fields.GetEnumerator();
    }

    public static class Json
    {
        public static string Write(object value)
        {
            var sb = new StringBuilder();
            Append(sb, value, "");
            return sb.ToString();
        }

        static void Append(StringBuilder sb, object value, string at)
        {
            switch (value)
            {
                case null: sb.Append("null"); break;
                case bool b: sb.Append(b ? "true" : "false"); break;
                case string s: AppendString(sb, s); break;
                case int i: sb.Append(i.ToString(CultureInfo.InvariantCulture)); break;
                case uint u: sb.Append(u.ToString(CultureInfo.InvariantCulture)); break;
                case long l: sb.Append(l.ToString(CultureInfo.InvariantCulture)); break;
                case float f: AppendNumber(sb, f, at); break;
                case double d: AppendNumber(sb, d, at); break;
                case Obj o:
                    sb.Append('{');
                    bool first = true;
                    foreach (var kv in o)
                    {
                        if (!first) sb.Append(',');
                        first = false;
                        AppendString(sb, kv.Key);
                        sb.Append(':');
                        Append(sb, kv.Value, at + "." + kv.Key);
                    }
                    sb.Append('}');
                    break;
                case IEnumerable list:
                    sb.Append('[');
                    int n = 0;
                    foreach (var item in list)
                    {
                        if (n > 0) sb.Append(',');
                        Append(sb, item, at + "[" + n + "]");
                        n++;
                    }
                    sb.Append(']');
                    break;
                default: throw new ArgumentException($"{at}: cannot write a {value.GetType().Name} as JSON");
            }
        }

        // JSON cannot carry a non-finite number: each becomes 0 (NaN) or the largest finite value of its sign,
        // and is reported with its path.
        public static void MakeFinite(object value, string at, Action<string> report)
        {
            object Fix(object v, string path)
            {
                if (v is float f && (float.IsNaN(f) || float.IsInfinity(f)))
                {
                    report(path);
                    return float.IsNaN(f) ? 0f : f > 0 ? float.MaxValue : float.MinValue;
                }
                MakeFinite(v, path, report);
                return v;
            }
            switch (value)
            {
                case Obj o:
                    foreach (var kv in o.ToList()) { var v = Fix(kv.Value, at + "." + kv.Key); if (!ReferenceEquals(v, kv.Value)) o[kv.Key] = v; }
                    break;
                case IList list when !(value is string):
                    for (int i = 0; i < list.Count; i++) { var v = Fix(list[i], at + "[" + i + "]"); if (!ReferenceEquals(v, list[i])) list[i] = v; }
                    break;
            }
        }

        static void AppendNumber(StringBuilder sb, double d, string at)
        {
            if (double.IsNaN(d) || double.IsInfinity(d)) throw new ArgumentException($"{at}: {d} is not a JSON number");
            sb.Append(d.ToString("R", CultureInfo.InvariantCulture));
        }

        static void AppendNumber(StringBuilder sb, float f, string at)
        {
            if (float.IsNaN(f) || float.IsInfinity(f)) throw new ArgumentException($"{at}: {f} is not a JSON number");
            sb.Append(f.ToString("R", CultureInfo.InvariantCulture));
        }

        static void AppendString(StringBuilder sb, string s)
        {
            sb.Append('"');
            foreach (char c in s)
            {
                switch (c)
                {
                    case '"': sb.Append("\\\""); break;
                    case '\\': sb.Append("\\\\"); break;
                    case '\n': sb.Append("\\n"); break;
                    case '\r': sb.Append("\\r"); break;
                    case '\t': sb.Append("\\t"); break;
                    default:
                        if (c < 0x20) sb.Append("\\u").Append(((int)c).ToString("x4"));
                        else sb.Append(c);
                        break;
                }
            }
            sb.Append('"');
        }
    }
}
