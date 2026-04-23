import sys
from rich.console import Console

console = Console(stderr=True)


def write_stdout(line: str) -> None:
    sys.stdout.write(line)
    sys.stdout.flush()


def write_stderr(line: str) -> None:
    sys.stderr.write(line)
    sys.stderr.flush()
