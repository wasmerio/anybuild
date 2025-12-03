# File forked from https://github.com/nickstenning/honcho/blob/main/honcho/environ.py
# MIT License
import re

PROCFILE_LINE = re.compile(r"^([A-Za-z0-9_-]+):\s*(.+)$")


class Procfile(object):
    """A data structure representing a Procfile"""

    def __init__(self):
        self.processes = {}

    @classmethod
    def loads(cls, contents):
        p = cls()
        for line in contents.splitlines():
            m = PROCFILE_LINE.match(line)
            if m:
                p.add_process(m.group(1), m.group(2))
        return p

    def add_process(self, name, command):
        assert name not in self.processes, (
            "process names must be unique within a Procfile"
        )
        self.processes[name] = command

    def get_start_command(self):
        if "web" in self.processes:
            return self.processes["web"]
        elif "default" in self.processes:
            return self.processes["default"]
        elif "start" in self.processes:
            return self.processes["start"]
        elif len(self.processes) == 1:
            return list(self.processes.values())[0]
        return None
