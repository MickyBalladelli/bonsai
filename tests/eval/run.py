#!/usr/bin/env python3
"""Bonsai real-task evaluation: answer-supporting retention, tokens, and time.

For each task this harness runs the CLI on a fixture (or real) repository and
reports deterministic quality proxies:

- important-file recall: fraction of hand-labeled task-relevant files present
  in the generated context, plus the fraction kept at full detail (level 1).
- tokens: raw full-source tokens vs shrunk emitted tokens, saving percent.
- follow-up estimate: rough full-source cost (chars/4) of important files
  missing from the context, i.e. what an agent would still have to read.
- completion time: wall-clock seconds for the CLI invocation.
- warnings: whether the output carries fidelity warnings.

Answer correctness itself needs a judge (LLM or human); see
docs/evaluation.md for the grading rubric. This script never gates: it exits
0 after printing the report unless the CLI itself fails.

Usage:
    BONSAI_BIN=target/release/bonsai python3 tests/eval/run.py
"""

import os
import subprocess
import sys
import tempfile
import time
import xml.etree.ElementTree as ET

REPO_ROOT = os.path.dirname(os.path.dirname(
    os.path.dirname(os.path.abspath(__file__))))


def find_binary():
    explicit = os.environ.get("BONSAI_BIN", "")
    if explicit:
        return explicit
    for candidate in ("target/debug/bonsai", "target/release/bonsai"):
        path = os.path.join(REPO_ROOT, candidate)
        if os.path.isfile(path) and os.access(path, os.X_OK):
            return path
    raise SystemExit(
        "no bonsai binary found: set BONSAI_BIN or build with cargo build"
    )


def write(path, contents):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(contents)


def fixture_auth(root):
    """Small repo: auth implementation plus dependency and lockfile noise."""
    write(os.path.join(root, "src/auth.ts"), (
        "export type Session = { user: string; expiresAt: number }\n"
        "\n"
        "export function createSession(user: string): Session {\n"
        "  return { user, expiresAt: Date.now() + 3600_000 }\n"
        "}\n"
        "\n"
        "export function isExpired(session: Session): boolean {\n"
        "  return Date.now() > session.expiresAt\n"
        "}\n"
    ))
    write(os.path.join(root, "src/session.ts"), (
        "import { createSession, isExpired, Session } from './auth'\n"
        "\n"
        "export function login(user: string): Session {\n"
        "  return createSession(user)\n"
        "}\n"
        "\n"
        "export function check(session: Session): string {\n"
        "  return isExpired(session) ? 'expired' : 'active'\n"
        "}\n"
    ))
    write(os.path.join(root, "tests/auth.test.ts"), (
        "import { login, check } from '../src/session'\n"
        "\n"
        "test('fresh login is active', () => {\n"
        "  expect(check(login('ann'))).toBe('active')\n"
        "})\n"
    ))
    write(os.path.join(root, "src/generated/types.ts"), (
        "export type Generated = { id: string }\n"
    ))
    write(os.path.join(root, "dist/bundle.min.js"), (
        "var x=1;var y=2;var z=x+y;console.log(z);\n"
    ))
    write(os.path.join(root, "package-lock.json"), (
        '{"name":"demo","lockfileVersion":3,"packages":{"node_modules/a":{"version":"1.0.0"}}}\n'
    ))
    write(os.path.join(root, "README.md"), "# Demo repo\n")


def fixture_budget(root):
    """Wider repo where a tight token budget forces lossy compression."""
    fixture_auth(root)
    for index in range(12):
        name = "mod%02d" % index
        write(os.path.join(root, "src", name + ".ts"), (
            "export function %s(a: string, b: string, c: string): string {\n"
            "  return a + b + c\n"
            "}\n" % name
        ))


def parse_stats(stdout):
    stats = {}
    for line in stdout.splitlines():
        line = line.strip()
        for key in ("raw_tokens", "shrunk_tokens", "tokens_saved",
                    "files_scanned", "files_dropped"):
            if line.startswith(key + ":"):
                stats[key] = int(line.split(":", 1)[1].strip())
        if line.startswith("saving_percent:"):
            stats["saving_percent"] = float(line.split(":", 1)[1].strip())
        if line.startswith("output_tokens:"):
            stats["output_tokens"] = line.split(":", 1)[1].strip()
        if line.startswith("over_budget:"):
            stats["over_budget"] = line.split(":", 1)[1].strip()
    return stats


