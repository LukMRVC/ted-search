//! Pin down exactly where the SED-alignment harvest drops the needed
//! representative for keyroot pair (2,5) on the counterexample
//!   t1 = {a{b{a}}}   t2 = {a{a}{b{b}{a{a{b}}}}}   k = 5   (true TED 4).
//!
//! Run: cargo run -p ted-lb-sed-struct-diff --release --example harvest_fail

use ted_base::TraversalSelection;
use ted_lb_sed_struct_diff::{k_relevant, StructDiffIndex, TraversalCharacter};
use tree_parsing::{parse_single, LabelDict};

fn build(s: &str) -> StructDiffIndex {
    let mut d = LabelDict::default();
    StructDiffIndex::from_tree(&parse_single(s.to_string(), &mut d), TraversalSelection::default())
}

const SED_INF: i32 = 1 << 28;
fn compat(c1: &TraversalCharacter, c2: &TraversalCharacter, k: i32) -> bool {
    (c1.sum - c2.sum).abs() <= k && (c1.diff - c2.diff).abs() <= k
}
// forward SED DP (copied from sed_forward)
fn fwd(s1: &[TraversalCharacter], s2: &[TraversalCharacter], k: i32) -> Vec<Vec<i32>> {
    let (n1, n2) = (s1.len(), s2.len());
    let mut f = vec![vec![SED_INF; n2 + 1]; n1 + 1];
    f[0][0] = 0;
    for i in 0..=n1 {
        for j in 0..=n2 {
            if i == 0 && j == 0 { continue; }
            let mut best = SED_INF;
            if i > 0 { best = best.min(f[i - 1][j] + 1); }
            if j > 0 { best = best.min(f[i][j - 1] + 1); }
            if i > 0 && j > 0 && f[i - 1][j - 1] < SED_INF && compat(&s1[i - 1], &s2[j - 1], k) {
                best = best.min(f[i - 1][j - 1] + i32::from(s1[i - 1].char != s2[j - 1].char));
            }
            f[i][j] = if best > k { SED_INF } else { best };
        }
    }
    f
}
// backward SED DP (copied from sed_backward)
fn bwd(s1: &[TraversalCharacter], s2: &[TraversalCharacter], k: i32) -> Vec<Vec<i32>> {
    let (n1, n2) = (s1.len(), s2.len());
    let mut b = vec![vec![SED_INF; n2 + 1]; n1 + 1];
    b[n1][n2] = 0;
    for i in (0..=n1).rev() {
        for j in (0..=n2).rev() {
            if i == n1 && j == n2 { continue; }
            let mut best = SED_INF;
            if i < n1 { best = best.min(b[i + 1][j] + 1); }
            if j < n2 { best = best.min(b[i][j + 1] + 1); }
            if i < n1 && j < n2 && b[i + 1][j + 1] < SED_INF && compat(&s1[i], &s2[j], k) {
                best = best.min(b[i + 1][j + 1] + i32::from(s1[i].char != s2[j].char));
            }
            b[i][j] = if best > k { SED_INF } else { best };
        }
    }
    b
}

fn main() {
    let k = 5;
    let t1 = build("{a{b{a}}}");
    let t2 = build("{a{a}{b{b}{a{a{b}}}}}");
    let (s1, s2) = (&t1.postl_struct, &t2.postl_struct);
    let f = fwd(s1, s2, k);
    let b = bwd(s1, s2, k);

    println!("t2 postorder (id: label sum diff  size depth kr_anc):");
    for y in 0..t2.tree_size as usize {
        let c = &t2.postl_struct[y];
        println!(
            "  {y}: label={} sum={} diff={}  size={} depth={} kr_anc={}",
            c.char, c.sum, c.diff, t2.postl_to_size[y], t2.postl_to_depth[y], t2.postl_to_kr_ancestor[y]
        );
    }
    println!("t1 postorder: 0:a 1:b 2:a  (path; kr_anc all = 2, the root/keyroot)\n");

    // Keyroot pair (kr_x=2, kr_y=5): which (x,y) does Full keep vs the harvest?
    println!("Keyroot pair (kr_x=2, kr_y=5) candidates (x with kr_anc=2, y with kr_anc=5):");
    println!("  (x, y) | k_relevant | compat | before f[x][y] | sub | after b[x+1][y+1] | tot | emit gate before+sub+after<=k");
    for x in 0..t1.tree_size {
        if t1.postl_to_kr_ancestor[x as usize] != 2 { continue; }
        for y in 0..t2.tree_size {
            if t2.postl_to_kr_ancestor[y as usize] != 5 { continue; }
            if (x - y).abs() > k { continue; }
            let kr = k_relevant(&t1, &t2, x, y, k);
            let (c1, c2) = (&s1[x as usize], &s2[y as usize]);
            let cp = compat(c1, c2, k);
            let before = f[x as usize][y as usize];
            let after = b[(x + 1) as usize][(y + 1) as usize];
            let sub = i32::from(c1.char != c2.char);
            let tot = if before >= SED_INF || after >= SED_INF { SED_INF } else { before + sub + after };
            let emitted = cp && before < SED_INF && after < SED_INF && tot <= k;
            let show = |v: i32| if v >= SED_INF { "INF".to_string() } else { v.to_string() };
            println!(
                "  ({x}, {y}) | {:<10} | {:<6} | {:<14} | {sub}   | {:<17} | {:<3} | {}  {}",
                kr, cp, show(before), show(after), show(tot), emitted,
                if kr && !emitted { "<-- NEEDED by Full, but harvest DROPS it" } else { "" }
            );
        }
    }
}
