use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

// 回帰テスト: `subject to NAME: <constraint>;` のようにブロックマーカーと同じ行に
// 制約本体が書かれた「インライン subject to」が正しくパースされ、適用されること。
// 修正前は `line.starts_with("subject to")` 分岐が `in_subject_to = true` を
// 立てるだけで、同じ行に続く制約本体を読み捨てていた（examples/knapsack.optica の
// capacity 制約が無視され、weight 合計が capacity=10 を超えて ~13.6 になっていた
// バグと同種）。同じく AtomicU64 で一時パスを一意化し、並列実行時の競合を避ける
// （tests/multiline.rs / tests/convergence.rs / tests/integrality.rs と同じ idiom）。
static CALL_COUNTER: AtomicU64 = AtomicU64::new(0);

fn run(src: &str) -> String {
    let n = CALL_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("optica_ist_{}_{}", std::process::id(), n));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("m.optica");
    std::fs::write(&path, src).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_optica"))
        .args([path.to_str().unwrap(), "-m", "de", "-i", "2000"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn obj(out: &str) -> f64 {
    out.lines()
        .find(|l| l.starts_with("Objective:"))
        .unwrap_or_else(|| panic!("no Objective line: {out}"))
        .split_whitespace()
        .last()
        .unwrap()
        .parse()
        .unwrap()
}

#[test]
fn inline_subject_to_constraint_is_enforced() {
    // x の上限は 10 だが、インライン制約 `cap: x <= 5;` が効いていれば最適は x=5, obj=5。
    // インライン制約が握り潰される（バグ再発）と ub=10 まで伸びて obj=10 になる。
    let src = "var x >= 0 <= 10;\nmaximize obj: x;\nsubject to cap: x <= 5;\n";
    let out = run(src);
    let o = obj(&out);
    assert!(
        o <= 5.0 + 1e-6,
        "inline 'subject to cap: x <= 5;' was not enforced (obj should be <= 5): {out}"
    );
    assert!(
        (o - 5.0).abs() < 1e-2,
        "constraint enforced but solver far from known optimum 5: {out}"
    );
}

#[test]
fn inline_subject_to_with_sum_is_enforced() {
    // examples/knapsack.optica と同型: `subject to NAME: sum{..} expr <= param;` の
    // インライン形式。capacity=10 を超える 2 アイテム（重み 6+5=11）を両方選ぶと
    // 違反になるはずなので、選択された重みの合計が capacity 以下でなければならない。
    let src = concat!(
        "set Items = {1, 2};\n",
        "param weight[Items] = {1: 6, 2: 5};\n",
        "param value[Items] = {1: 10, 2: 10};\n",
        "param capacity = 10;\n",
        "var x[Items] >= 0 <= 1;\n",
        "maximize profit: sum{i in Items} value[i] * x[i];\n",
        "subject to cap: sum{i in Items} weight[i] * x[i] <= capacity;\n",
    );
    let out = run(src);
    let mut total_weight = 0.0;
    let weights = [("x[1]", 6.0), ("x[2]", 5.0)];
    for line in out.lines().filter(|l| l.contains(" = ")) {
        let name = line.split('=').next().unwrap().trim();
        let v: f64 = line.split('=').nth(1).unwrap().trim().parse().unwrap();
        if let Some((_, w)) = weights.iter().find(|(n, _)| *n == name) {
            total_weight += w * v;
        }
    }
    assert!(
        total_weight <= 10.0 + 1e-3,
        "inline 'subject to cap: sum{{..}} <= capacity;' was not enforced, total weight {total_weight}: {out}"
    );
}
