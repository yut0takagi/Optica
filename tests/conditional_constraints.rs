//! Issue #8: 条件付き制約 `if <cond> then <branch> else <branch>`。
//! cond は parse 時（インライン data / ループ変数）に評価し、真偽で制約の枝を選ぶ。
//! 値としての if 式（`x <= if a then 1 else 2`）とは区別される。

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn run(model: &str) -> (String, String, bool) {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("optica_cond_{}_{}", std::process::id(), n));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("m.optica");
    std::fs::write(&path, model).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_optica"))
        .args([path.to_str().unwrap(), "-i", "1200"])
        .output()
        .unwrap();
    (
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
        out.status.success(),
    )
}

fn objective_of(stdout: &str) -> f64 {
    stdout
        .lines()
        .find(|l| l.starts_with("Objective:"))
        .and_then(|l| l.split_whitespace().last())
        .and_then(|t| t.parse().ok())
        .unwrap_or(f64::NAN)
}

/// `if dist[w] <= 10 then y[w] <= 1 else y[w] == 0`。
/// A(dist=5) は then で y<=1（自由）、B(dist=20) は else で y==0（固定）→ 最適 1。
#[test]
fn conditional_constraint_picks_branch_by_data() {
    let model = "set W = {A, B};\n\
                 param dist[W] = {A: 5, B: 20};\n\
                 var y[W] >= 0 <= 1;\n\
                 maximize obj: sum{w in W} y[w];\n\
                 subject to cond: forall w in W: if dist[w] <= 10 then y[w] <= 1 else y[w] == 0;\n";
    let (stdout, stderr, ok) = run(model);
    assert!(ok, "should solve. stdout=\n{}\nstderr=\n{}", stdout, stderr);
    let obj = objective_of(&stdout);
    assert!(
        (obj - 1.0).abs() < 0.3,
        "A free(<=1) + B forced(==0) => ~1, got {}. stdout=\n{}",
        obj,
        stdout
    );
}

/// 値としての if 式は制約の辺として従来通り機能する（if 制約と区別される）。
/// f[A]=1>0 なので rhs=3 → y<=3 → 最適 3。
#[test]
fn if_expression_as_constraint_side_still_works() {
    let model = "set W = {A};\n\
                 param f[W] = {A: 1};\n\
                 var y[W] >= 0 <= 10;\n\
                 maximize obj: sum{w in W} y[w];\n\
                 subject to c: forall w in W: y[w] <= if f[w] > 0 then 3 else 8;\n";
    let (stdout, stderr, ok) = run(model);
    assert!(ok, "should solve. stdout=\n{}\nstderr=\n{}", stdout, stderr);
    let obj = objective_of(&stdout);
    assert!(
        (obj - 3.0).abs() < 0.3,
        "if-expression rhs => y<=3 => ~3, got {}. stdout=\n{}",
        obj,
        stdout
    );
}
