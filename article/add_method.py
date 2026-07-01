#!/usr/bin/env python3
"""Incrementally add a single lower-bound method to the real-datasets experiment.

The original ``run_experiments.sh`` deletes ``query_times.csv`` and re-runs the
whole pipeline (all five filters + full verification) for every dataset.  This
script lets you add one new method without re-running the other filters:

  1. Filter (incremental): run the Rust CLI for *only* the new method.  This
     (re)writes ``<DisplayName>_candidates.csv`` and emits one timing block.
  2. query_times.csv (idempotent): drop any existing block for this method and
     append the fresh one.  Other methods' timings are left untouched, so
     re-running the same method never produces duplicate blocks.
  3. Verify (full): re-run ``query_validate`` over *all* ``*_candidates.csv``
     files in the dataset dir, regenerating ``verified-all.csv``.  query_validate
     deduplicates (query, candidate) pairs across files, so the new method's
     pairs are simply folded into the union.

The three artifacts produced match exactly what ``graphs.ipynb`` consumes.

Usage:
    python add_method.py sed-struct-diff
    python add_method.py sed-struct-diff --datasets sentiment rna
    python add_method.py sed --runs 5 --sed-traversal-first preorder \\
                             --sed-traversal-second postorder

``<method>`` is the kebab-case CLI value accepted by the Rust binary
(e.g. ``sed``, ``sed-struct``, ``sed-struct-diff``, ``structural``, ``bib``,
``lblint``).  The script reads the method's *display name* (used for both the
candidate filename and the query_times.csv block header) directly from the
CLI's own formatted output, so there is no name mapping to keep in sync.
"""

from __future__ import annotations

import argparse
import subprocess
import sys
from pathlib import Path

# Resolve everything relative to this script's location (article/), so the
# script works regardless of the current working directory.
SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parent
DATASETS_DIR = SCRIPT_DIR / "datasets"
QUERY_VALIDATE = REPO_ROOT / "external-sources" / "build" / "query_validate"

# Mirrors the SKIP_TARGET in run_experiments.sh: this directory holds the
# separate Ukkonen-vs-regular sub-experiment, not a real dataset.
EXCLUDED_DATASETS = {"ukkonen-vs-regular-sed-struct-test"}


def eprint(*args, **kwargs) -> None:
    print(*args, file=sys.stderr, **kwargs)


def discover_datasets(names: list[str] | None) -> list[Path]:
    """Return the dataset directories to process.

    With no ``names``, returns every directory under ``datasets/`` except the
    excluded sub-experiment.  With ``names``, returns those specific datasets
    (by directory basename) and errors out if any are missing.
    """
    if not DATASETS_DIR.is_dir():
        sys.exit(f"Datasets directory not found: {DATASETS_DIR}")

    if names:
        selected = []
        for name in names:
            path = DATASETS_DIR / name
            if not path.is_dir():
                sys.exit(f"Requested dataset is not a directory: {path}")
            selected.append(path)
        return selected

    return sorted(
        p
        for p in DATASETS_DIR.iterdir()
        if p.is_dir() and p.name not in EXCLUDED_DATASETS
    )


def run_filter(
    dataset: Path,
    method: str,
    runs: int,
    traversal_first: str,
    traversal_second: str,
) -> list[str]:
    """Run the Rust CLI for one method on one dataset.

    Returns the three formatted output lines ``[<DisplayName>, time:Xms,
    candidates:N]`` captured from the CLI's stdout.  The CLI also writes
    ``<DisplayName>_candidates.csv`` into the dataset directory as a side effect.
    """
    cmd = [
        "cargo", "run", "--release", "--",
        "--formatted",
        "--runs", str(runs),
        "--dataset", str(dataset / "trees_sorted.bracket"),
        "--queries", str(dataset / "query.csv"),
        "--output", str(dataset),
        method,
        "--sed-traversal-first", traversal_first,
        "--sed-traversal-second", traversal_second,
    ]
    # cwd=REPO_ROOT so `cargo run` finds the workspace manifest. Compilation
    # progress and the spinner go to stderr; the formatted result is on stdout.
    proc = subprocess.run(
        cmd, cwd=REPO_ROOT, text=True, capture_output=True
    )
    if proc.returncode != 0:
        eprint(proc.stderr)
        sys.exit(f"cargo run failed for method '{method}' on {dataset.name}")

    lines = [ln for ln in proc.stdout.splitlines() if ln.strip()]
    # Expected: <DisplayName> / time:Xms / candidates:N
    if len(lines) < 3 or not lines[1].startswith("time:") or not lines[2].startswith("candidates:"):
        eprint("Unexpected CLI output (stdout):")
        eprint(proc.stdout)
        sys.exit(f"Could not parse timing block for '{method}' on {dataset.name}")

    return lines[:3]


