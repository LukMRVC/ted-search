//! Does dropping the `before+sub+after<=k` alignment gate fix harvesting?
//! Compares three harvest sources, each fed through a faithful replica of the
//! driver (public `TopDiffState`/`tree_dist`/`e_budget`), against the exact TED:
//!
//!   full   = k_relevant only, WHOLE band            (Touzet oracle)
//!   band   = k_relevant + structural test, WHOLE band (shipped StructBand)
//!   br     = k_relevant only, but ONLY the cells BR-SED's furthest-reaching
//!            snake examines (alignment gate removed) -- the proposed design
//!
//! Run: cargo run -p ted-lb-sed-struct-diff --release --example harvest_independent_test

use std::collections::HashMap;
use ted_base::TraversalSelection;
use ted_lb_sed_struct_diff::{
    e_budget, k_relevant, ted_k_struct_diff, StructDiffIndex, TopDiffState, TraversalCharacter,
};
use tree_parsing::{parse_single, LabelDict};

fn build(s: &str) -> StructDiffIndex {
    let mut d = LabelDict::default();
    StructDiffIndex::from_tree(&parse_single(s.to_string(), &mut d), TraversalSelection::default())
}

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0
    }
    fn range(&mut self, n: usize) -> usize {
        (self.next() >> 33) as usize % n
    }
}
fn random_tree(rng: &mut Lcg, max_nodes: usize, labels: &[&str]) -> String {
    let target = 1 + rng.range(max_nodes);
    let mut remaining = target - 1;
    fn go(rng: &mut Lcg, labels: &[&str], remaining: &mut usize) -> String {
        let mut s = String::from("{");
        s.push_str(labels[rng.range(labels.len())]);
        while *remaining > 0 {
            if rng.range(2) == 0 { break; }
            *remaining -= 1;
            s.push_str(&go(rng, labels, remaining));
        }
        s.push('}');
        s
    }
    go(rng, labels, &mut remaining)
}

// dedup exactly like emit_pair: keep largest x and largest y per keyroot pair,
// admit only k_relevant cells (NO alignment cost gate).
fn emit(
    t1: &StructDiffIndex, t2: &StructDiffIndex, k: i32, x: i32, y: i32,
    map: &mut HashMap<(i32, i32), usize>, vec: &mut Vec<(i32, i32)>,
) {
    if !k_relevant(t1, t2, x, y, k) { return; }
    let key = (t1.postl_to_kr_ancestor[x as usize], t2.postl_to_kr_ancestor[y as usize]);
    match map.get(&key) {
        None => { map.insert(key, vec.len()); vec.push((x, y)); }
        Some(&idx) => {
            if x > vec[idx].0 { vec[idx].0 = x; }
            if y > vec[idx].1 { vec[idx].1 = y; }
        }
    }
}

// full band, k_relevant only (Touzet oracle collection)
fn full_harvest(t1: &StructDiffIndex, t2: &StructDiffIndex, k: i32) -> Vec<(i32, i32)> {
    let (mut map, mut vec) = (HashMap::new(), Vec::new());
    for x in (0..t1.tree_size).rev() {
        let (y_hi, y_lo) = ((x + k).min(t2.tree_size - 1), (x - k).max(0));
        for y in (y_lo..=y_hi).rev() {
            emit(t1, t2, k, x, y, &mut map, &mut vec);
        }
    }
    vec
}

// full band + structural test (shipped harvest_pairs_via_sed_struct)
fn band_harvest(t1: &StructDiffIndex, t2: &StructDiffIndex, k: i32) -> Vec<(i32, i32)> {
    let (mut map, mut vec) = (HashMap::new(), Vec::new());
    for x in (0..t1.tree_size).rev() {
        let c1 = &t1.postl_struct[x as usize];
        let (y_hi, y_lo) = ((x + k).min(t2.tree_size - 1), (x - k).max(0));
        for y in (y_lo..=y_hi).rev() {
            let c2 = &t2.postl_struct[y as usize];
            if (c1.sum - c2.sum).abs() <= k && (c1.diff - c2.diff).abs() <= k {
                emit(t1, t2, k, x, y, &mut map, &mut vec);
            }
        }
    }
    vec
}

