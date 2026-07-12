# Spec-Support Table & Unsupported-Construct Errors — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn silently-skipped unsupported constructs (`def`/`bellman`/`transition:`/`terminal`/`initial:`) into explicit default errors (with `--allow-unsupported` escape hatch), give undefined function calls a clear error, and ship a `docs/SPEC_SUPPORT.md` support table — closing Issue #3 criteria ①③④.

**Architecture:** Mirror the Issue #6 `--allow-missing-params` pattern: `parse()` records unsupported constructs into `Model.unsupported` (no signature change) and keeps skipping; `main.rs` decides error-vs-warning by flag. A localized expr-parser guard errors on `name(` function-call syntax. Regression tests use the `param_diagnostics.rs` binary-spawning harness.

**Tech Stack:** Rust (pure, no external solver deps). Build/test via `. "$HOME/.cargo/env" && cargo test`.

---

## Environment note

`cargo` is NOT on PATH in this env. Every cargo/rustc command MUST be prefixed in the same shell line:
`. "$HOME/.cargo/env" && cargo test`

## File Structure

- Modify `src/parser.rs` — add `Model.unsupported` field + init; add `unsupported_construct()` helper; record-and-skip in dispatch loop; unit test.
- Modify `src/expr.rs` — error on `identifier(` function-call for non-builtins.
- Modify `src/cli.rs` — `Args.allow_unsupported` field + `--allow-unsupported` parsing.
- Modify `src/main.rs` — post-parse unsupported diagnostic block; init `allow_unsupported` in REPL Args.
- Create `tests/unsupported_constructs.rs` — integration regression tests.
- Create `docs/SPEC_SUPPORT.md` — support table.
- Modify `README.md` — one link line to the support table.

---

### Task 1: `Model.unsupported` field + `unsupported_construct()` helper (parser)

**Files:**
- Modify: `src/parser.rs` (struct `Model` ~line 10-24; `Model::new()`; dispatch loop ~line 182-196)
- Test: inline `#[cfg(test)]` in `src/parser.rs`

- [ ] **Step 1: Add the field to the `Model` struct**

In `pub struct Model { ... }`, after `pub cp_globals: Vec<String>, ...`, add:

```rust
    /// 未対応（planned）構文を検出した行の説明（Issue #3）。
    /// 既定ではエラー、`--allow-unsupported` で警告してスキップ継続。
    pub unsupported: Vec<String>,
```

- [ ] **Step 2: Initialize it in `Model::new()`**

In `Self { ... }` inside `Model::new()`, after `cp_globals: Vec::new(),`, add:

```rust
            unsupported: Vec::new(),
```

- [ ] **Step 3: Add the `unsupported_construct()` helper**

Add a free function near the top-level `pub fn parse` (module scope, not inside `impl`):

```rust
/// 行が「未対応（planned）」構文で始まるなら人間可読な構文名を返す（Issue #3）。
/// `stage`/`state`/`decision` は set/var 登録される partial なのでここには含めない。
fn unsupported_construct(line: &str) -> Option<&'static str> {
    const TABLE: &[(&str, &str)] = &[
        ("def ", "def (user-defined functions)"),
        ("bellman ", "bellman (dynamic-programming recursion)"),
        ("transition:", "transition (DP state transition)"),
        ("terminal ", "terminal (DP terminal condition)"),
        ("initial:", "initial (DP initial condition)"),
    ];
    TABLE
        .iter()
        .find(|(p, _)| line.starts_with(p))
        .map(|(_, name)| *name)
}
```

- [ ] **Step 4: Add a unit test for the helper**

In the existing `#[cfg(test)] mod tests { ... }` in `src/parser.rs`, add:

```rust
    #[test]
    fn unsupported_construct_detects_planned_syntax() {
        assert_eq!(
            super::unsupported_construct("def total_cost(i) -> real:"),
            Some("def (user-defined functions)")
        );
        assert_eq!(
            super::unsupported_construct("bellman V[t]:"),
            Some("bellman (dynamic-programming recursion)")
        );
        assert_eq!(
            super::unsupported_construct("transition: S[t+1] = S[t]"),
            Some("transition (DP state transition)")
        );
        // 正当な構文は None
        assert_eq!(super::unsupported_construct("var x >= 0;"), None);
        assert_eq!(super::unsupported_construct("maximize obj: x;"), None);
        assert_eq!(super::unsupported_construct("stage t in 1..12;"), None);
    }
```

- [ ] **Step 5: Run the unit test — expect FAIL (helper compiles, but wire-up not done yet; this step verifies the helper + field compile)**

Run: `. "$HOME/.cargo/env" && cargo test --lib unsupported_construct_detects_planned_syntax -- --nocapture`
Expected: PASS (helper is pure; this confirms it compiles and behaves). If it fails to compile, fix field/helper syntax.

