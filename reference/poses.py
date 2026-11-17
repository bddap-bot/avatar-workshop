import itertools
import math
import sys
from typing import Iterator

sys.dont_write_bytecode = True
from names import names

Pose = list[float]


def frames(path: str) -> Iterator[tuple[int, dict[tuple[str, str], Pose]]]:
    with open(path) as f:
        rows = (line.rstrip("\n").split("\t") for line in f)
        for frame, group in itertools.groupby(rows, key=lambda r: int(r[0])):
            yield frame, {(r[1], r[2]): [float(v) for v in r[3].split(",")] for r in group}


def degrees(a: Pose, b: Pose) -> float:
    """The angle between the rotations at a[3:7] and b[3:7], unit quaternions (x, y, z, w)."""
    ax, ay, az, aw = a[3:7]
    bx, by, bz, bw = b[3:7]
    w = aw * bw + ax * bx + ay * by + az * bz
    x = aw * bx - bw * ax - (ay * bz - az * by)
    y = aw * by - bw * ay - (az * bx - ax * bz)
    z = aw * bz - bw * az - (ax * by - ay * bx)
    return math.degrees(2 * math.atan2(math.sqrt(x * x + y * y + z * z), abs(w)))


def main() -> None:
    named = names()
    constrained = "--constrained" in sys.argv
    if constrained:
        sys.argv.remove("--constrained")
    what = "constrained poses" if constrained else "driver poses"
    a, b = frames(sys.argv[1]), frames(sys.argv[2])
    sa: dict[tuple[str, str], Pose] = {}
    sb: dict[tuple[str, str], Pose] = {}
    na, nb = next(a, None), next(b, None)
    worst, turn, changed = 0.0, 0.0, 0
    while na or nb:
        frame = min(x[0] for x in (na, nb) if x)
        if na and na[0] == frame:
            sa.update(na[1])
            na = next(a, None)
        if nb and nb[0] == frame:
            sb.update(nb[1])
            nb = next(b, None)
        changed += 1
        if sa.keys() != sb.keys():
            sys.exit(f"{what} differ in transforms at frame {frame}")
        for k in sa:
            if len(sa[k]) != len(sb[k]):
                sys.exit(f"{what[:-1]} {k if named else sorted(sa).index(k)} has {len(sa[k])} and {len(sb[k])} values at frame {frame}")
            values = zip(sa[k][:3] + sa[k][7:], sb[k][:3] + sb[k][7:]) if constrained else zip(sa[k], sb[k])
            for x, y in values:
                d = abs(x - y)
                if not d <= worst:
                    worst = d if d == d else math.inf
            if constrained:
                d = degrees(sa[k], sb[k])
                if not d <= turn:
                    turn = d if d == d else math.inf
    if constrained:
        print(f"{what} ({sys.argv[3]}): {len(sa)} poses over {changed} changed frames, max position and scale difference {worst:.3g}, max rotation difference {turn:.3g} degrees")
        sys.exit(worst > 1e-6 or turn > 1e-4)
    print(f"{what} ({sys.argv[3]}): {len(sa)} transforms over {changed} changed frames, max difference {worst:.3g}")
    sys.exit(worst > 1e-5)


main()
