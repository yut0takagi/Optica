//! Issue #9: 添字算術 `t-1` / `t+1` が forall 展開込みで正しく解決されることの回帰テスト。

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn run(model: &str, args: &[&str]) -> (String, String, bool) {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("optica_subscript_{}_{}", std::process::id(), n));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("m.optica");
    std::fs::write(&path, model).unwrap();

    let mut argv: Vec<String> = vec![path.to_str().unwrap().to_string()];
    for s in args {
        argv.push(s.to_string());
    }
    let out = Command::new(env!("CARGO_BIN_EXE_optica"))
        .args(&argv)
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

/// `forall t in 2..3: x[t] >= x[t-1]` が展開され、`x[t-1]` が具体添字に解決されて解ける。
#[test]
fn forall_with_subscript_offset_solves() {
    let model = "set T = 1..3;\n\
                 var x[T] >= 0 <= 10;\n\
                 maximize obj: sum{t in T} x[t];\n\
                 subject to cap: sum{t in T} x[t] <= 12;\n\
                 subject to mono: forall t in 2..3: x[t] >= x[t-1];\n";
    let (stdout, stderr, ok) = run(model, &["-i", "800"]);
    assert!(
        ok,
        "subscript-arithmetic model must solve. stdout=\n{}\nstderr=\n{}",
        stdout, stderr
    );
    assert!(
        !stderr.contains("unknown")
            && !stderr.contains("bad index")
            && !stderr.contains("subscript"),
        "must not raise index/symbol errors. stderr=\n{}",
        stderr
    );
    // cap を上限 12 まで使えるので最適値は 12 付近。
    let obj = objective_of(&stdout);
    assert!(
        obj > 11.0,
        "should push objective toward the cap of 12, got {}. stdout=\n{}",
        obj,
        stdout
    );
}
