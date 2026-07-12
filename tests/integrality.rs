use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

fn run(path: &str) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_optica"))
        .args([path, "-m", "de"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).to_string()
}

// 一時モデルを書き出して解く。cargo test は既定で並列実行するため、
// tests/multiline.rs / tests/convergence.rs と同じく AtomicU64 でパスを一意化し、
// 同名ファイルへの競合を避ける。
static CALL_COUNTER: AtomicU64 = AtomicU64::new(0);

fn run_src(src: &str) -> String {
    let n = CALL_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("optica_int_{}_{}", std::process::id(), n));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("m.optica");
    std::fs::write(&path, src).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_optica"))
        .args([path.to_str().unwrap(), "-m", "de", "-i", "2000"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).to_string()
}

#[test]
fn binary_vars_are_integral() {
    // simple_knapsack: var x[ITEMS] binary; maximize count sum x[i]; sum<=2 → 2 個が 1、他 0
    let out = run("examples/simple_knapsack.optica");
    // 印字される各変数は 0 か 1 のみ（小数点以下が 0）
    for line in out.lines().filter(|l| l.contains(" = ")) {
        let v: f64 = line.split('=').nth(1).unwrap().trim().parse().unwrap();
        assert!(
            (v - v.round()).abs() < 1e-9,
            "non-integral binary var: {line}"
        );
    }
    assert!(
        out.contains("Objective: 2") || out.contains("2.000000e0"),
        "count should be 2: {out}"
    );
}

#[test]
fn continuous_var_named_int_stays_continuous() {
    // 回帰: 変数名に "int" を含む連続変数（例 point）が、部分文字列一致で
    // 誤って整数扱いされ丸められてはならない。最適 point=3.7。
    // 部分文字列一致のバグがあると 3 か 4 に丸められる。
    let out = run_src("var point >= 0 <= 10;\nminimize obj: (point - 3.7) ^ 2;\n");
    let v: f64 = out
        .lines()
        .find(|l| l.contains(" = "))
        .and_then(|l| l.split('=').nth(1))
        .unwrap_or_else(|| panic!("no variable line printed: {out}"))
        .trim()
        .parse()
        .unwrap();
    assert!(
        (v - 3.7).abs() < 0.15,
        "continuous var 'point' was wrongly rounded (should stay ~3.7): {out}"
    );
}
