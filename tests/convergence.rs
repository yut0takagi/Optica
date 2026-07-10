use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

// NOTE(deviation from plan): the plan's `run()` helper keys the temp dir only on
// `std::process::id()`, which is identical across all threads of this test binary.
// Since `cargo test` runs tests in parallel by default, the two tests below (sharing
// this helper and the literal filename "m.optica") raced on the same file, causing
// intermittent failures (same pattern already fixed in tests/multiline.rs). Add a
// per-call counter to keep paths unique.
static CALL_COUNTER: AtomicU64 = AtomicU64::new(0);

fn run(src: &str) -> String {
    let n = CALL_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("optica_cv_{}_{}", std::process::id(), n));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("m.optica");
    std::fs::write(&path, src).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_optica"))
        .args([path.to_str().unwrap(), "-m", "de"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn obj(out: &str) -> f64 {
    out.lines()
        .find(|l| l.starts_with("Objective:"))
        .unwrap()
        .split_whitespace()
        .last()
        .unwrap()
        .parse()
        .unwrap()
}

#[test]
fn maximize_interior_optimum_converges() {
    // maximize 10 - (y-3)^2 → 最適 y=3, obj=10。早期終了バグがあると 10 に届かない。
    let out = run("var y >= 0 <= 10;\nmaximize obj: 10 - (y - 3) ^ 2;\n");
    assert!((obj(&out) - 10.0).abs() < 1e-3, "expected ~10, got: {out}");
    assert!(
        !out.contains("Iterations: 1\n"),
        "must not stop at iteration 1: {out}"
    );
}

#[test]
fn minimize_negative_optimum_converges() {
    // minimize (y-3)^2 - 100 → 最適 y=3, obj=-100。
    let out = run("var y >= 0 <= 10;\nminimize obj: (y - 3) ^ 2 - 100;\n");
    assert!(
        (obj(&out) - (-100.0)).abs() < 1e-3,
        "expected ~-100, got: {out}"
    );
}
