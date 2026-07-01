//! Emit the "Top node pairs" figure data for a tree pair at threshold `k` as a
//! single JSON object on stdout, for plotting in a notebook.
//!
//! The matrix is over the two postorder traversals: rows indexed by `x`
//! (postorder id in tree 1), columns by `y` (postorder id in tree 2). Four
//! boolean layers are emitted per cell — `in_band` (light gray, `|x-y|<=k`),
//! `struct_ok` (dark gray, the SED-STRUCT neighborhood test), `k_relevant`, and
//! `is_topnode` (green, the harvested top-node pairs) — plus the raw
//! `topnode_pairs` list fed to `tree_dist`.
//!
//! Run:
//!   cargo run -p ted-lb-sed-struct-diff --release --example topnodes_matrix -- \
//!       '{a{b}{c}}' '{a{b}{x}}' 3 [source]
//!
//! `source` selects the green (top-node) layer: one of `full`, `struct`,
//! `structband` (default, the shipped harvest), or `sed`.

use std::process::exit;

use ted_base::TraversalSelection;
use ted_lb_sed_struct_diff::{topnode_matrix, PairSource, StructDiffIndex, TopNodeMatrix};
use tree_parsing::{parse_single, LabelDict};

fn build(s: &str, dict: &mut LabelDict) -> StructDiffIndex {
    StructDiffIndex::from_tree(&parse_single(s.to_string(), dict), TraversalSelection::default())
}

fn parse_source(s: &str) -> Option<PairSource> {
    match s.to_ascii_lowercase().as_str() {
        "full" => Some(PairSource::Full),
        "struct" => Some(PairSource::Struct),
        "structband" | "struct-band" => Some(PairSource::StructBand),
        "sed" | "sedalignment" | "sed-alignment" => Some(PairSource::SedAlignment),
        _ => None,
    }
}

/// Write a `n1×n2` boolean grid as a JSON array-of-arrays of `0`/`1`.
fn write_grid(out: &mut String, grid: &[Vec<bool>]) {
    out.push('[');
    for (i, row) in grid.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push('[');
        for (j, &b) in row.iter().enumerate() {
            if j > 0 {
                out.push(',');
            }
            out.push(if b { '1' } else { '0' });
        }
        out.push(']');
    }
    out.push(']');
}

fn write_int_array(out: &mut String, xs: &[i32]) {
    out.push('[');
    for (i, x) in xs.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&x.to_string());
    }
    out.push(']');
}

fn to_json(m: &TopNodeMatrix) -> String {
    let mut out = String::new();
    out.push('{');
    out.push_str(&format!("\"k\":{},", m.k));
    out.push_str(&format!("\"n1\":{},", m.n1));
    out.push_str(&format!("\"n2\":{},", m.n2));

    out.push_str("\"x_labels\":");
    write_int_array(&mut out, &m.x_labels);
    out.push(',');
    out.push_str("\"y_labels\":");
    write_int_array(&mut out, &m.y_labels);
    out.push(',');

    out.push_str("\"in_band\":");
    write_grid(&mut out, &m.in_band);
    out.push(',');
    out.push_str("\"struct_ok\":");
    write_grid(&mut out, &m.struct_ok);
    out.push(',');
    out.push_str("\"k_relevant\":");
    write_grid(&mut out, &m.k_relevant);
    out.push(',');
    out.push_str("\"is_topnode\":");
    write_grid(&mut out, &m.is_topnode);
    out.push(',');
    out.push_str("\"br_sed_visited\":");
    write_grid(&mut out, &m.br_sed_visited);
    out.push(',');

    out.push_str("\"topnode_pairs\":[");
    for (i, &(x, y)) in m.topnode_pairs.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!("[{x},{y}]"));
    }
    out.push(']');

    out.push('}');
    out
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!(
            "usage: {} <tree1> <tree2> <k> [source]\n\
             source: full | struct | structband (default) | sed",
            args.first().map(String::as_str).unwrap_or("topnodes_matrix")
        );
        exit(2);
    }

    let s1 = &args[1];
    let s2 = &args[2];
    let k: i32 = match args[3].parse() {
        Ok(k) => k,
        Err(_) => {
            eprintln!("k must be an integer, got '{}'", args[3]);
            exit(2);
        }
    };
    let source = match args.get(4) {
        None => PairSource::StructBand,
        Some(s) => match parse_source(s) {
            Some(src) => src,
            None => {
                eprintln!("unknown source '{s}' (full | struct | structband | sed)");
                exit(2);
            }
        },
    };

    let mut dict = LabelDict::default();
    let t1 = build(s1, &mut dict);
    let t2 = build(s2, &mut dict);
    let m = topnode_matrix(&t1, &t2, k, source);

    println!("{}", to_json(&m));
}