- [ ] **Step 6: Wire record-and-skip into the dispatch loop**

In `pub fn parse`, the dispatch loop begins `for stmt in &statements {  let line = stmt.trim();`. The current harmless-skip block is:

```rust
        // 空行・コメントをスキップ
        if line.is_empty()
            || line.starts_with('#')
            || line.starts_with("//")
            || line.starts_with("model ")
            || line.starts_with("problem ")
            || line.starts_with("transition:")
            || line.starts_with("def ")
            || line.starts_with("bellman ")
            || line.starts_with("terminal ")
            || line.starts_with("initial:")
            || line.starts_with("end")
            || line == "}"
        {
            continue;
        }
```

Replace it with (record unsupported first, then keep only genuinely-harmless skips):

```rust
        // 未対応（planned）構文は記録してスキップ。方針判断（エラー/警告）は main 側で
        // `--allow-unsupported` を見て行う（Issue #3, #6 と同じ流儀）。
        if let Some(name) = unsupported_construct(line) {
            model.unsupported.push(name.to_string());
            continue;
        }

        // 空行・コメント・無害なマーカーをスキップ
        if line.is_empty()
            || line.starts_with('#')
            || line.starts_with("//")
            || line.starts_with("model ")
            || line.starts_with("problem ")
            || line.starts_with("end")
            || line == "}"
        {
            continue;
        }
```

- [ ] **Step 7: Build to confirm parser compiles**

Run: `. "$HOME/.cargo/env" && cargo build`
Expected: compiles (warnings ok). `main.rs` does not yet read `model.unsupported` — that's Task 3.

- [ ] **Step 8: Commit**

```bash
git add src/parser.rs
git commit -m "feat(parser): record unsupported constructs into Model.unsupported (#3)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 2: Explicit error for undefined function calls (expr)

**Files:**
- Modify: `src/expr.rs` (the `_ =>` identifier branch, ~line 351, inside the primary/atom parser)
- Test: inline `#[cfg(test)]` in `src/expr.rs`

- [ ] **Step 1: Write the failing test**

In the `#[cfg(test)] mod tests` of `src/expr.rs` (near the existing `unknown_function_errors` test), add:

