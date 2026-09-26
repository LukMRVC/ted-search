//! Turn the dataset bracket files into COPY-TEXT rows for the Postgres
//! tree-similarity benchmark, keeping only the lines the reference dataset
//! loader keeps.
//!
//! `COPY` aborts the whole load on the first unparseable value, whereas
//! `tree_parsing::parse_dataset` silently drops malformed and non-ASCII lines.
//! Filtering here through `tree_parsing::accepts_line` — the reference code
//! itself — makes the loaded collection identical to the one ted-search
//! benchmarks, without re-deriving the bracket escape rules.
//!
//!   bracket-prep trees   <file>   ->  id \t tree
//!   bracket-prep queries <file>   ->  id \t k \t tree
//!
//! `id` is the original 0-based file line number, so dropped lines never shift
//! the ids of the lines that survive. Dropped counts go to stderr.

use std::env;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::process::ExitCode;

/// Query files are `<threshold><QUERY_DELIM><tree>`. Split on the first
/// delimiter only: 28 swissprot query trees contain `;` in their labels.
const QUERY_DELIM: char = ';';

/// Escape a value for `COPY ... FROM STDIN` in TEXT format, where backslash is
/// the escape character. The dataset files contain no tabs, newlines or CRs
/// (every line is one whole tree), but escaping them costs nothing and keeps
/// the output well-formed if that ever changes.
fn escape_copy(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 8);
    for c in value.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            _ => out.push(c),
        }
    }
    out
}

enum Mode {
    Trees,
    Queries,
}

fn run(mode: Mode, path: &str) -> io::Result<(u64, u64)> {
    let reader = BufReader::new(File::open(path)?);
    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());

    let mut kept = 0u64;
    let mut dropped = 0u64;

    for (idx, line) in reader.lines().enumerate() {
        let line = line?;
        match mode {
            Mode::Trees => {
                if tree_parsing::accepts_line(&line) {
                    writeln!(out, "{}\t{}", idx, escape_copy(&line))?;
                    kept += 1;
                } else {
                    dropped += 1;
                }
            }
            Mode::Queries => {
                // A query line without a delimiter, a non-numeric threshold, or
                // an unparseable tree is dropped the same way the reference
                // `parse_queries` drops it.
                let parsed = line
                    .split_once(QUERY_DELIM)
                    .and_then(|(k, tree)| k.trim().parse::<i32>().ok().map(|k| (k, tree)))
                    .filter(|(_, tree)| tree_parsing::accepts_line(tree));

                match parsed {
                    Some((k, tree)) => {
                        writeln!(out, "{}\t{}\t{}", idx, k, escape_copy(tree))?;
                        kept += 1;
                    }
                    None => dropped += 1,
                }
            }
        }
    }

    out.flush()?;
    Ok((kept, dropped))
}

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let (mode, path) = match args.get(1).map(String::as_str) {
        Some("trees") if args.len() == 3 => (Mode::Trees, &args[2]),
        Some("queries") if args.len() == 3 => (Mode::Queries, &args[2]),
        _ => {
            eprintln!("usage: bracket-prep <trees|queries> <file>");
            return ExitCode::from(2);
        }
    };

    match run(mode, path) {
        Ok((kept, dropped)) => {
            eprintln!("{path}: kept {kept}, dropped {dropped}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("bracket-prep: {path}: {e}");
            ExitCode::FAILURE
        }
    }
}
