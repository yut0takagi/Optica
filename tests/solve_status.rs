//! Issue #23: 解ステータス表示の正直化。
//! ヒューリスティック（DE/PSO/hybrid）は最適性を証明しないため、目的値が 0 に近くても
//! `optimal` と表示してはならない。制約違反が残る解は `infeasible` と表示する。

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// 一意な一時パスにモデルを書き出してバイナリを実行し、(stdout, stderr, success) を返す。
fn run(src: &str, args: &[&str]) -> (String, String, bool) {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("optica_status_{}_{}", std::process::id(), n));
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
    (
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
        out.status.success(),
    )
}

#[test]
fn heuristic_solution_not_labeled_optimal() {
    // (x-3)^2 の最小化。最適 x=3, obj=0 だが DE は最適性を証明しないので optimal 禁止。
    let (stdout, _stderr, ok) = run(
        "var x >= 0 <= 10;\nminimize obj: (x - 3) ^ 2;\n",
        &["-m", "de", "-i", "500"],
    );
    assert!(ok, "solve should succeed. stdout=\n{}", stdout);
    assert!(
        !stdout.contains("Status: optimal"),
        "heuristic solution must not be labeled optimal. stdout=\n{}",
        stdout
    );
    assert!(
        stdout.contains("Status: heuristic_feasible"),
        "feasible heuristic solution should be heuristic_feasible. stdout=\n{}",
        stdout
    );
}

#[test]
fn infeasible_model_reports_infeasible() {
    // x∈[0,1] に対し x>=5 は満たせない。制約違反が残るので infeasible。
    let (stdout, _stderr, _ok) = run(
        "var x >= 0 <= 1;\nminimize obj: x;\nsubject to\n  c1: x >= 5;\n",
        &["-m", "de", "-i", "500"],
    );
    assert!(
        stdout.contains("Status: infeasible"),
        "constraint-violating solution should be infeasible. stdout=\n{}",
        stdout
    );
    assert!(
        !stdout.contains("Status: optimal"),
        "must not claim optimal for an infeasible solution. stdout=\n{}",
        stdout
    );
}
