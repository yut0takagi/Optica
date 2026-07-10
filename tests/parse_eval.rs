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
