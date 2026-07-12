//! Issue #5 / #6: パラメータの充足性に関する診断。
//!
//! #5: 値なしスカラー宣言 `param alpha real;` は既知シンボルとして登録され、
//!     JSON サイドカー等で補完すれば `unknown symbol` にならずに解ける。
//! #6: 宣言だけされ値の無いパラメータを参照するモデルは、暗黙 0 評価による偽の
//!     `Objective: 0` を避けるため既定でエラーにする。`--allow-missing-params`
//!     を渡した場合のみ警告を出して 0 として続行する。

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// モデル（と任意のサイドカー JSON）を一意な一時ディレクトリに書き出して実行する。
/// argv は `prefix ++ [model_path] ++ suffix`。`solve` サブコマンド形式にしたい場合は
/// prefix=["solve"] を渡す（ファイル直指定形式ではオプションが読み飛ばされる #4 の制約を回避）。
fn run(
    model: &str,
    json: Option<&str>,
    prefix: &[&str],
    suffix: &[&str],
) -> (String, String, bool) {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("optica_paramdiag_{}_{}", std::process::id(), n));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("m.optica");
    std::fs::write(&path, model).unwrap();
    if let Some(j) = json {
        std::fs::write(dir.join("m.json"), j).unwrap();
    }

    let mut argv: Vec<String> = prefix.iter().map(|s| s.to_string()).collect();
    argv.push(path.to_str().unwrap().to_string());
    for s in suffix {
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

// ---- Issue #5 ----

#[test]
fn scalar_param_without_value_is_known_symbol_and_solves_when_filled() {
    // `param alpha real;` は値なしスカラー宣言。JSON サイドカーで補完する。
    let model = "param alpha real;\nvar x >= 0 <= 10;\nminimize obj: (x - alpha) ^ 2;\n";
    let (stdout, stderr, ok) = run(model, Some("{\"alpha\": 3}"), &[], &[]);

    assert!(
        !stderr.contains("unknown symbol"),
        "value-less scalar param must be a known symbol. stderr=\n{}",
        stderr
    );
    assert!(ok, "should solve once alpha is filled. stderr=\n{}", stderr);
    let obj = objective_of(&stdout);
    assert!(
        obj < 1e-3,
        "min (x-alpha)^2 with alpha=3 should reach ~0, got {}. stdout=\n{}",
        obj,
        stdout
    );
}

// ---- Issue #6 ----

#[test]
fn unset_param_errors_by_default() {
    // profit[P] は宣言のみで値なし。参照すると各要素が暗黙 0 になり Objective 0 の偽陽性。
    let model = "set P = {A, B};\nparam profit[P] real;\nvar x[P] >= 0 <= 1;\nmaximize total: sum{i in P} profit[i] * x[i];\nsubject to lim: sum{i in P} x[i] <= 1;\n";
    let (stdout, stderr, ok) = run(model, None, &[], &[]);

    assert!(
        !ok,
        "unset param model must fail by default. stdout=\n{}\nstderr=\n{}",
        stdout, stderr
    );
    assert!(
        stderr.contains("profit"),
        "diagnostic should name the unset param. stderr=\n{}",
        stderr
    );
    assert!(
        !stdout.contains("Objective:"),
        "must not print a bogus objective for an unset-param model. stdout=\n{}",
        stdout
    );
}

#[test]
fn unset_param_allowed_as_zero_with_flag() {
    // `--allow-missing-params` を渡すと警告を出して 0 として続行する。
    // フラグを確実に読ませるため solve サブコマンド形式を使う（#4 の CLI 制約回避）。
    let model = "set P = {A, B};\nparam profit[P] real;\nvar x[P] >= 0 <= 1;\nmaximize total: sum{i in P} profit[i] * x[i];\nsubject to lim: sum{i in P} x[i] <= 1;\n";
    let (stdout, stderr, ok) = run(model, None, &["solve"], &["--allow-missing-params"]);

    assert!(
        ok,
        "with the flag, solve should proceed. stdout=\n{}\nstderr=\n{}",
        stdout, stderr
    );
    assert!(
        stderr.to_lowercase().contains("warning") && stderr.contains("profit"),
        "should warn about the unset param. stderr=\n{}",
        stderr
    );
    assert!(
        stdout.contains("Objective:"),
        "should still report an objective. stdout=\n{}",
        stdout
    );
}
