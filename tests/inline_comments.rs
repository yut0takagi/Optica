//! Issue #12: 行末インラインコメント（`#` または `//`）を、文字列リテラル外であれば
//! 安全に取り除く。従来は `#` が式パーサに渡り `unexpected char '#'` になっていた。

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn run(model: &str) -> (String, String, bool) {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("optica_comment_{}_{}", std::process::id(), n));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("m.optica");
    std::fs::write(&path, model).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_optica"))
        .args([path.to_str().unwrap()])
        .output()
        .unwrap();
    (
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
        out.status.success(),
    )
}

#[test]
fn hash_inline_comment_is_stripped() {
    let model = "var x >= 0 <= 10;  # decision variable\nminimize obj: (x - 3) ^ 2;  # quadratic\n";
    let (stdout, stderr, ok) = run(model);
    assert!(
        !stderr.contains("unexpected char"),
        "inline # comment must be stripped, not lexed. stderr=\n{}",
        stderr
    );
    assert!(ok, "should solve. stderr=\n{}", stderr);
    assert!(
        stdout.contains("Objective:"),
        "should report an objective. stdout=\n{}",
        stdout
    );
}

#[test]
fn double_slash_inline_comment_is_stripped() {
    let model =
        "var x >= 0 <= 10;  // decision variable\nminimize obj: (x - 3) ^ 2;  // quadratic\n";
    let (stdout, stderr, ok) = run(model);
    assert!(
        !stderr.contains("unexpected char") && !stderr.contains("parse error"),
        "inline // comment must be stripped. stderr=\n{}",
        stderr
    );
    assert!(ok, "should solve. stderr=\n{}", stderr);
    assert!(
        stdout.contains("Objective:"),
        "should report an objective. stdout=\n{}",
        stdout
    );
}
