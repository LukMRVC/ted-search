# Reproduction usage

Clone this repository and run `prepare_repo.sh` bash script.




# TED CLI Usage

This repository currently contains one CLI package in the workspace:

- `ted-search-cli` (tree search CLI)

## tree-search-cli (`ted-search-cli`)

Run from repository root:

```bash
cargo run -p ted-search-cli -- --help
```

Usage:

```text
ted-search-cli [OPTIONS] --dataset <DATASET> --queries <QUERIES> <METHOD>
```

Required arguments:

- `--dataset`, `-d`: path to dataset file
- `--queries`, `-q`: path to query CSV (`<threshold>;<tree>` format)
- `<METHOD>`: lower bound method, one of:
  - `lblint`
  - `sed`
  - `sed-exact`
  - `sed-struct`
  - `structural`
  - `bib`

Optional arguments:

- `--runs`, `-r`: run count for repeated benchmarking (default `1`)
- `--delimiter`: query CSV delimiter (default `;`)
- `--traversal-a`: first traversal for `sed`, `sed-exact`, and `sed-struct` (default `preorder`)
- `--traversal-b`: second traversal for `sed`, `sed-exact`, and `sed-struct` (default `postorder`)

Traversal option values:

- `preorder`
- `postorder`
- `reversed-preorder`
- `reversed-postorder`

Example:

```bash
cargo run -p ted-search-cli -- \
  --dataset article/datasets/labels-10/collection.csv \
  --queries article/datasets/labels-10/query.csv \
  --runs 3 \
  --delimiter ';' \
  --traversal-a preorder \
  --traversal-b reversed-postorder \
  sed-struct
```

## tree-statistics-cli

`tree-statistics-cli` is not currently registered as a Cargo package in this workspace.

Current status:

- `bin/tree-statistics-cli/` exists but is empty.
- `cargo run -p tree-statistics-cli -- --help` fails because the package is not in `Cargo.toml` workspace members.

When the crate is added, document its usage here in the same style as `ted-search-cli`.
