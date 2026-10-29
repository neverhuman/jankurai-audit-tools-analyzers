#!/usr/bin/env python3
"""Render a release section from an explicit Git commit range to stdout."""
import argparse
import datetime
import re
import subprocess
from pathlib import Path


def git(*args):
    return subprocess.check_output(["git", *args], text=True).strip()


def commit(ref):
    return git("rev-parse", "--verify", "--end-of-options", ref + "^{commit}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--since", required=True, help="previous immutable release tag")
    parser.add_argument("--until", required=True, help="release candidate commit")
    parser.add_argument("--version", required=True)
    parser.add_argument("--date", required=True, help="YYYY-MM-DD")
    parser.add_argument("--changelog", type=Path, help="render updated changelog instead of section")
    args = parser.parse_args()
    try:
        if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+(?:[-+][A-Za-z0-9.+-]+)?", args.version):
            raise ValueError("invalid version")
        if datetime.date.fromisoformat(args.date).isoformat() != args.date:
            raise ValueError("date must be YYYY-MM-DD")
        start, end = commit(args.since), commit(args.until)
        subprocess.run(["git", "merge-base", "--is-ancestor", start, end], check=True)
        shas = git("rev-list", "--reverse", "--topo-order", start + ".." + end).splitlines()
        if not shas:
            raise ValueError("empty release range")
        lines = [f"## [{args.version}] - {args.date}", "", "### Changed", ""]
        for sha in shas:
            subject = git("show", "-s", "--format=%s", sha)
            trailers = git("show", "-s", "--format=%(trailers:key=Todo,valueonly,unfold)", sha)
            lines.append(f"- {subject} (`{sha}`)")
            for trailer in trailers.splitlines():
                lines.append(f"  Todo: {trailer}")
        section = "\n".join(lines) + "\n"
        if args.changelog:
            original = args.changelog.read_text()
            marker = "## [Unreleased]\n"
            if original.count(marker) != 1:
                raise ValueError("expected exactly one Unreleased heading")
            before, after = original.split(marker)
            if after.lstrip() and not after.lstrip().startswith("## ["):
                raise ValueError("Unreleased must be empty; notes come from commits")
            if f"## [{args.version}]" in original:
                raise ValueError("version already recorded")
            section = before + marker + "\n" + section + "\n" + after.lstrip("\n")
        print(section, end="")
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        parser.exit(1, f"release-notes: {error}\n")


if __name__ == "__main__":
    main()
