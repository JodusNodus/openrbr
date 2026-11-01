"""Loads the development environment into os.environ as the shell scripts get it: runs scripts/dev-env.sh (dev.env and
the folders' defaults) once and adds what it exports. Imported by replay.py, so by every script that imports it."""
import os
import subprocess

_SCRIPT = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "scripts", "dev-env.sh")


def load():
    out = subprocess.run(["bash", "-c", 'source "$1" && env -0', "dev-env", _SCRIPT], check=True,
                         capture_output=True).stdout
    for entry in out.split(b"\0"):
        key, sep, value = entry.decode(errors="surrogateescape").partition("=")
        if sep and key not in os.environ:
            os.environ[key] = value


load()