def parse_blocks(text: str) -> list[list[str]]:
    """Parse query_times.csv into 3-line-ish blocks.

    A line without ``:`` starts a new block (the method display name); following
    lines containing ``:`` (``time:``, ``candidates:``) belong to it.  This
    mirrors the parser in graphs.ipynb's ``read_query_times``.
    """
    blocks: list[list[str]] = []
    current: list[str] | None = None
    for raw in text.splitlines():
        line = raw.strip()
        if not line:
            continue
        if ":" not in line:
            current = [line]
            blocks.append(current)
        elif current is not None:
            current.append(line)
        # else: stray "key:value" line before any header -> ignore
    return blocks


def update_query_times(dataset: Path, block: list[str]) -> None:
    """Idempotently replace this method's block in query_times.csv.

    Drops any existing block whose header matches (case-insensitively) and
    appends the fresh block.  Creates the file if it does not exist.
    """
    path = dataset / "query_times.csv"
    display = block[0]

    existing = parse_blocks(path.read_text()) if path.exists() else []
    kept = [b for b in existing if b[0].strip().lower() != display.strip().lower()]
    kept.append(block)

    path.write_text("\n".join("\n".join(b) for b in kept) + "\n")


def regenerate_verified(dataset: Path) -> int:
    """Re-run query_validate over all candidate files, rewriting verified-all.csv.

    Returns the number of candidate files fed to the validator.
    """
    candidate_files = sorted(dataset.glob("*_candidates.csv"))
    if not candidate_files:
        eprint(f"  [skip verify] no *_candidates.csv files in {dataset.name}")
        return 0

    print(f"  Verifying union of {len(candidate_files)} candidate file(s):")
    for cf in candidate_files:
        print(f"    - {cf.name}")

    cmd = [
        str(QUERY_VALIDATE),
        str(dataset / "trees_sorted.bracket"),
        str(dataset / "query.csv"),
        *[str(cf) for cf in candidate_files],
    ]
    out_path = dataset / "verified-all.csv"
    with out_path.open("w") as out:
        proc = subprocess.run(cmd, stdout=out, text=True)
    if proc.returncode != 0:
        sys.exit(f"query_validate failed on {dataset.name}")

    return len(candidate_files)


def process_dataset(dataset: Path, args: argparse.Namespace) -> dict | None:
    """Run all three steps for one dataset. Returns a summary row, or None if skipped."""
    trees = dataset / "trees_sorted.bracket"
    queries = dataset / "query.csv"
    if not trees.is_file() or not queries.is_file():
        eprint(f"Skipping {dataset.name}: missing trees_sorted.bracket or query.csv")
        return None

    print(f"Processing: {dataset.name}")

    print(f"  Running filter: {args.method}")
    block = run_filter(
        dataset, args.method, args.runs,
        args.sed_traversal_first, args.sed_traversal_second,
    )
    display = block[0]
    time_str = block[1].replace("time:", "").strip()
    cand_str = block[2].replace("candidates:", "").strip()
    print(f"    {display}: {time_str}, {cand_str} candidates")

    update_query_times(dataset, block)
    print("  Updated query_times.csv")

    n_files = regenerate_verified(dataset)
    print(f"  Regenerated verified-all.csv\n")

    return {
        "dataset": dataset.name,
        "display": display,
        "time": time_str,
        "candidates": cand_str,
        "candidate_files": n_files,
    }


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Incrementally add one method to the real-datasets experiment.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "method",
        help="Kebab-case CLI method value (e.g. sed, sed-struct, sed-struct-diff, structural, bib, lblint)",
    )
    parser.add_argument(
        "--datasets", nargs="+", metavar="NAME",
        help="Dataset directory names to process (default: all under datasets/ except the ukkonen sub-experiment)",
    )
    parser.add_argument(
        "--runs", type=int, default=3,
        help="Number of timed runs; the fastest is reported (default: 3)",
    )
    parser.add_argument(
        "--sed-traversal-first", default="reversed-preorder",
        help="First traversal for SED-based methods (default: reversed-preorder, matching run_experiments.sh)",
    )
    parser.add_argument(
        "--sed-traversal-second", default="preorder",
        help="Second traversal for SED-based methods (default: preorder, matching run_experiments.sh)",
    )
    args = parser.parse_args()

    if not QUERY_VALIDATE.is_file():
        sys.exit(
            f"query_validate not found at {QUERY_VALIDATE}\n"
            "Build it first (see external-sources)."
        )

    datasets = discover_datasets(args.datasets)
    if not datasets:
        sys.exit("No datasets to process.")

    print(f"Adding method '{args.method}' to {len(datasets)} dataset(s)\n")

    summaries = []
    for dataset in datasets:
        result = process_dataset(dataset, args)
        if result is not None:
            summaries.append(result)

    print("=" * 60)
    print(f"Done. Added '{args.method}' to {len(summaries)} dataset(s):")
    for s in summaries:
        print(
            f"  {s['dataset']:<24} {s['display']:<16} "
            f"{s['time']:>10}  {s['candidates']:>12} candidates"
        )


if __name__ == "__main__":
    main()
