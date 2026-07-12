use std::process::Command;

fn run(src: &str, args: &[&str]) -> String {
    let dir = std::env::temp_dir().join(format!("optica_test_{}", std::process::id()));
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

#[test]
fn single_line_objective_with_function() {
    // maximize sqrt(y) の代わりに、関数が評価に効くことを確認: minimize obj: (y-3)^2 は y->3 で最小
    let out = run(
        "var y >= 0 <= 10;\nminimize obj: (y - 3) ^ 2;\n",
        &["-m", "de"],
    );
    // 目的値が 0 付近、y ~ 3
    assert!(
        out.contains("y = 3.") || out.contains("y = 2.9") || out.contains("y = 3.0"),
        "got: {out}"
    );
}

#[test]
fn unknown_symbol_is_error() {
    let dir = std::env::temp_dir().join(format!("optica_unk_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("m.optica");
    std::fs::write(&path, "var y >= 0 <= 1;\nminimize obj: y + zzz;\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_optica"))
        .arg(path.to_str().unwrap())
        .output()
        .unwrap();
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        all.contains("unknown symbol") || !out.status.success(),
        "expected parse error, got: {all}"
    );
}

/// テスト用に一時 .optica ファイルへ書き出し、optica バイナリを実行して
/// (stdout+stderr 結合出力, 終了ステータス成功可否) を返す。
fn run_capture(name: &str, src: &str) -> (String, bool) {
    let dir = std::env::temp_dir().join(format!("optica_{}_{}", name, std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("m.optica");
    std::fs::write(&path, src).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_optica"))
        .arg(path.to_str().unwrap())
        .output()
        .unwrap();
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (all, out.status.success())
}

// ---- Fix 1 回帰テスト: サポート外/不正な比較演算子を持つ制約は黙って読み捨てられず
// 明示的なパースエラーになる（修正前は `subject to cap: x < 5;` が握り潰され、
// x が実質無制約のまま x=10 が "feasible" として報告されていた）。

#[test]
fn constraint_with_unsupported_operator_is_error() {
    // `<` は認識される演算子（<=, >=, ==）のいずれでもないため、修正前は
    // parse_constraint がこの制約を黙って読み捨て、x は無制約のまま x=10 になっていた。
    let (out, ok) = run_capture(
        "unsupported_op",
        "var x >= 0 <= 10;\nmaximize obj: x;\nsubject to cap: x < 5;\n",
    );
    assert!(
        !ok && (out.contains("no supported operator") || out.contains("parse error")),
        "expected parse error for unsupported operator '<', got: {out}"
    );
}

#[test]
fn constraint_with_chained_comparison_is_error() {
    // `0 <= x <= 5` は `<=` で3分割されるため parts.len() != 2 となり、
    // 修正前は黙って読み捨てられていた。
    let (out, ok) = run_capture(
        "chained_cmp",
        "var x >= 0 <= 10;\nmaximize obj: x;\nsubject to cap: 0 <= x <= 5;\n",
    );
    assert!(
        !ok && (out.contains("malformed") || out.contains("parse error")),
        "expected parse error for chained comparison '0 <= x <= 5', got: {out}"
    );
}

#[test]
fn constraint_with_not_equal_operator_is_error() {
    // `!=` も <=, >=, == のいずれでもないため、修正前は黙って読み捨てられていた。
    let (out, ok) = run_capture(
        "not_equal",
        "var x >= 0 <= 10;\nmaximize obj: x;\nsubject to cap: x != 0;\n",
    );
    assert!(
        !ok && (out.contains("no supported operator") || out.contains("parse error")),
        "expected parse error for '!=' operator, got: {out}"
    );
}

#[test]
fn valid_le_constraint_still_parses_and_is_enforced() {
    // Fix 1 が正当な演算子まで壊していないことの陽性コントロール。
    let (out, ok) = run_capture(
        "valid_le",
        "var x >= 0 <= 10;\nmaximize obj: x;\nsubject to cap: x <= 5;\n",
    );
    assert!(ok, "valid '<=' constraint should still parse: {out}");
}

// ---- Fix 2 回帰テスト: `sum{i in SET}` の SET 名が typo している場合、
// 修正前は未知シンボル検証をすり抜けて評価時に空集合 → sum=0 という
// サイレントな誤答（Objective: 0, Status: optimal）になっていた。

#[test]
fn sum_over_unknown_set_name_is_error() {
    let (out, ok) = run_capture(
        "sum_typo_set",
        concat!(
            "set Items = {1, 2};\n",
            "param value[Items] = {1: 10, 2: 20};\n",
            "var x[Items] >= 0 <= 1;\n",
            // "Items" の typo: "Itemz"
            "maximize profit: sum{i in Itemz} value[i] * x[i];\n",
        ),
    );
    assert!(
        !ok && out.contains("unknown symbol"),
        "expected parse error for typo'd set name 'Itemz' in sum header, got: {out}"
    );
}

#[test]
fn sum_over_valid_set_name_still_works() {
    // Fix 2 が正当な集合名まで壊していないことの陽性コントロール。
    let (out, ok) = run_capture(
        "sum_valid_set",
        concat!(
            "set Items = {1, 2};\n",
            "param value[Items] = {1: 10, 2: 20};\n",
            "var x[Items] >= 0 <= 1;\n",
            "maximize profit: sum{i in Items} value[i] * x[i];\n",
        ),
    );
    assert!(ok, "valid 'sum{{i in Items}}' should still parse: {out}");
}
