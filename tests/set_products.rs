//! Issue #11: 集合の直積 `set C = A * B`。タプル要素は "a,b" のカンマ連結で保持し、
//! `var x[C]` / `sum{c in C}` / `forall c in C` の `x[c]` が `x[a,b]` と一致して動く。

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn run(model: &str) -> (String, String, bool) {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("optica_prod_{}_{}", std::process::id(), n));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("m.optica");
    std::fs::write(&path, model).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_optica"))
        .args([path.to_str().unwrap(), "-i", "1000"])
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

/// 直積集合を使った変数宣言 + sum。A(2)×B(2)=4 変数、cap 10 → 最適 10。
#[test]
fn product_set_var_and_sum() {
    let model = "set A = {1, 2};\n\
                 set B = {X, Y};\n\
                 set C = A * B;\n\
                 var z[C] >= 0 <= 5;\n\
                 maximize obj: sum{c in C} z[c];\n\
                 subject to lim: sum{c in C} z[c] <= 10;\n";
    let (stdout, stderr, ok) = run(model);
    assert!(ok, "should solve. stdout=\n{}\nstderr=\n{}", stdout, stderr);
    assert!(
        (objective_of(&stdout) - 10.0).abs() < 0.3,
        "cap 10 => ~10. stdout=\n{}",
        stdout
    );
}

/// forall over product。4 変数それぞれ <=2 → 最適 8。
#[test]
fn product_set_forall() {
    let model = "set A = {1, 2};\n\
                 set B = {X, Y};\n\
                 set C = A * B;\n\
                 var z[C] >= 0 <= 5;\n\
                 maximize obj: sum{c in C} z[c];\n\
                 subject to each: forall c in C: z[c] <= 2;\n";
    let (stdout, stderr, ok) = run(model);
    assert!(ok, "should solve. stdout=\n{}\nstderr=\n{}", stdout, stderr);
    assert!(
        (objective_of(&stdout) - 8.0).abs() < 0.3,
        "4 vars each <=2 => ~8. stdout=\n{}",
        stdout
    );
}

/// 未定義のオペランドは明示 parse error。
#[test]
fn product_unknown_operand_errors() {
    let model = "set C = A * B;\nvar z[C] >= 0 <= 1;\nmaximize o: sum{c in C} z[c];\n";
    let (_stdout, stderr, ok) = run(model);
    assert!(!ok, "unknown operand must fail. stderr=\n{}", stderr);
    assert!(
        stderr.contains("unknown set"),
        "should name the unknown operand set. stderr=\n{}",
        stderr
    );
}
