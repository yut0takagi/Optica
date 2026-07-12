//! Issue #3: 未対応（planned）構文は黙って無視せず既定でエラーにする。
//! `--allow-unsupported` を渡した場合のみ警告してスキップ続行する。

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// モデルを一意な一時ファイルに書き出して実バイナリを実行する。
/// argv は `prefix ++ [model_path] ++ suffix`。`solve` サブコマンド形式にしたい場合は
/// prefix=["solve"] を渡す（ファイル直指定形式でのオプション読み飛ばし #4 制約の回避）。
fn run(model: &str, prefix: &[&str], suffix: &[&str]) -> (String, String, bool) {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("optica_unsup_{}_{}", std::process::id(), n));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("m.optica");
    std::fs::write(&path, model).unwrap();

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

/// def を含み、その関数を参照しない最小モデル（未対応診断を単独で観測するため）。
fn model_with_def() -> String {
    "def helper(i) -> real:\n    return 1;\nvar x >= 0 <= 10;\nmaximize obj: x;\n".to_string()
}

#[test]
fn def_errors_by_default() {
    let (stdout, stderr, ok) = run(&model_with_def(), &[], &[]);
    assert!(
        !ok,
        "def model must fail by default. stdout=\n{}\nstderr=\n{}",
        stdout, stderr
    );
    assert!(
        stderr.contains("def") && stderr.contains("SPEC_SUPPORT.md"),
        "error should name the construct and point to the support doc. stderr=\n{}",
        stderr
    );
}

#[test]
fn unsupported_allowed_with_flag() {
    // フラグを確実に読ませるため solve サブコマンド形式（#4 の CLI 制約回避）。
    let (stdout, stderr, ok) = run(&model_with_def(), &["solve"], &["--allow-unsupported"]);
    assert!(
        ok,
        "with --allow-unsupported, solve should proceed. stdout=\n{}\nstderr=\n{}",
        stdout, stderr
    );
    assert!(
        stderr.to_lowercase().contains("warning") && stderr.contains("def"),
        "should warn about the skipped construct. stderr=\n{}",
        stderr
    );
    assert!(
        stdout.contains("Objective:"),
        "should still solve. stdout=\n{}",
        stdout
    );
}

#[test]
fn dp_constructs_error_by_default() {
    for line in [
        "bellman V[t]:",
        "transition: S = S",
        "terminal cost;",
        "initial: S = 0",
    ] {
        let model = format!("var x >= 0 <= 5;\nmaximize obj: x;\n{}\n", line);
        let (_stdout, stderr, ok) = run(&model, &[], &[]);
        assert!(!ok, "'{}' must fail by default. stderr=\n{}", line, stderr);
        assert!(
            stderr.contains("unsupported"),
            "'{}' should report an unsupported-construct error. stderr=\n{}",
            line,
            stderr
        );
    }
}

#[test]
fn unknown_function_call_errors() {
    // def なしで未定義関数を呼ぶ → 式パーサが明示エラー（parse error 経路）。
    let model = "var x >= 0 <= 10;\nmaximize obj: total_cost(x);\n";
    let (_stdout, stderr, ok) = run(model, &[], &[]);
    assert!(
        !ok,
        "undefined function call must fail. stderr=\n{}",
        stderr
    );
    assert!(
        stderr.contains("unknown function") && stderr.contains("total_cost"),
        "should give a clear unknown-function error. stderr=\n{}",
        stderr
    );
}

#[test]
fn valid_model_still_parses_and_solves() {
    // 正当な LP は未対応診断に引っかからず従来通り解ける（回帰）。
    let model =
        "var x >= 0 <= 4;\nvar y >= 0 <= 4;\nmaximize obj: 3*x + 2*y;\nsubject to c: x + y <= 4;\n";
    let (stdout, stderr, ok) = run(model, &[], &[]);
    assert!(
        ok,
        "valid model must still solve. stdout=\n{}\nstderr=\n{}",
        stdout, stderr
    );
    assert!(
        stdout.contains("Objective:"),
        "should report an objective. stdout=\n{}",
        stdout
    );
}