// instrumented BR-SED body: records every (row,col) its snake examines
fn br_sed_examined(s1: &[TraversalCharacter], s2: &[TraversalCharacter], k: usize, examined: &mut Vec<(i32, i32)>) {
    use std::cmp::{max, min};
    let s1len = s1.len() as i32;
    let s2len = s2.len() as i32;
    let size_diff = s2len - s1len;
    let threshold = min(s2len, k as i32);
    let zero_k: i32 = threshold + 1;
    let array_size = (2 * threshold + 3) as usize;
    let mut current_row = vec![(-1i32, true); array_size];
    let mut next_row = vec![(-1i32, true); array_size];
    let target_diagonal = size_diff + zero_k;
    let target_diagonal_idx = target_diagonal as usize;
    let end_max = target_diagonal << 1;
    for i in 1..=threshold + 1 {
        std::mem::swap(&mut next_row, &mut current_row);
        let original_start: i32 = if i <= zero_k { -i + 1 } else { i - (zero_k << 1) + 1 };
        let original_end: i32;
        if i <= target_diagonal {
            original_end = i;
            next_row[(zero_k + i) as usize] = (-1, true);
        } else {
            original_end = end_max - i;
        }
        let budget = k as i32 - (i - 1);
        let (min_valid_diag, max_valid_diag) = if budget <= 0 { (size_diff, size_diff) } else { (size_diff - budget, size_diff + budget) };
        let start = max(original_start, min_valid_diag);
        let end = min(original_end, max_valid_diag + 1);
        let (mut current_cell, mut next_cell, mut previous_cell, mut next_allowed_substitution);
        if i <= zero_k && start == original_start {
            current_cell = -1; next_cell = i - 2; next_allowed_substitution = true;
        } else {
            let start_idx = (zero_k + start) as usize;
            current_cell = if start > original_start && start_idx > 0 { current_row[start_idx - 1].0 } else { -1 };
            (next_cell, next_allowed_substitution) = current_row[start_idx];
        }
        let mut diagonal_index: usize = (start + zero_k).try_into().unwrap();
        let mut max_row_number;
        let allowed_edits = i - 1;
        let mut can_substitute;
        for diag_offset in start..end {
            previous_cell = current_cell;
            current_cell = next_cell;
            can_substitute = next_allowed_substitution;
            (next_cell, next_allowed_substitution) = current_row[diagonal_index + 1];
            max_row_number = max(current_cell + (if can_substitute { 1 } else { 0 }), max(previous_cell, next_cell + 1));
            if !can_substitute && max_row_number == current_cell {
                next_row[diagonal_index] = (max_row_number, false);
                diagonal_index += 1;
                continue;
            }
            let k = k as i32;
            let mut struct_ok = false;
            while max_row_number < s1len && (max_row_number + diag_offset) < s2len {
                let c1 = &s1[max_row_number as usize];
                let c2 = &s2[(max_row_number + diag_offset) as usize];
                examined.push((max_row_number, max_row_number + diag_offset)); // <-- record
                let char_eq = c1.char == c2.char;
                struct_ok = (allowed_edits + (c1.sum - c2.sum).abs() <= k) && (allowed_edits + (c1.diff - c2.diff).abs() <= k);
                if !char_eq || !struct_ok { break; }
                max_row_number += 1;
            }
            next_row[diagonal_index] = (max_row_number, struct_ok);
            diagonal_index += 1;
        }
        if next_row[target_diagonal_idx].0 >= s1len { return; }
    }
}

// harvest ONLY the cells BR-SED examines, k_relevant only (no alignment gate)
fn br_harvest(t1: &StructDiffIndex, t2: &StructDiffIndex, k: i32) -> Vec<(i32, i32)> {
    let swapped = t1.postl_struct.len() > t2.postl_struct.len();
    let (s1, s2) = if swapped { (&t2.postl_struct, &t1.postl_struct) } else { (&t1.postl_struct, &t2.postl_struct) };
    let mut examined = Vec::new();
    br_sed_examined(s1, s2, k as usize, &mut examined);
    let (mut map, mut vec) = (HashMap::new(), Vec::new());
    for (row, col) in examined {
        let (x, y) = if swapped { (col, row) } else { (row, col) };
        if x < 0 || y < 0 || x >= t1.tree_size || y >= t2.tree_size { continue; }
        emit(t1, t2, k, x, y, &mut map, &mut vec);
    }
    vec
}

