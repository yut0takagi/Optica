use std::path::{Path, PathBuf};
use std::process::Command;

// Issue #17: `examples/` 直下に置いた例は、すべて parse から solve まで通ることを保証する。
// 新しい例を足すときは、置き場所で扱いを選ぶ。
//   - `examples/` 直下: このテストの検証対象。壊れていれば CI が落ちる
//   - `examples/experimental/`: 将来仕様の例。未対応構文を含むので検証しない
// 既知最適値との照合は tests/golden.rs が受け持つ。ここでは「解けること」だけを見る。

fn top_level_examples() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|ext| ext == "optica"))
        .collect();
    files.sort();
    files
}

#[test]
fn top_level_examples_exist() {
    // ディレクトリの読み違いで対象が0件になり、下のテストが空回りで緑になるのを防ぐ。
    assert!(
        !top_level_examples().is_empty(),
        "examples/ 直下に .optica が1件も見つからない"
    );
}

#[test]
fn every_top_level_example_solves() {
    let mut failures = Vec::new();
    for path in top_level_examples() {
        let out = Command::new(env!("CARGO_BIN_EXE_optica"))
            .arg(&path)
            .output()
            .unwrap();
        let stdout = String::from_utf8_lossy(&out.stdout);
        let solved = out.status.success() && stdout.lines().any(|l| l.starts_with("Objective:"));
        if !solved {
            let stderr = String::from_utf8_lossy(&out.stderr);
            failures.push(format!(
                "{} (status: {})\n{}",
                path.display(),
                out.status,
                stderr.trim()
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "examples/ 直下の例が解けなかった。将来仕様の例なら examples/experimental/ へ移すこと:\n\n{}",
        failures.join("\n\n")
    );
}
