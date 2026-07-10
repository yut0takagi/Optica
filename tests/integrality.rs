use std::process::Command;

fn run(path: &str) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_optica"))
        .args([path, "-m", "de"])
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
