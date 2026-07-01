//! Probe: can a *completed* BR-SED / SED-alignment run harvest all the top-node
//! pairs TopDiff needs? Reconstructs `collect_pairs_full` (the proven-correct
//! oracle collection) and `sed_alignment` (the alignment-harvest) exactly from
//! the crate source, diagnoses the discrepancy, and confirms at the driver level.
//! Public API only + reconstructed private helpers.
//!
//! Run: cargo run -p ted-lb-sed-struct-diff --release --example br_sed_probe

use std::collections::HashMap;
use ted_base::TraversalSelection;
use ted_lb_sed_struct_diff::{
    bounded_string_edit_distance_with_structure as br_sed, k_relevant, ted_k_struct_diff,
    ted_k_with_source, PairSource, StructDiffIndex, TraversalCharacter,
};
use tree_parsing::{parse_single, LabelDict};

fn build(s: &str) -> StructDiffIndex {
    let mut d = LabelDict::default();
    StructDiffIndex::from_tree(&parse_single(s.to_string(), &mut d), TraversalSelection::default())
}

// ---- reproduce the test's random tree generator --------------------------
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
            if rng.range(2) == 0 {
                break;
            }
            *remaining -= 1;
            s.push_str(&go(rng, labels, remaining));
        }
        s.push('}');
        s
    }
    go(rng, labels, &mut remaining)
}

// ---- reconstruct collect_pairs_full (TopDiff's exact k_relevant collection) ----
// Representative per keyroot pair = (largest x, largest y), exactly as source.
fn collect_full(t1: &StructDiffIndex, t2: &StructDiffIndex, k: i32) -> HashMap<(i32, i32), (i32, i32)> {
    let mut reps: HashMap<(i32, i32), (i32, i32)> = HashMap::new();
    for x in (0..t1.tree_size).rev() {
        let x_kr = t1.postl_to_kr_ancestor[x as usize];
        let mut y = (x + k).min(t2.tree_size - 1);
        let y_low = (x - k).max(0);
        while y >= y_low {
            if k_relevant(t1, t2, x, y, k) {
                let key = (x_kr, t2.postl_to_kr_ancestor[y as usize]);
                reps.entry(key).and_modify(|r| { if y > r.1 { r.1 = y } }).or_insert((x, y));
            }
            y -= 1;
        }
    }
    reps
}

// ---- reconstruct sed_alignment's harvest (full forward+backward SED DP) --------
const SED_INF: i32 = 1 << 28;
fn sed_sub_compatible(c1: &TraversalCharacter, c2: &TraversalCharacter, k: i32) -> bool {
    (c1.sum - c2.sum).abs() <= k && (c1.diff - c2.diff).abs() <= k
}
fn sed_forward(s1: &[TraversalCharacter], s2: &[TraversalCharacter], k: i32) -> Vec<Vec<i32>> {
    let (n1, n2) = (s1.len(), s2.len());
    let mut f = vec![vec![SED_INF; n2 + 1]; n1 + 1];
    f[0][0] = 0;
    for i in 0..=n1 {
        for j in 0..=n2 {
            if i == 0 && j == 0 { continue; }
            let mut best = SED_INF;
            if i > 0 { best = best.min(f[i - 1][j] + 1); }
            if j > 0 { best = best.min(f[i][j - 1] + 1); }
            if i > 0 && j > 0 && f[i - 1][j - 1] < SED_INF {
                let (c1, c2) = (&s1[i - 1], &s2[j - 1]);
                if sed_sub_compatible(c1, c2, k) {
                    best = best.min(f[i - 1][j - 1] + i32::from(c1.char != c2.char));
                }
            }
            f[i][j] = if best > k { SED_INF } else { best };
        }
    }
    f
}
fn sed_backward(s1: &[TraversalCharacter], s2: &[TraversalCharacter], k: i32) -> Vec<Vec<i32>> {
    let (n1, n2) = (s1.len(), s2.len());
    let mut b = vec![vec![SED_INF; n2 + 1]; n1 + 1];
    b[n1][n2] = 0;
    for i in (0..=n1).rev() {
        for j in (0..=n2).rev() {
            if i == n1 && j == n2 { continue; }
            let mut best = SED_INF;
            if i < n1 { best = best.min(b[i + 1][j] + 1); }
            if j < n2 { best = best.min(b[i][j + 1] + 1); }
            if i < n1 && j < n2 && b[i + 1][j + 1] < SED_INF {
                let (c1, c2) = (&s1[i], &s2[j]);
                if sed_sub_compatible(c1, c2, k) {
                    best = best.min(b[i + 1][j + 1] + i32::from(c1.char != c2.char));
                }
            }
            b[i][j] = if best > k { SED_INF } else { best };
        }
    }
    b
}
/// Returns (lb, representatives-per-keyroot-pair) exactly as `sed_alignment`.
fn sed_align(t1: &StructDiffIndex, t2: &StructDiffIndex, k: i32) -> (i32, HashMap<(i32, i32), (i32, i32)>) {
    let (s1, s2) = (&t1.postl_struct, &t2.postl_struct);
    let (n1, n2) = (s1.len(), s2.len());
    let f = sed_forward(s1, s2, k);
    let lb = f[n1][n2].min(k + 1);
    let mut reps: HashMap<(i32, i32), (i32, i32)> = HashMap::new();
    if lb > k {
        return (lb, reps);
    }
    let b = sed_backward(s1, s2, k);
    for i in (1..=n1).rev() {
        for j in (1..=n2).rev() {
            let (c1, c2) = (&s1[i - 1], &s2[j - 1]);
            if !sed_sub_compatible(c1, c2, k) { continue; }
            let (before, after) = (f[i - 1][j - 1], b[i][j]);
            if before >= SED_INF || after >= SED_INF { continue; }
            if before + i32::from(c1.char != c2.char) + after <= k {
                let (x, y) = ((i - 1) as i32, (j - 1) as i32);
                if !k_relevant(t1, t2, x, y, k) { continue; }
                let key = (t1.postl_to_kr_ancestor[x as usize], t2.postl_to_kr_ancestor[y as usize]);
                reps.entry(key)
                    .and_modify(|r| { if x > r.0 { r.0 = x } if y > r.1 { r.1 = y } })
                    .or_insert((x, y));
            }
        }
    }
    (lb, reps)
}

