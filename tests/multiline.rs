use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

// NOTE(deviation from plan): the plan's `run()` helper keys the temp dir only on
// `std::process::id()`, which is identical across all threads of this test binary.
// Since `cargo test` runs tests in parallel by default, both tests below (sharing
// this helper and the literal filename "m.optica") raced on the same file, causing
// intermittent failures. Add a per-call counter to keep paths unique.
static CALL_COUNTER: AtomicU64 = AtomicU64::new(0);

fn run(src: &str, args: &[&str]) -> String {
    let n = CALL_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("optica_ml_{}_{}", std::process::id(), n));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("m.optica");
    std::fs::write(&path, src).unwrap();
    let mut a = vec![path.to_str().unwrap().to_string()];
    for s in args {
        a.push(s.to_string());
    }
    let out = Command::new(env!("CARGO_BIN_EXE_optica"))
        .args(&a)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).to_string()
}

// 注: これらは「複数行の取り込み/forall 展開」を検証する。収束修正(Task 4)に依存しないよう、
// 最適 fitness が 0 or 正の問題を使う（旧 early-return でも正しく収束する）。

#[test]
fn multiline_objective_is_captured() {
    // 目的が次行。captured なら x→3（最適0）、dropped なら既定 Sphere で x→0。x で判別。
    let src = "set S = {1, 2};\nvar x[S] >= 0 <= 5;\nminimize c:\n    sum(i in S) (x[i] - 3) ^ 2\n";
    let out = run(src, &["-m", "de", "-i", "2000"]);
    assert!(
        out.contains("= 3.") || out.contains("= 2.9"),
        "objective dropped? x should be ~3: {out}"
    );
}

#[test]
fn forall_constraint_expands() {
    // forall i in S: x[i] >= 2 を各要素へ展開。minimize sum x[i] は展開時のみ最適 6、
    // 展開されないと 0。最適 fitness=6(正) なので収束修正前でも収束する。
    let src = "set S = {1, 2, 3};\nvar x[S] >= 0 <= 10;\nminimize c:\n    sum(i in S) x[i]\nsubject to:\n    lo:\n        forall i in S:\n            x[i] >= 2\n";
    let out = run(src, &["-m", "de", "-i", "2000"]);
    let o: f64 = out
        .lines()
        .find(|l| l.starts_with("Objective:"))
        .unwrap()
        .split_whitespace()
        .last()
        .unwrap()
        .parse()
        .unwrap();
    assert!(
        (o - 6.0).abs() < 0.1,
        "forall not expanded? expected ~6: {out}"
    );
}
