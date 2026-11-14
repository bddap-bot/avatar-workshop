from __future__ import annotations

import sys
from types import TracebackType


def names() -> bool:
    """Whether a tool may print names: an avatar outside this repository, and its recordings, hold names that are
    not this repository's, so without `--names` an uncaught exception reports only where it happened."""
    if "--names" in sys.argv:
        sys.argv[:] = [a for a in sys.argv if a != "--names"]
        return True

    def hook(kind: type[BaseException], value: BaseException, tb: TracebackType | None) -> None:
        while tb is not None and tb.tb_next is not None:
            tb = tb.tb_next
        at = "" if tb is None else f" at {tb.tb_frame.f_code.co_filename}:{tb.tb_lineno}"
        print(f"failed{at}: pass --names for the message", file=sys.stderr)

    sys.excepthook = hook
    return False