fn br_sed_finishes(t1: &StructDiffIndex, t2: &StructDiffIndex, k: i32) -> String {
    let (a, b) = if t1.postl_struct.len() > t2.postl_struct.len() {
        (&t2.postl_struct, &t1.postl_struct)
    } else {
        (&t1.postl_struct, &t2.postl_struct)
    };
    match br_sed(a, b, k as usize) {
        usize::MAX => "usize::MAX (stops early, > k)".to_string(),
        d => format!("{d}  (FINISHES: <= k)"),
    }
}

fn diagnose(s1: &str, s2: &str, k: i32) {
    let t1 = build(s1);
    let t2 = build(s2);
    let true_ted = ted_k_struct_diff(&t1, &t2, 40);
    println!("--------------------------------------------------------------");
    println!("t1 = {s1}   t2 = {s2}   k = {k}");
    println!("true TED = {true_ted}   (<= k, correct answer is {true_ted})");
    println!("driver Full        = {}", ted_k_with_source(&t1, &t2, k, PairSource::Full));
    println!("driver StructBand  = {}   (shipped default, exact)", ted_k_with_source(&t1, &t2, k, PairSource::StructBand));
    println!("driver SedAlignment= {}   <-- alignment-harvest is WRONG", ted_k_with_source(&t1, &t2, k, PairSource::SedAlignment));
    println!("BR-SED distance    = {}", br_sed_finishes(&t1, &t2, k));

    let full = collect_full(&t1, &t2, k);
    let (_, sed) = sed_align(&t1, &t2, k);
    println!("\n  keyroot pair | Full rep (x,y) | SedAlign rep (x,y) | verdict");
    let mut keys: Vec<_> = full.keys().cloned().collect();
    keys.sort();
    for key in keys {
        let fr = full[&key];
        match sed.get(&key) {
            None => println!("  {:?}      | {:?}        | MISSING            | keyroot pair never emitted", key, fr),
            Some(sr) => {
                let verdict = if *sr == fr {
                    "ok"
                } else if sr.1 < fr.1 || sr.0 < fr.0 {
                    "rep too small -> tree_dist under-reaches"
                } else {
                    "differs"
                };
                println!("  {:?}      | {:?}        | {:?}          | {verdict}", key, fr, sr);
            }
        }
    }
}

fn main() {
    let labels: &[&str] = &["a", "b"];
    let mut rng = Lcg(0xC0FFEE_1234_5678);
    let mut found: Vec<(usize, String, String, i32)> = Vec::new();
    for _ in 0..400_000 {
        let s1 = random_tree(&mut rng, 7, labels);
        let s2 = random_tree(&mut rng, 7, labels);
        let t1 = build(&s1);
        let t2 = build(&s2);
        let true_ted = ted_k_struct_diff(&t1, &t2, 40);
        for k in 1..=5 {
            if true_ted <= k && ted_k_with_source(&t1, &t2, k, PairSource::SedAlignment) != true_ted {
                found.push(((t1.tree_size + t2.tree_size) as usize, s1.clone(), s2.clone(), k));
            }
        }
    }
    found.sort();
    found.dedup_by(|a, b| a.1 == b.1 && a.2 == b.2 && a.3 == b.3);
    println!("driver-level counterexamples (alignment-harvest over-estimates): {}\n", found.len());
    for (_, s1, s2, k) in found.iter().take(3) {
        diagnose(s1, s2, *k);
    }
}
