use std::process::Command;

// Task 6: data 内蔵の golden 例に対する統合テスト。各例は解析的に確定した既知最適値を持ち、
// ソルバーは固定シード・単一スレッド DE のため決定的に同じ Objective を返す
// （examples/ 配下の静的ファイルを読むだけで書き込みは行わないため、tests/multiline.rs 等の
// AtomicU64 による一時パス一意化は不要）。
fn obj_of(path: &str) -> f64 {
    let out = Command::new(env!("CARGO_BIN_EXE_optica"))
        .args([path, "-m", "de", "-i", "2000"])
        .output()
        .unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    s.lines()
        .find(|l| l.starts_with("Objective:"))
        .unwrap_or("Objective: nan")
        .split_whitespace()
        .last()
        .unwrap()
        .parse()
        .unwrap_or(f64::NAN)
}

#[test]
fn knapsack_binary_optimum() {
    // 0/1 ナップサック: value={A:60,B:100,C:120}, weight={A:10,B:20,C:30}, capacity=50。
    // 既知最適 220（B+C, weight=50）。整数解なので厳密一致に近いはずタイトな許容誤差。
    let o = obj_of("examples/f1_knapsack_binary.optica");
    assert!((o - 220.0).abs() < 1e-6, "expected ~220, got {o}");
}

#[test]
fn lp_production_optimum() {
    // LP: profit={A:3,B:5}, usage={A:1,B:2}, cap=10, x∈[0,100]。
    // 利益/使用比は A=3, B=2.5 なので A を優先。x[A]=10, x[B]=0 → 30 が既知最適。
    let o = obj_of("examples/f1_lp_production.optica");
    assert!((o - 30.0).abs() < 1e-3, "expected ~30, got {o}");
}

#[test]
fn nlp_curve_optimum() {
    // minimize (y-2)^2 + 1, y∈[0,5] → 既知最適 y=2, obj=1。
    let o = obj_of("examples/f1_nlp_curve.optica");
    assert!((o - 1.0).abs() < 1e-3, "expected ~1, got {o}");
}

#[test]
fn existing_simple_knapsack_count() {
    // examples/simple_knapsack.optica: 4 アイテムから最大2つ選択してカウントを最大化。
    // 既知最適 2。
    let o = obj_of("examples/simple_knapsack.optica");
    assert!((o - 2.0).abs() < 1e-6, "expected ~2, got {o}");
}
