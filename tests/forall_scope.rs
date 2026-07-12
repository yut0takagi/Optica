use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

// 回帰テスト: `subject to` コンテキスト外に置かれた top-level の bare `forall` は
// サイレントに読み捨てられてはならず、明示的なパースエラーになること。
// 修正前は dispatch のどの分岐にもマッチせず落ち、制約が一切適用されないまま
// 「無制約の最適値」を feasible として報告していた（silent-wrong-answer）。
// 一方、正当な2形式（`subject to:` 直下の nested forall / インライン
// `subject to cap: forall ...`）は引き続きパース・適用されなければならない。
// 一時パスは AtomicU64 で一意化し、並列実行時の同名ファイル競合を避ける
// （tests/multiline.rs / tests/golden 以外の同 idiom と同じ）。
static CALL_COUNTER: AtomicU64 = AtomicU64::new(0);

struct Run {
    stdout: String,
    stderr: String,
    ok: bool,
}

fn run(src: &str) -> Run {
    let n = CALL_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("optica_fa_{}_{}", std::process::id(), n));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("m.optica");
    std::fs::write(&path, src).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_optica"))
        .args([path.to_str().unwrap(), "-m", "de", "-i", "2000"])
        .output()
        .unwrap();
    Run {
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        ok: out.status.success(),
    }
}

fn obj(stdout: &str) -> f64 {
    stdout
        .lines()
        .find(|l| l.starts_with("Objective:"))
        .unwrap_or_else(|| panic!("no Objective line: {stdout}"))
        .split_whitespace()
        .last()
        .unwrap()
        .parse()
        .unwrap()
}

#[test]
fn top_level_bare_forall_is_a_parse_error() {
    // subject to の外にある bare forall。修正前は無視され obj=15（無制約）を
    // feasible として返していた。修正後は非ゼロ終了かつ stderr に "forall"。
    let src = "set Items = {1, 2, 3};\n\
               var x[Items] >= 0 <= 5;\n\
               maximize obj: sum{i in Items} x[i];\n\
               forall i in Items:\n\
               \x20 x[i] <= 2;\n";
    let r = run(src);
    assert!(
        !r.ok,
        "top-level bare forall must fail (nonzero exit). stdout: {}",
        r.stdout
    );
    assert!(
        r.stderr.contains("forall"),
        "parse error should mention 'forall'. stderr: {}",
        r.stderr
    );
}

#[test]
fn block_forall_inside_subject_to_is_enforced() {
    // 正当なブロック形式: subject to: 直下に nested forall。制約が効けば obj=6。
    // （効かないと ub=5 まで伸びて obj=15）。
    let src = "set Items = {1, 2, 3};\n\
               var x[Items] >= 0 <= 5;\n\
               maximize obj: sum{i in Items} x[i];\n\
               subject to:\n\
               \x20   cap:\n\
               \x20       forall i in Items:\n\
               \x20           x[i] <= 2\n";
    let r = run(src);
    assert!(r.ok, "valid block forall must parse. stderr: {}", r.stderr);
    assert!(
        (obj(&r.stdout) - 6.0).abs() < 1e-2,
        "block forall constraint not enforced (expected ~6): {}",
        r.stdout
    );
}

#[test]
fn inline_subject_to_forall_is_enforced() {
    // 正当なインライン形式: subject to cap: forall ...。制約が効けば obj=6。
    let src = "set Items = {1, 2, 3};\n\
               var x[Items] >= 0 <= 5;\n\
               maximize obj: sum{i in Items} x[i];\n\
               subject to cap: forall i in Items: x[i] <= 2;\n";
    let r = run(src);
    assert!(r.ok, "valid inline forall must parse. stderr: {}", r.stderr);
    assert!(
        (obj(&r.stdout) - 6.0).abs() < 1e-2,
        "inline forall constraint not enforced (expected ~6): {}",
        r.stdout
    );
}