def parse_context_files(output_path):
    tree = ET.parse(output_path)
    files = {}
    for element in tree.getroot().iter("file"):
        files[element.get("path", "")] = {
            "level": int(element.get("level", "1")),
            "tokens": int(element.get("tokens", "0")),
        }
    warnings = [node.text or "" for node in tree.getroot().iter("warning")]
    return files, warnings


def estimate_tokens(path):
    with open(path, encoding="utf-8") as handle:
        return max(1, len(handle.read()) // 4)


def run_task(binary, task, workdir):
    repo = task.get("repo") or os.path.join(workdir, task["name"])
    if task.get("fixture") is not None:
        os.makedirs(repo, exist_ok=True)
        task["fixture"](repo)
    output = os.path.join(workdir, task["name"] + ".xml")
    # Note: no --quiet, because --quiet also suppresses the stats report.
    cmd = [binary, repo, "--stats", "--output-file", output]
    cmd.extend(task.get("args", []))
    started = time.perf_counter()
    proc = subprocess.run(cmd, capture_output=True, text=True)
    elapsed = time.perf_counter() - started
    if proc.returncode != 0:
        raise SystemExit("task %s failed:\n%s" % (task["name"], proc.stderr))
    stats = parse_stats(proc.stdout)
    files, warnings = parse_context_files(output)
    important = task["important"]
    present = [p for p in important if p in files]
    full_detail = [p for p in present if files[p]["level"] == 1]
    missing = [p for p in important if p not in files]
    followup = sum(estimate_tokens(os.path.join(repo, p)) for p in missing)
    return {
        "name": task["name"],
        "question": task["question"],
        "seconds": elapsed,
        "stats": stats,
        "recall": len(present) / len(important),
        "full_detail_recall": len(full_detail) / len(important),
        "present": present,
        "missing": missing,
        "followup_estimate": followup,
        "warnings": warnings,
        "stats_stdout": proc.stdout,
    }


TASKS = [
    {
        "name": "auth-fix",
        "question": "Fix login session expiry handling.",
        "fixture": fixture_auth,
        "args": ["--focus", "fix login session expiry"],
        "important": ["src/auth.ts", "src/session.ts"],
    },
    {
        "name": "auth-fix-tight-budget",
        "question": "Fix login session expiry handling (level 2, 1500 tokens).",
        "fixture": fixture_budget,
        "args": ["--focus", "fix login session expiry",
                 "--level", "2", "--max-tokens", "1500"],
        "important": ["src/auth.ts", "src/session.ts"],
    },
    {
        "name": "bonsai-self-chunks",
        "question": "Where is stale numbered-chunk retirement implemented? (level 2)",
        "fixture": None,  # real repository, scanned in place
        "repo": REPO_ROOT,
        "args": ["--include", "src/**", "--level", "2",
                 "--focus", "retire stale numbered output chunks"],
        "important": ["src/walker.rs", "src/main.rs"],
    },
]


def main():
    binary = find_binary()
    workdir = tempfile.mkdtemp(prefix="bonsai-eval-")
    results = [run_task(binary, task, workdir) for task in TASKS]

    print("task | seconds | files | raw | shrunk | saved%% | "
          "recall | L1 recall | follow-up* | warnings")
    for result in results:
        stats = result["stats"]
        print("%s | %.2f | %s | %s | %s | %s | %.2f | %.2f | %d | %d" % (
            result["name"],
            result["seconds"],
            stats.get("files_scanned", "?"),
            stats.get("raw_tokens", "?"),
            stats.get("shrunk_tokens", "?"),
            stats.get("saving_percent", "?"),
            result["recall"],
            result["full_detail_recall"],
            result["followup_estimate"],
            len(result["warnings"]),
        ))
        if result["missing"]:
            print("  missing important files: %s"
                  % ", ".join(result["missing"]))
    print()
    print("*follow-up: estimated tokens (chars/4) to read important files "
          "missing from the context separately.")
    print("Artifacts kept under %s for inspection." % workdir)


if __name__ == "__main__":
    main()