```rust
    #[test]
    fn user_defined_function_call_errors_clearly() {
        // `total_cost(i)` は組み込み関数でない識別子の直後に `(` が来る呼び出し構文。
        let err = crate::expr::parse_expr_str("total_cost(i)").unwrap_err();
        assert!(
            err.contains("unknown function") && err.contains("total_cost"),
            "should name the unknown function, got: {}",
            err
        );
    }
```

NOTE: the exact test entry point (`parse_expr_str` vs a `Parser::parse` helper) must match how `unknown_function_errors` already parses a string. Before writing, read that existing test and reuse its exact parse-entry call; adjust this test to the same API.

- [ ] **Step 2: Run to verify it fails**

Run: `. "$HOME/.cargo/env" && cargo test --lib user_defined_function_call_errors_clearly -- --nocapture`
Expected: FAIL — current behavior errors with a different message (e.g. trailing token / bad index / unknown symbol) or does not contain "unknown function".

- [ ] **Step 3: Add the guard in the `_ =>` identifier branch**

In `src/expr.rs`, the atom parser's fallback arm handles a bare identifier `id` with optional `[idx,...]`. At the very start of that `_ =>` arm — before the `if self.peek() == Some(&Tok::LBracket)` index handling — add:

```rust
                // 組み込み関数でない識別子の直後に `(` が来たらユーザー定義関数呼び出し。
                // 未対応（Issue #3）なので曖昧なシンボル解釈に落とさず明示エラーにする。
                if self.peek() == Some(&Tok::LPar) {
                    return Err(format!(
                        "unknown function '{}': user-defined functions are not supported \
                         (see docs/SPEC_SUPPORT.md)",
                        id
                    ));
                }
```

(`id` is the identifier string already bound in the match arm; confirm its exact binding name when reading the code and use it verbatim.)

- [ ] **Step 4: Run to verify it passes**

Run: `. "$HOME/.cargo/env" && cargo test --lib user_defined_function_call_errors_clearly -- --nocapture`
Expected: PASS

- [ ] **Step 5: Run full lib tests to ensure no regression (e.g. `x[i]` indexing still parses)**

Run: `. "$HOME/.cargo/env" && cargo test --lib`
Expected: all PASS.

- [ ] **Step 6: Commit**

```bash
git add src/expr.rs
git commit -m "fix(expr): explicit error on undefined function calls like total_cost(i) (#3)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 3: `--allow-unsupported` flag + main diagnostic

**Files:**
- Modify: `src/cli.rs` (`Args` struct; both `Args { .. }` constructions in `parse()`; option loop)
- Modify: `src/main.rs` (post-parse diagnostic; REPL `Args { .. }` construction ~line 305)

- [ ] **Step 1: Add the field to `Args`**

In `src/cli.rs`, in `pub struct Args`, after `pub allow_missing_params: bool,` add:

```rust
    /// 未対応（planned）構文を検出しても警告に留めてスキップ続行する（Issue #3）。
    pub allow_unsupported: bool,
```

- [ ] **Step 2: Initialize in the empty-args early return**

In `Args::parse`, the `if args.is_empty()` branch returns `Args { ... allow_missing_params: false, }`. Add after it:

```rust
                allow_unsupported: false,
```

- [ ] **Step 3: Add the local + option parsing + final struct**

After `let mut allow_missing_params = false;` add:

```rust
        let mut allow_unsupported = false;
```

In the option `match args[i].as_str()`, after `"--allow-missing-params" => allow_missing_params = true,` add:

```rust
                "--allow-unsupported" => allow_unsupported = true,
```

In the final `Ok(Args { ... allow_missing_params, })`, add:

```rust
            allow_unsupported,
```

- [ ] **Step 4: Initialize in the REPL Args construction in main.rs**

In `src/main.rs` around line 305 there is an `Args { ... allow_missing_params: false, }` used by the REPL. Add:

```rust
                    allow_unsupported: false,
```

- [ ] **Step 5: Add the diagnostic block in main.rs**

In `src/main.rs`, immediately AFTER the sidecar-JSON load block and BEFORE the `if model.dim == 0 {` check, insert:

```rust
    // 未対応構文診断（Issue #3）: def/bellman/transition/terminal/initial は現状スキップされる。
    // 既定ではエラーにし、--allow-unsupported でのみ警告に留めてスキップ続行する。
    if !model.unsupported.is_empty() {
        if args.allow_unsupported {
            eprintln!(
                "warning: unsupported constructs skipped: {}",
                model.unsupported.join(", ")
            );
        } else {
            eprintln!(
                "error: unsupported constructs used: {}",
                model.unsupported.join(", ")
            );
            eprintln!(
                "hint: these are planned but not yet implemented (see docs/SPEC_SUPPORT.md); \
                 pass --allow-unsupported to skip them"
            );
            std::process::exit(1);
        }
    }
```

- [ ] **Step 6: Build**

Run: `. "$HOME/.cargo/env" && cargo build`
Expected: compiles.

- [ ] **Step 7: Commit**

```bash
git add src/cli.rs src/main.rs
git commit -m "feat(cli): --allow-unsupported flag + default error on unsupported constructs (#3)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 4: Integration regression tests

**Files:**
- Create: `tests/unsupported_constructs.rs`

- [ ] **Step 1: Write the test file (copy the `run()` harness from `tests/param_diagnostics.rs`)**

```rust
//! Issue #3: 未対応（planned）構文は黙って無視せず既定でエラーにする。
//! `--allow-unsupported` を渡した場合のみ警告してスキップ続行する。

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// モデルを一意な一時ファイルに書き出して実バイナリを実行する。
/// argv は `prefix ++ [model_path] ++ suffix`。
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
    assert!(!ok, "def model must fail by default. stdout=\n{}\nstderr=\n{}", stdout, stderr);
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
    assert!(ok, "with --allow-unsupported, solve should proceed. stdout=\n{}\nstderr=\n{}", stdout, stderr);
    assert!(
        stderr.to_lowercase().contains("warning") && stderr.contains("def"),
        "should warn about the skipped construct. stderr=\n{}",
        stderr
    );
    assert!(stdout.contains("Objective:"), "should still solve. stdout=\n{}", stdout);
}

#[test]
fn dp_constructs_error_by_default() {
    for line in ["bellman V[t]:", "transition: S = S", "terminal cost;", "initial: S = 0"] {
        let model = format!("var x >= 0 <= 5;\nmaximize obj: x;\n{}\n", line);
        let (_stdout, stderr, ok) = run(&model, &[], &[]);
        assert!(!ok, "'{}' must fail by default. stderr=\n{}", line, stderr);
        assert!(
            stderr.contains("unsupported"),
            "'{}' should report an unsupported-construct error. stderr=\n{}",
            line, stderr
        );
    }
}

#[test]
fn unknown_function_call_errors() {
    // def なしで未定義関数を呼ぶ → 式パーサが明示エラー（parse error 経路）。
    let model = "var x >= 0 <= 10;\nmaximize obj: total_cost(x);\n";
    let (_stdout, stderr, ok) = run(model, &[], &[]);
    assert!(!ok, "undefined function call must fail. stderr=\n{}", stderr);
    assert!(
        stderr.contains("unknown function") && stderr.contains("total_cost"),
        "should give a clear unknown-function error. stderr=\n{}",
        stderr
    );
}

#[test]
fn valid_model_still_parses_and_solves() {
    // 正当な LP は未対応診断に引っかからず従来通り解ける（回帰）。
    let model = "var x >= 0 <= 4;\nvar y >= 0 <= 4;\nmaximize obj: 3*x + 2*y;\nsubject to c: x + y <= 4;\n";
    let (stdout, stderr, ok) = run(model, &[], &[]);
    assert!(ok, "valid model must still solve. stdout=\n{}\nstderr=\n{}", stdout, stderr);
    assert!(stdout.contains("Objective:"), "should report an objective. stdout=\n{}", stdout);
}
```

- [ ] **Step 2: Run the new integration tests**

Run: `. "$HOME/.cargo/env" && cargo test --test unsupported_constructs`
Expected: all 5 PASS. If `dp_constructs_error_by_default` fails because a bare `bellman V[t]:` line without variables triggers `no variables` first — the diagnostic block is placed before `dim==0`, so unsupported error wins; each model here also declares `var x` so `dim>0` anyway.

- [ ] **Step 3: Commit**

```bash
git add tests/unsupported_constructs.rs
git commit -m "test: regression tests for unsupported-construct errors (#3)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 5: `docs/SPEC_SUPPORT.md` support table + README link

**Files:**
- Create: `docs/SPEC_SUPPORT.md`
- Modify: `README.md` (add one link line near the existing unsupported-syntax note, ~line 127 or ~175)

- [ ] **Step 1: Write `docs/SPEC_SUPPORT.md`**

Content: a table with columns `構文 | 状態 | 備考`, covering: var/param/set declarations (supported), objective single/multi (supported; weighted_sum/epsilon supported), constraints & operators `<= >= =` (supported), `forall`/`sum` (supported), builtin funcs `min max abs sqrt exp log pow` (supported), `if..then..else` expr (supported), `stage`/`state`/`decision` (**partial** — parsed & registered, DP semantics not evaluated), `bellman`/`transition`/`terminal`/`initial` (**planned** — default error), `def` user-defined functions (**planned** — default error), stochastic `prob[]`/scenario, CP globals `no_overlap`/`cumulative`, `import`/`def` ML embedding (**planned/unsupported**). Add a short "examples の現状" list: which `examples/*.optica` run today vs are `[EXPERIMENTAL]`. State the `--allow-unsupported` escape hatch. Use the exact `supported/partial/planned/unsupported` vocabulary from Issue #3.

- [ ] **Step 2: Add the README link**

In `README.md`, near the existing prose about unsupported syntax, add one line:

```markdown
> 対応構文の一覧（supported / partial / planned / unsupported）は [docs/SPEC_SUPPORT.md](docs/SPEC_SUPPORT.md) を参照。
```

- [ ] **Step 3: Commit**

```bash
git add docs/SPEC_SUPPORT.md README.md
git commit -m "docs: add SPEC_SUPPORT.md support table and link from README (#3)

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>"
```

---

### Task 6: Full verification

- [ ] **Step 1: Run the entire test suite**

Run: `. "$HOME/.cargo/env" && cargo test`
Expected: all tests PASS (existing + new).

- [ ] **Step 2: Manual CLI verification**

```bash
. "$HOME/.cargo/env"
# default error on def
printf 'def f(i)->real:\n return 1;\nvar x>=0<=5;\nmaximize o: x;\n' > /tmp/def.optica
cargo run -q -- /tmp/def.optica ; echo "exit=$?"          # expect error + exit 1
cargo run -q -- /tmp/def.optica --allow-unsupported ; echo "exit=$?"   # expect warning + solve
```

- [ ] **Step 3: Update memory index if needed** (note PR/branch outcome in the Optica memory file per project convention).

## Self-Review

- **Spec coverage:** ① support table → Task 5. ③ unsupported errors → Tasks 1+2+3. ④ regression tests → Tasks 1(unit)+2(unit)+4(integration). Escape hatch `--allow-unsupported` → Task 3. Function-call clarity → Task 2. All spec sections covered. ② examples split intentionally out of scope (separate PR) per spec.
- **Placeholder scan:** Two "read the existing code and confirm the exact binding/API" notes remain in Task 2 (Steps 1 & 3) — these are verification instructions, not deferred work; the code to write is fully shown. Acceptable.
- **Type consistency:** `Model.unsupported: Vec<String>`, `unsupported_construct() -> Option<&'static str>`, `Args.allow_unsupported: bool` used consistently across Tasks 1/3/4. `run()` harness signature `(model, prefix, suffix)` used consistently in Task 4.
