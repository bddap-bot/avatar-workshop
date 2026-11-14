import itertools
import math
import sys
from typing import Iterator

sys.dont_write_bytecode = True
from names import names


def frames(path: str) -> Iterator[tuple[int, dict[tuple[str, str], list[float]]]]:
    with open(path) as f:
        rows = (line.rstrip("\n").split("\t") for line in f)
        for frame, group in itertools.groupby(rows, key=lambda r: int(r[0])):
            yield frame, {(r[1], r[2]): [float(v) for v in r[3].split(",")] for r in group}


def main() -> None:
    named = names()
    a, b = frames(sys.argv[1]), frames(sys.argv[2])
    sa: dict[tuple[str, str], list[float]] = {}
    sb: dict[tuple[str, str], list[float]] = {}
    na, nb = next(a, None), next(b, None)
    worst, changed = 0.0, 0
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
            sys.exit(f"driver poses differ in transforms at frame {frame}")
        for k in sa:
            if len(sa[k]) != len(sb[k]):
                sys.exit(f"driver pose {k if named else sorted(sa).index(k)} has {len(sa[k])} and {len(sb[k])} values at frame {frame}")
            for x, y in zip(sa[k], sb[k]):
                d = abs(x - y)
                if not d <= worst:
                    worst = d if d == d else math.inf
    print(f"driver poses ({sys.argv[3]}): {len(sa)} transforms over {changed} changed frames, max difference {worst:.3g}")
    sys.exit(worst > 1e-5)


main()
