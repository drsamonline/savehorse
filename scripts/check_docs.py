#!/usr/bin/env python3
"""Check that every `savehorse ...` command in the documentation parses.

The README and the user guide are full of example commands, and until this
script existed nothing verified them. They had drifted: `pulse priority
--commit` was rejected by the CLI for wanting a value, because `commit` was not
a declared parameter on that operation and was not in the global switch list
either.

This reimplements the CLI's argument splitter and checks each documented
command against the manifest, which is the same source the CLI reads. It runs
in CI, so a command that does not parse cannot be merged.

Templates (`savehorse <module> <op> ...`) and the top-level subcommands are
skipped by design: they are not operations.
"""

import json
import os
import re
import shlex
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Mirrors GLOBAL_SWITCHES in apps/savehorse-cli/src/main.rs.
GLOBAL_SWITCHES = {"json", "dry-run", "commit", "help", "version"}

# Handled before module dispatch, so they have no operation behind them.
TOP_LEVEL = {"modules", "ops", "--version", "--help", "help", "-h"}


def load_manifest():
    out = subprocess.run(
        [sys.executable, os.path.join(ROOT, "scripts", "export_manifest.py"), "/dev/stdout"],
        capture_output=True,
        cwd=ROOT,
    )
    if out.returncode != 0:
        # export_manifest writes a file, so fall back to a temp one.
        import tempfile

        with tempfile.NamedTemporaryFile(suffix=".json", delete=False) as tmp:
            path = tmp.name
        subprocess.run(
            [sys.executable, os.path.join(ROOT, "scripts", "export_manifest.py"), path],
            check=True,
            cwd=ROOT,
        )
        with open(path) as handle:
            return json.load(handle)

    return json.loads(out.stdout)


def parse_arguments(args, declared):
    """A port of `parse_arguments` in apps/savehorse-cli/src/main.rs."""
    switches = set(GLOBAL_SWITCHES) | {
        p["name"] for p in declared if p["kind"] == "Bool"
    }
    positional, flags, i = [], {}, 0

    while i < len(args):
        arg = args[i]
        if arg.startswith("--"):
            body = arg[2:]
            if "=" in body:
                key, value = body.split("=", 1)
                flags[key] = value
                i += 1
                continue
            if body in switches:
                flags[body] = "true"
                i += 1
                continue
            if i + 1 < len(args) and not args[i + 1].startswith("--"):
                flags[body] = args[i + 1]
                i += 2
                continue
            return None, "--{} needs a value".format(body)
        positional.append(arg)
        i += 1

    return (positional, flags), None


def documented_commands(path):
    text = open(path).read()
    found = []
    for block in re.findall(r"```[a-z]*\n(.*?)```", text, re.S):
        for line in block.splitlines():
            line = line.strip()
            if line.startswith("savehorse "):
                found.append(line)
    for line in re.findall(r"`(savehorse [^`]+)`", text):
        found.append(line.strip())
    return found


def is_template(command):
    return "<module>" in command or "<operation>" in command


def main():
    modules = load_manifest()
    ops = {(m["name"], o["name"]): o for m in modules for o in m["ops"]}

    problems = []
    checked = 0

    for doc in ("README.md", "USER_GUIDE.md"):
        path = os.path.join(ROOT, doc)
        if not os.path.exists(path):
            continue
        for command in documented_commands(path):
            if is_template(command):
                continue
            try:
                argv = shlex.split(command)
            except ValueError as exc:
                problems.append((doc, command, "unbalanced quoting: {}".format(exc)))
                continue
            if len(argv) < 2:
                continue
            if argv[1] in TOP_LEVEL:
                continue
            # `<module> --help` is module help, handled before dispatch.
            if len(argv) >= 3 and argv[2] in ("--help", "-h"):
                continue
            if len(argv) < 3:
                problems.append((doc, command, "no operation given"))
                continue

            key = (argv[1], argv[2])
            op = ops.get(key)
            if op is None:
                problems.append(
                    (doc, command, "no such operation: {}.{}".format(*key))
                )
                continue

            checked += 1
            parsed, error = parse_arguments(argv[3:], op["params"])
            if error:
                problems.append((doc, command, error))
                continue

            positional, flags = parsed
            wanted = [p["name"] for p in op["params"] if p["positional"]]
            if len(positional) != len(wanted):
                problems.append(
                    (
                        doc,
                        command,
                        "{} positional(s), operation wants {} ({})".format(
                            len(positional), len(wanted), ", ".join(wanted) or "none"
                        ),
                    )
                )
                continue

            declared = {p["name"] for p in op["params"]}
            unknown = sorted(f for f in flags if f not in declared | GLOBAL_SWITCHES)
            if unknown:
                problems.append(
                    (
                        doc,
                        command,
                        "unknown flag(s): {}".format(
                            ", ".join("--" + u for u in unknown)
                        ),
                    )
                )

    if problems:
        print("documented commands that do not parse:\n")
        for doc, command, why in problems:
            print("  {}\n    {}\n    -> {}\n".format(doc, command, why))
        print("{} problem(s)".format(len(problems)))
        return 1

    print("{} documented commands checked, all parse".format(checked))
    return 0


if __name__ == "__main__":
    sys.exit(main())