// faithful replica of e_max_over_left_paths (private) via public e_budget + postl_to_lch
fn e_max_over_left_paths(t1: &StructDiffIndex, t2: &StructDiffIndex, x_l: i32, y_l: i32, k: i32) -> i32 {
    let mut e_max = 0;
    let mut top_x = x_l;
    while top_x > -1 {
        let mut top_y = y_l;
        while top_y > -1 {
            e_max = e_max.max(e_budget(t1, t2, top_x, top_y, k));
            top_y = t2.postl_to_lch[top_y as usize];
        }
        top_x = t1.postl_to_lch[top_x as usize];
    }
    e_max
}

// faithful replica of the driver's pair-consumption loop (early prune skipped:
// it never fires when true TED <= k, the only cases we score)
fn drive(t1: &StructDiffIndex, t2: &StructDiffIndex, k: i32, mut pairs: Vec<(i32, i32)>) -> i32 {
    if (t1.tree_size - t2.tree_size).abs() > k { return k + 1; }
    let mut state = TopDiffState::new(t1.tree_size, k);
    pairs.sort_unstable();
    for (x_l, y_l) in pairs {
        let e_max = e_max_over_left_paths(t1, t2, x_l, y_l, k);
        let d = state.tree_dist(t1, t2, x_l, y_l, k, e_max);
        state.td.set(x_l as usize, y_l as usize, d);
    }
    let result = state.td.read_at((t1.tree_size - 1) as usize, (t2.tree_size - 1) as usize);
    if !result.is_finite() || result > k as f64 { return k + 1; }
    result as i32
}

fn main() {
    let labels: &[&str] = &["a", "b"];
    let mut rng = Lcg(0xC0FFEE_1234_5678);
    let (mut scored, mut full_bad, mut band_bad, mut br_bad) = (0u64, 0u64, 0u64, 0u64);
    let mut shown = 0;

    for _ in 0..400_000 {
        let s1 = random_tree(&mut rng, 8, labels);
        let s2 = random_tree(&mut rng, 8, labels);
        let t1 = build(&s1);
        let t2 = build(&s2);
        let true_ted = ted_k_struct_diff(&t1, &t2, 40);
        for k in 1..=5 {
            if true_ted > k { continue; }
            scored += 1;
            if drive(&t1, &t2, k, full_harvest(&t1, &t2, k)) != true_ted { full_bad += 1; }
            if drive(&t1, &t2, k, band_harvest(&t1, &t2, k)) != true_ted { band_bad += 1; }
            let got_br = drive(&t1, &t2, k, br_harvest(&t1, &t2, k));
            if got_br != true_ted {
                br_bad += 1;
                if shown < 4 {
                    shown += 1;
                    let full: HashMap<_, _> = full_harvest(&t1, &t2, k).into_iter()
                        .map(|(x, y)| ((t1.postl_to_kr_ancestor[x as usize], t2.postl_to_kr_ancestor[y as usize]), (x, y))).collect();
                    let br: HashMap<_, _> = br_harvest(&t1, &t2, k).into_iter()
                        .map(|(x, y)| ((t1.postl_to_kr_ancestor[x as usize], t2.postl_to_kr_ancestor[y as usize]), (x, y))).collect();
                    println!("BR-harvest FAIL: t1={s1} t2={s2} k={k} true={true_ted} got={got_br}");
                    let mut keys: Vec<_> = full.keys().cloned().collect();
                    keys.sort();
                    for key in keys {
                        let f = full[&key];
                        match br.get(&key) {
                            None => println!("   keyroot {:?}: full rep {:?} -> BR MISSING", key, f),
                            Some(b) if *b != f => println!("   keyroot {:?}: full rep {:?} -> BR {:?}  (differs)", key, f, b),
                            _ => {}
                        }
                    }
                    println!();
                }
            }
        }
    }

    println!("scored cases (true TED <= k): {scored}");
    println!("  full  (k_relevant, whole band)          wrong: {full_bad}");
    println!("  band  (k_relevant+struct, whole band)   wrong: {band_bad}   <- shipped StructBand");
    println!("  br    (k_relevant, BR-SED cells only)   wrong: {br_bad}   <- the proposed 'harvest during BR-SED'");
}
