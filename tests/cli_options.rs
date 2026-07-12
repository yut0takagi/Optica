//! Issue #4: ファイル直指定形式 `optica <file> [OPTIONS]` でも `-q`/`-v` などの
//! オプションが有効になること（従来は `start_idx` が index 1 のオプションを読み飛ばしていた）。

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// argv = `prefix ++ [model_path] ++ suffix` で実行し、(stdout, stderr, success) を返す。
fn run(model: &str, prefix: &[&str], suffix: &[&str]) -> (String, String, bool) {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("optica_cliopt_{}_{}", std::process::id(), n));
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

const MODEL: &str = "var x >= 0 <= 10;\nminimize obj: (x - 3) ^ 2;\n";

#[test]
fn quiet_flag_honored_in_file_direct_form() {
    // `optica <file> -q` は `optica solve <file> -q` と同様に目的値のみを出力する。
    let (stdout, _stderr, ok) = run(MODEL, &[], &["-q"]);
    assert!(ok, "should solve. stdout=\n{}", stdout);
    assert!(
        !stdout.contains("Status:"),
        "-q in file-direct form should suppress the full report. stdout=\n{}",
        stdout
    );
    assert!(
        stdout.trim().parse::<f64>().is_ok(),
        "quiet output should be just the objective value. stdout=\n{}",
        stdout
    );
}

#[test]
fn verbose_flag_honored_in_file_direct_form() {
    // `optica <file> -v` は診断行 `[optica] ...` を stderr に出す。
    let (_stdout, stderr, ok) = run(MODEL, &[], &["-v"]);
    assert!(ok, "should solve. stderr=\n{}", stderr);
    assert!(
        stderr.contains("[optica]"),
        "-v in file-direct form should print the verbose diagnostic. stderr=\n{}",
        stderr
    );
}

#[test]
fn quiet_flag_still_works_in_solve_subcommand_form() {
    // 回帰ガード: `solve` サブコマンド形式の従来動作を壊さない。
    let (stdout, _stderr, ok) = run(MODEL, &["solve"], &["-q"]);
    assert!(ok, "should solve. stdout=\n{}", stdout);
    assert!(
        !stdout.contains("Status:") && stdout.trim().parse::<f64>().is_ok(),
        "-q should still work in subcommand form. stdout=\n{}",
        stdout
    );
}
