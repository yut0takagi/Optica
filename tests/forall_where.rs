//! Issue #10: `forall ... where <cond>` の条件付き展開。
//! where 条件が true の組み合わせだけ制約を生成する。where 条件は parse 時に評価するため、
//! 参照する param はインライン data 由来のもののみ有効。

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn run(model: &str) -> (String, String, bool) {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("optica_where_{}_{}", std::process::id(), n));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("m.optica");
    std::fs::write(&path, model).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_optica"))
        .args([path.to_str().unwrap(), "-i", "1500"])
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

/// `where p[i] > 0` は p>0 の i だけを制約する。A,C は cap 1、B は自由(<=5) → 最適 7。
#[test]
fn where_filters_single_subscript_by_param() {
    let model = "set I = {A, B, C};\n\
                 param p[I] = {A: 1, B: 0, C: 1};\n\
                 var x[I] >= 0 <= 5;\n\
                 maximize obj: sum{i in I} x[i];\n\
                 subject to lim: forall i in I where p[i] > 0: x[i] <= 1;\n";
    let (stdout, stderr, ok) = run(model);
    assert!(ok, "should solve. stdout=\n{}\nstderr=\n{}", stdout, stderr);
    let obj = objective_of(&stdout);
    assert!(
        (obj - 7.0).abs() < 0.5,
        "where should cap only A,C (=1) leaving B free (=5) => ~7, got {}. stdout=\n{}",
        obj,
        stdout
    );
}

/// 複数添字 + where（ループ変数条件）。対角 j==m は自由(<=3)、非対角は cap 1 → 最適 8。
#[test]
fn where_filters_multiple_subscripts() {
    let model = "set J = {1, 2};\n\
                 set M = {1, 2};\n\
                 var y[J, M] >= 0 <= 3;\n\
                 maximize obj: sum{j in J} sum{m in M} y[j, m];\n\
                 subject to c: forall j in J, m in M where j != m: y[j, m] <= 1;\n";
    let (stdout, stderr, ok) = run(model);
    assert!(ok, "should solve. stdout=\n{}\nstderr=\n{}", stdout, stderr);
    assert!(
        !stderr.contains("bad forall binding"),
        "comma inside where must not break header parsing. stderr=\n{}",
        stderr
    );
    let obj = objective_of(&stdout);
    assert!(
        (obj - 8.0).abs() < 0.5,
        "diagonal free (3+3) + off-diagonal capped (1+1) => ~8, got {}. stdout=\n{}",
        obj,
        stdout
    );
}

/// 空の where 条件は原因の分かるエラーになる（#10 完了条件3）。
#[test]
fn empty_where_errors_clearly() {
    let model = "set I = {A, B};\n\
                 var x[I] >= 0 <= 1;\n\
                 maximize obj: sum{i in I} x[i];\n\
                 subject to c: forall i in I where : x[i] <= 1;\n";
    let (_stdout, stderr, ok) = run(model);
    assert!(!ok, "empty where must fail. stderr=\n{}", stderr);
    assert!(
        stderr.to_lowercase().contains("where"),
        "error should mention the where clause. stderr=\n{}",
        stderr
    );
}
