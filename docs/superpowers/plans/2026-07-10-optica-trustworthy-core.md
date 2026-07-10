# Optica Fase1: Trustworthy Core 実装計画

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Optica のコア（式評価器・パーサ・ソルバー）を修正し、厳選した data 内蔵例が既知最適を返すことを golden テストで保証する。

**Architecture:** 文字列を毎回再解析する式評価器を、パース時に1度だけ AST 化して評価する新モジュール `src/expr.rs`（Pratt パーサ）に置換。パーサに複数行 `maximize:`/`subject to:` ブロックと `forall` 制約展開を追加。ソルバーの早期収束バグ（`best_fit < TOLERANCE`）を撤去し、`binary/int` 変数は丸め込み修復で整数化する。

**Tech Stack:** Rust 2021 / cargo（rustup stable, `rustc 1.97`）/ 依存は `serde_json` のみ / テストは `#[cfg(test)]` ＋ `tests/` 統合テスト。

**検証環境:** rustup 導入済み。各ステップは `. "$HOME/.cargo/env"` を通してから `cargo` を実行する（PATH 未設定のため）。

---

## 参照（現状コードの要点）

- `src/parser.rs`
  - `struct Model { dim, lb, ub, var_names, var_map, maximize, params, sets, objective_expr: Option<String>, constraints: Vec<Constraint>, objectives: Vec<Objective>, pareto, cp_globals }`
  - `struct Constraint { name, expr: String, op: ConstraintOp, rhs: f64 }`（RHS が数値/スカラーのみ＝要改良）
  - `struct Objective { name, expr: String, maximize }`
  - `enum ConstraintOp { Le, Ge, Eq }`
  - Model メソッド: `evaluate_objective`, `check_constraints`, `evaluate_expr`, `eval_if`, `eval_condition`, `eval_comparison`, `eval_arith`, `eval_symbol`, `evaluate_sum`（＝旧文字列評価器。Task 2 で AST 経由に置換・削除）
  - `parse()` は行ベース。`maximize`/`minimize` は1行前提（複数行を落とす）。
- `src/solver/mod.rs`
  - `compute_fitness(model, x)` が `model.evaluate_expr(...)` / `evaluate_objective` / `check_constraints` を呼ぶ。
  - 早期収束: `if best_fit < TOLERANCE { return ... }`（`de_single` 内、`pso` 内の2箇所）。
- `src/config.rs`: `TOLERANCE = 1e-10` ほか定数。

---

## Task 0: リポジトリ衛生（stale バイナリ撤去）

**Files:**
- Delete: `optica`（Git 追跡された stale バイナリ）
- Create: `.gitignore`

- [ ] **Step 1: 追跡バイナリと新規ビルドが別物であることを記録・削除**

```bash
cd /Users/s32747/Develop/Optica
. "$HOME/.cargo/env"
shasum optica target/release/optica 2>/dev/null || true   # 参考: ハッシュ不一致を確認
git rm --cached optica
rm -f optica
```

- [ ] **Step 2: `.gitignore` を作成**

`.gitignore`:
```gitignore
/target
/optica
*.rs.bk
```

- [ ] **Step 3: ビルドが緑のままか確認**

Run: `. "$HOME/.cargo/env" && cargo build`
Expected: `Finished` （エラーなし）

- [ ] **Step 4: Commit**

```bash
git add .gitignore
git commit -m "chore: remove stale tracked binary and add .gitignore"
```

---

## Task 1: AST 式評価器モジュール `src/expr.rs`（standalone）

現行の壊れた評価（`min` 誤り、`abs/sqrt/exp/log`/`^` 未対応）を、Pratt パーサ＋AST で正しく評価する独立モジュールを追加する。この時点では Model へ配線せず、モジュール内ユニットテストのみで完結させる。

**Files:**
- Create: `src/expr.rs`
- Modify: `src/main.rs`（`mod expr;` を追加）

**設計（この型・関数名は後続タスクでもそのまま使用する）:**
- `pub enum Expr { Num(f64), Sym{name:String, idx:Vec<String>}, Neg(Box<Expr>), Bin(Op,Box<Expr>,Box<Expr>), Func(Func,Vec<Expr>), Sum(Vec<(String,SetRef)>,Box<Expr>), If(Box<Cond>,Box<Expr>,Box<Expr>) }`
- `pub enum Op { Add, Sub, Mul, Div, Pow }`
- `pub enum Cmp { Lt, Le, Gt, Ge, Eq, Ne }`
- `pub enum Func { Min, Max, Abs, Sqrt, Exp, Log, Pow }`
- `pub enum SetRef { Named(String), Range(i64,i64) }`
- `pub struct Cond { lhs:Expr, cmp:Cmp, rhs:Expr }`
- `pub struct Ctx<'a> { var_map:&'a HashMap<String,usize>, params:&'a HashMap<String,HashMap<String,f64>>, sets:&'a HashMap<String,Vec<String>> }`
- `pub fn compile(src:&str) -> Result<Expr,String>`（構文のみ。未知関数名はエラー）
- `pub fn eval(e:&Expr, x:&[f64], env:&HashMap<String,String>, ctx:&Ctx) -> f64`
- バインディングパワー: `+ -`=10, `* /`=20, `^`=30（右結合）, 単項`-`=25。`sum` 本体は min_bp=10 でパース（`+/-` の手前で停止）。

- [ ] **Step 1: 失敗するユニットテストを書く**

`src/expr.rs`（末尾に追加。まだ本体が無いのでコンパイルは通らない＝失敗を期待）:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn ev(src: &str) -> f64 {
        let e = compile(src).expect("compile");
        let vm = HashMap::new();
        let params = HashMap::new();
        let sets = HashMap::new();
        let env = HashMap::new();
        let ctx = Ctx { var_map: &vm, params: &params, sets: &sets };
        eval(&e, &[], &env, &ctx)
    }

    #[test]
    fn arithmetic_and_precedence() {
        assert_eq!(ev("5"), 5.0);
        assert_eq!(ev("2 * 3"), 6.0);
        assert_eq!(ev("10 - 2 * 3"), 4.0);
        assert_eq!(ev("(2 + 3) * 2"), 10.0);
        assert_eq!(ev("2 * (3 + 4)"), 14.0);
        assert_eq!(ev("0 - 4"), -4.0);
        assert_eq!(ev("2 ^ 3"), 8.0);
        assert_eq!(ev("3 ^ 2"), 9.0);
    }

    #[test]
    fn functions() {
        assert_eq!(ev("max(2, 7)"), 7.0);
        assert_eq!(ev("min(2, 7)"), 2.0);
        assert_eq!(ev("abs(0 - 4)"), 4.0);
        assert_eq!(ev("sqrt(9)"), 3.0);
        assert_eq!(ev("exp(0)"), 1.0);
        assert_eq!(ev("log(1)"), 0.0);
        assert_eq!(ev("3 + max(2, 7)"), 10.0);
    }

    #[test]
    fn conditional() {
        assert_eq!(ev("if 1 < 2 then 10 else 20"), 10.0);
        assert_eq!(ev("if 3 < 2 then 10 else 20"), 20.0);
    }

    #[test]
    fn unknown_function_errors() {
        assert!(compile("frobnicate(1)").is_err());
    }

    #[test]
    fn sum_over_set_with_params_and_vars() {
        // objective: sum{i in S} p[i]*x[i], S={1,2,3}, p={10,40,30}, x=[1,0,1]
        let mut sets = HashMap::new();
        sets.insert("S".to_string(), vec!["1".into(), "2".into(), "3".into()]);
        let mut params = HashMap::new();
        let mut p = HashMap::new();
        p.insert("1".into(), 10.0); p.insert("2".into(), 40.0); p.insert("3".into(), 30.0);
        params.insert("p".to_string(), p);
        let mut vm = HashMap::new();
        vm.insert("x[1]".to_string(), 0usize);
        vm.insert("x[2]".to_string(), 1usize);
        vm.insert("x[3]".to_string(), 2usize);
        let x = [1.0, 0.0, 1.0];
        let env = HashMap::new();
        let ctx = Ctx { var_map: &vm, params: &params, sets: &sets };
        let e = compile("sum{i in S} p[i] * x[i]").unwrap();
        assert_eq!(eval(&e, &x, &env, &ctx), 40.0); // 10*1 + 40*0 + 30*1
    }

    #[test]
    fn sum_added_terms_split_correctly() {
        // sum{i in S} x[i] + sum{i in S} x[i] == 2 * sum
        let mut sets = HashMap::new();
        sets.insert("S".to_string(), vec!["1".into(), "2".into()]);
        let mut vm = HashMap::new();
        vm.insert("x[1]".to_string(), 0usize);
        vm.insert("x[2]".to_string(), 1usize);
        let x = [1.0, 1.0];
        let env = HashMap::new();
        let params = HashMap::new();
        let ctx = Ctx { var_map: &vm, params: &params, sets: &sets };
        let e = compile("sum(i in S) x[i] + sum(i in S) x[i]").unwrap();
        assert_eq!(eval(&e, &x, &env, &ctx), 4.0);
    }
}
```

- [ ] **Step 2: テストが失敗（コンパイル不能）することを確認**

Run: `. "$HOME/.cargo/env" && cargo test --lib expr 2>&1 | tail -20`
Expected: コンパイルエラー（`compile`/`eval`/`Ctx` 未定義）

- [ ] **Step 3: `src/expr.rs` の本体を実装**

`src/expr.rs`（テストモジュールの前に配置する完全な本体）:
```rust
//! AST ベースの式評価器（Pratt パーサ）。
//! 文字列を1度だけ Expr にコンパイルし、以後は再帰評価する。

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Op { Add, Sub, Mul, Div, Pow }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cmp { Lt, Le, Gt, Ge, Eq, Ne }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Func { Min, Max, Abs, Sqrt, Exp, Log, Pow }

#[derive(Debug, Clone)]
pub enum SetRef { Named(String), Range(i64, i64) }

#[derive(Debug, Clone)]
pub enum Expr {
    Num(f64),
    Sym { name: String, idx: Vec<String> },
    Neg(Box<Expr>),
    Bin(Op, Box<Expr>, Box<Expr>),
    Func(Func, Vec<Expr>),
    Sum(Vec<(String, SetRef)>, Box<Expr>),
    If(Box<Cond>, Box<Expr>, Box<Expr>),
}

#[derive(Debug, Clone)]
pub struct Cond { pub lhs: Expr, pub cmp: Cmp, pub rhs: Expr }

pub struct Ctx<'a> {
    pub var_map: &'a HashMap<String, usize>,
    pub params: &'a HashMap<String, HashMap<String, f64>>,
    pub sets: &'a HashMap<String, Vec<String>>,
}

// ---- Lexer ----
#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Ident(String),
    LPar, RPar, LBrace, RBrace, LBracket, RBracket, Comma,
    Plus, Minus, Star, Slash, Caret,
    Lt, Le, Gt, Ge, EqEq, Ne,
}

fn lex(src: &str) -> Result<Vec<Tok>, String> {
    let b = src.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < b.len() {
        let c = b[i] as char;
        if c.is_whitespace() { i += 1; continue; }
        match c {
            '(' => { out.push(Tok::LPar); i += 1; }
            ')' => { out.push(Tok::RPar); i += 1; }
            '{' => { out.push(Tok::LBrace); i += 1; }
            '}' => { out.push(Tok::RBrace); i += 1; }
            '[' => { out.push(Tok::LBracket); i += 1; }
            ']' => { out.push(Tok::RBracket); i += 1; }
            ',' => { out.push(Tok::Comma); i += 1; }
            '+' => { out.push(Tok::Plus); i += 1; }
            '-' => { out.push(Tok::Minus); i += 1; }
            '*' => { out.push(Tok::Star); i += 1; }
            '/' => { out.push(Tok::Slash); i += 1; }
            '^' => { out.push(Tok::Caret); i += 1; }
            '<' => { if i + 1 < b.len() && b[i+1] == b'=' { out.push(Tok::Le); i += 2; } else { out.push(Tok::Lt); i += 1; } }
            '>' => { if i + 1 < b.len() && b[i+1] == b'=' { out.push(Tok::Ge); i += 2; } else { out.push(Tok::Gt); i += 1; } }
            '=' => { if i + 1 < b.len() && b[i+1] == b'=' { out.push(Tok::EqEq); i += 2; } else { return Err("unexpected '='".into()); } }
            '!' => { if i + 1 < b.len() && b[i+1] == b'=' { out.push(Tok::Ne); i += 2; } else { return Err("unexpected '!'".into()); } }
            _ if c.is_ascii_digit() => {
                let s = i;
                while i < b.len() && (b[i] as char).is_ascii_digit() { i += 1; }
                // 小数点は「. の直後が数字」のときだけ消費（範囲 ".." を食わない）
                if i + 1 < b.len() && b[i] == b'.' && (b[i + 1] as char).is_ascii_digit() {
                    i += 1;
                    while i < b.len() && (b[i] as char).is_ascii_digit() { i += 1; }
                }
                let n = src[s..i].parse::<f64>().map_err(|e| e.to_string())?;
                out.push(Tok::Num(n));
            }
            _ if c.is_ascii_alphabetic() || c == '_' => {
                let s = i;
                i += 1;
                while i < b.len() {
                    let ch = b[i] as char;
                    if ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' { i += 1; } else { break; }
                }
                out.push(Tok::Ident(src[s..i].to_string()));
            }
            other => return Err(format!("unexpected char '{}'", other)),
        }
    }
    Ok(out)
}

// ---- Parser (Pratt) ----
struct P { t: Vec<Tok>, i: usize }

impl P {
    fn peek(&self) -> Option<&Tok> { self.t.get(self.i) }
    fn next(&mut self) -> Option<Tok> { let v = self.t.get(self.i).cloned(); if v.is_some() { self.i += 1; } v }
    fn eat(&mut self, t: &Tok) -> Result<(), String> {
        if self.peek() == Some(t) { self.i += 1; Ok(()) } else { Err(format!("expected {:?}", t)) }
    }

    fn parse_expr(&mut self, min_bp: u8) -> Result<Expr, String> {
        let mut lhs = self.parse_prefix()?;
        loop {
            let (op, lbp, rbp) = match self.peek() {
                Some(Tok::Plus) => (Op::Add, 10, 11),
                Some(Tok::Minus) => (Op::Sub, 10, 11),
                Some(Tok::Star) => (Op::Mul, 20, 21),
                Some(Tok::Slash) => (Op::Div, 20, 21),
                Some(Tok::Caret) => (Op::Pow, 31, 30), // 右結合
                _ => break,
            };
            if lbp < min_bp { break; }
            self.i += 1;
            let rhs = self.parse_expr(rbp)?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_prefix(&mut self) -> Result<Expr, String> {
        match self.next() {
            Some(Tok::Num(n)) => Ok(Expr::Num(n)),
            Some(Tok::Minus) => { let e = self.parse_expr(25)?; Ok(Expr::Neg(Box::new(e))) }
            Some(Tok::LPar) => { let e = self.parse_expr(0)?; self.eat(&Tok::RPar)?; Ok(e) }
            Some(Tok::Ident(id)) => self.parse_ident(id),
            other => Err(format!("unexpected token {:?}", other)),
        }
    }

    fn parse_ident(&mut self, id: String) -> Result<Expr, String> {
        match id.as_str() {
            "if" => {
                let lhs = self.parse_expr(5)?;
                let cmp = self.parse_cmp()?;
                let rhs = self.parse_expr(5)?;
                self.expect_ident("then")?;
                let a = self.parse_expr(5)?;
                self.expect_ident("else")?;
                let b = self.parse_expr(0)?;
                Ok(Expr::If(Box::new(Cond { lhs, cmp, rhs }), Box::new(a), Box::new(b)))
            }
            "sum" => {
                let close = match self.next() {
                    Some(Tok::LBrace) => Tok::RBrace,
                    Some(Tok::LPar) => Tok::RPar,
                    other => return Err(format!("sum expects {{ or (, got {:?}", other)),
                };
                let iters = self.parse_iters(&close)?;
                self.eat(&close)?;
                let body = self.parse_expr(10)?; // + / - の手前で停止
                Ok(Expr::Sum(iters, Box::new(body)))
            }
            "min" | "max" | "abs" | "sqrt" | "exp" | "log" | "pow" => {
                self.eat(&Tok::LPar)?;
                let mut args = Vec::new();
                if self.peek() != Some(&Tok::RPar) {
                    args.push(self.parse_expr(0)?);
                    while self.peek() == Some(&Tok::Comma) { self.i += 1; args.push(self.parse_expr(0)?); }
                }
                self.eat(&Tok::RPar)?;
                let f = match id.as_str() {
                    "min" => Func::Min, "max" => Func::Max, "abs" => Func::Abs,
                    "sqrt" => Func::Sqrt, "exp" => Func::Exp, "log" => Func::Log, "pow" => Func::Pow,
                    _ => unreachable!(),
                };
                Ok(Expr::Func(f, args))
            }
            _ => {
                // symbol with optional [idx, ...]（添字は識別子/数値のカンマ区切り）
                let mut idx = Vec::new();
                if self.peek() == Some(&Tok::LBracket) {
                    self.i += 1; // consume '['
                    loop {
                        match self.next() {
                            Some(Tok::Ident(s)) => idx.push(s),
                            Some(Tok::Num(n)) => idx.push(fmt_index(n)),
                            o => return Err(format!("bad index token {:?}", o)),
                        }
                        match self.peek() {
                            Some(Tok::Comma) => { self.i += 1; }
                            Some(Tok::RBracket) => { self.i += 1; break; }
                            o => return Err(format!("expected , or ] in index, got {:?}", o)),
                        }
                    }
                }
                Ok(Expr::Sym { name: id, idx })
            }
        }
    }

    fn parse_iters(&mut self, close: &Tok) -> Result<Vec<(String, SetRef)>, String> {
        let mut v = Vec::new();
        loop {
            let name = match self.next() { Some(Tok::Ident(s)) => s, o => return Err(format!("iter var expected, got {:?}", o)) };
            self.expect_ident("in")?;
            let set = self.parse_setref()?;
            v.push((name, set));
            if self.peek() == Some(&Tok::Comma) { self.i += 1; continue; }
            if self.peek() == Some(close) { break; }
            return Err("bad sum header".into());
        }
        Ok(v)
    }

    fn parse_setref(&mut self) -> Result<SetRef, String> {
        // Fase1: sum ヘッダの集合は「名前付き集合」のみ対応。
        // 範囲（a..b）は parser.rs が `set S = a..b` を要素へ展開済みなので、名前で参照する。
        match self.next() {
            Some(Tok::Ident(s)) => Ok(SetRef::Named(s)),
            o => Err(format!("set name expected in sum header, got {:?}", o)),
        }
    }

    fn parse_cmp(&mut self) -> Result<Cmp, String> {
        match self.next() {
            Some(Tok::Lt) => Ok(Cmp::Lt), Some(Tok::Le) => Ok(Cmp::Le),
            Some(Tok::Gt) => Ok(Cmp::Gt), Some(Tok::Ge) => Ok(Cmp::Ge),
            Some(Tok::EqEq) => Ok(Cmp::Eq), Some(Tok::Ne) => Ok(Cmp::Ne),
            o => Err(format!("comparison expected, got {:?}", o)),
        }
    }

    fn expect_ident(&mut self, kw: &str) -> Result<(), String> {
        match self.next() { Some(Tok::Ident(s)) if s == kw => Ok(()), o => Err(format!("expected '{}', got {:?}", kw, o)) }
    }
}

fn fmt_index(n: f64) -> String {
    if n.fract() == 0.0 { (n as i64).to_string() } else { n.to_string() }
}

pub fn compile(src: &str) -> Result<Expr, String> {
    let toks = lex(src)?;
    let mut p = P { t: toks, i: 0 };
    let e = p.parse_expr(0)?;
    if p.i != p.t.len() { return Err(format!("trailing tokens from position {}", p.i)); }
    Ok(e)
}

// ---- Evaluator ----
pub fn eval(e: &Expr, x: &[f64], env: &HashMap<String, String>, ctx: &Ctx) -> f64 {
    match e {
        Expr::Num(v) => *v,
        Expr::Sym { name, idx } => eval_sym(name, idx, x, env, ctx),
        Expr::Neg(a) => -eval(a, x, env, ctx),
        Expr::Bin(op, a, b) => {
            let av = eval(a, x, env, ctx);
            let bv = eval(b, x, env, ctx);
            match op {
                Op::Add => av + bv, Op::Sub => av - bv, Op::Mul => av * bv,
                Op::Div => if bv.abs() < 1e-12 { 0.0 } else { av / bv },
                Op::Pow => av.powf(bv),
            }
        }
        Expr::Func(f, args) => eval_func(*f, args, x, env, ctx),
        Expr::Sum(iters, body) => eval_sum(iters, body, x, env, ctx),
        Expr::If(c, a, b) => if eval_cond(c, x, env, ctx) { eval(a, x, env, ctx) } else { eval(b, x, env, ctx) },
    }
}

fn eval_cond(c: &Cond, x: &[f64], env: &HashMap<String, String>, ctx: &Ctx) -> bool {
    let a = eval(&c.lhs, x, env, ctx);
    let b = eval(&c.rhs, x, env, ctx);
    match c.cmp {
        Cmp::Lt => a < b, Cmp::Le => a <= b, Cmp::Gt => a > b, Cmp::Ge => a >= b,
        Cmp::Eq => (a - b).abs() < 1e-9, Cmp::Ne => (a - b).abs() >= 1e-9,
    }
}

fn eval_func(f: Func, args: &[Expr], x: &[f64], env: &HashMap<String, String>, ctx: &Ctx) -> f64 {
    let a = |k: usize| eval(&args[k], x, env, ctx);
    match f {
        Func::Min if args.len() == 2 => a(0).min(a(1)),
        Func::Max if args.len() == 2 => a(0).max(a(1)),
        Func::Abs if args.len() == 1 => a(0).abs(),
        Func::Sqrt if args.len() == 1 => { let v = a(0); if v < 0.0 { 0.0 } else { v.sqrt() } }
        Func::Exp if args.len() == 1 => a(0).exp(),
        Func::Log if args.len() == 1 => { let v = a(0); if v <= 0.0 { 0.0 } else { v.ln() } }
        Func::Pow if args.len() == 2 => a(0).powf(a(1)),
        _ => 0.0,
    }
}

fn eval_sym(name: &str, idx: &[String], x: &[f64], env: &HashMap<String, String>, ctx: &Ctx) -> f64 {
    if idx.is_empty() {
        if let Some(m) = ctx.params.get(name) { if let Some(v) = m.get("_") { return *v; } }
        if let Some(i) = ctx.var_map.get(name) { return x[*i]; }
        if let Some(sv) = env.get(name) { if let Ok(v) = sv.parse::<f64>() { return v; } }
        return 0.0;
    }
    let key: Vec<String> = idx.iter().map(|t| env.get(t).cloned().unwrap_or_else(|| t.clone())).collect();
    let k = key.join(",");
    let vk = format!("{}[{}]", name, k);
    if let Some(i) = ctx.var_map.get(&vk) { return x[*i]; }
    if let Some(m) = ctx.params.get(name) { if let Some(v) = m.get(&k) { return *v; } }
    0.0
}

fn eval_sum(iters: &[(String, SetRef)], body: &Expr, x: &[f64], env: &HashMap<String, String>, ctx: &Ctx) -> f64 {
    let mut acc = 0.0;
    let mut e2 = env.clone();
    sum_rec(iters, 0, body, x, &mut e2, ctx, &mut acc);
    acc
}

fn sum_rec(iters: &[(String, SetRef)], k: usize, body: &Expr, x: &[f64], env: &mut HashMap<String, String>, ctx: &Ctx, acc: &mut f64) {
    if k == iters.len() { *acc += eval(body, x, env, ctx); return; }
    let (ref var, ref sref) = iters[k];
    let vals: Vec<String> = match sref {
        SetRef::Named(s) => ctx.sets.get(s).cloned().unwrap_or_default(),
        SetRef::Range(a, b) => (*a..=*b).map(|v| v.to_string()).collect(),
    };
    for v in vals {
        env.insert(var.clone(), v);
        sum_rec(iters, k + 1, body, x, env, ctx, acc);
    }
    env.remove(var);
}
```

> **重要な実装 note（executor 向け）**: 上のスケルトンでは添字 `x[i]` の `[` `]` をレクサで扱っていない。
> 実装時は次のいずれかにする（推奨: A）:
> - **A. レクサに `LBracket`/`RBracket` を追加**し、`parse_ident` で `[` を見たら
>   `']'` まで識別子/数値をカンマ区切りで読み、`Sym{name, idx}` を作る。
> - B. 既存 `parser.rs` の `eval_symbol` と同様、識別子に `[` `]` を含めて 1 Ident とし、
>   `Sym` 生成時に名前と添字へ分解する。
> golden テスト `sum_over_set_with_params_and_vars` が通ることで正当性を確認する。

- [ ] **Step 4: `src/main.rs` に `mod expr;` を追加**

`src/main.rs`（`mod cli;` 群のそば）:
```rust
mod cli;
mod config;
mod expr;
mod parser;
mod solver;
```

- [ ] **Step 5: テストが通ることを確認**

Run: `. "$HOME/.cargo/env" && cargo test --lib 2>&1 | tail -20`
Expected: `test result: ok.`（expr の全テスト green）。`cargo clippy --all-targets -- -D warnings` も green。

- [ ] **Step 6: Commit**

```bash
git add src/expr.rs src/main.rs
git commit -m "feat(expr): add AST expression evaluator (Pratt parser) with unit tests"
```

---

## Task 2: AST を Model に配線（旧文字列評価器を置換）

Model が目的/制約/多目的を **コンパイル済み `Expr`** で保持し、評価を `expr::eval` 経由にする。旧 `evaluate_expr`/`eval_arith`/`eval_symbol`/`evaluate_sum`/`eval_if`/`eval_condition`/`eval_comparison` を削除する。

**Files:**
- Modify: `src/parser.rs`（Model 構造・`parse()` 末尾でコンパイル・`check_constraints`/`evaluate_objective` を AST 化・旧評価器削除・`Constraint` に `lhs/rhs` の Expr 追加）
- Modify: `src/solver/mod.rs`（`compute_fitness` を AST 経由へ）
- Test: `tests/parse_eval.rs`（フルパース→評価の統合）

**Model 変更（この形を後続でも使用）:**
- `Constraint { name:String, lhs:expr::Expr, rhs:expr::Expr, op:ConstraintOp }`（旧 `expr:String`/`rhs:f64` を置換）
- `Objective { name:String, ast:expr::Expr, maximize:bool }`（旧 `expr:String`）
- `Model.objective_ast: Option<expr::Expr>`（旧 `objective_expr:String` は保持しつつ末尾でコンパイルしても良いが、最終的に AST を正とする）
- Model に `fn ctx(&self) -> expr::Ctx`（`Ctx{ var_map, params, sets }` を返す）
- `evaluate_objective(&self, x)`: `objective_ast` を `expr::eval`。無ければ Sphere。
- `check_constraints(&self, x)`: 各制約で `eval(lhs)-eval(rhs)` から違反量算出（op 別）。

- [ ] **Step 1: 失敗する統合テストを書く**

`tests/parse_eval.rs`:
```rust
use std::process::Command;

fn run(src: &str, args: &[&str]) -> String {
    let dir = std::env::temp_dir().join(format!("optica_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("m.optica");
    std::fs::write(&path, src).unwrap();
    let mut a = vec![path.to_str().unwrap().to_string()];
    for s in args { a.push(s.to_string()); }
    let out = Command::new(env!("CARGO_BIN_EXE_optica")).args(&a).output().unwrap();
    String::from_utf8_lossy(&out.stdout).to_string()
}

#[test]
fn single_line_objective_with_function() {
    // maximize sqrt(y) の代わりに、関数が評価に効くことを確認: minimize obj: (y-3)^2 は y->3 で最小
    let out = run("var y >= 0 <= 10;\nminimize obj: (y - 3) ^ 2;\n", &["-m", "de"]);
    // 目的値が 0 付近、y ~ 3
    assert!(out.contains("y = 3.") || out.contains("y = 2.9") || out.contains("y = 3.0"), "got: {out}");
}
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `. "$HOME/.cargo/env" && cargo test --test parse_eval 2>&1 | tail -20`
Expected: FAIL（早期収束バグ or `^` 未配線で y が 3 にならない）

- [ ] **Step 3: `Constraint`/`Objective`/`Model` を AST 保持に変更し、`parse()` 末尾でコンパイル**

`src/parser.rs` の該当構造体・メソッドを次のように変更する（要点、完全な差し替え）:
```rust
#[derive(Debug, Clone)]
pub struct Constraint {
    #[allow(dead_code)]
    pub name: String,
    pub lhs: crate::expr::Expr,
    pub rhs: crate::expr::Expr,
    pub op: ConstraintOp,
}

#[derive(Debug, Clone)]
pub struct Objective {
    pub name: String,
    pub ast: crate::expr::Expr,
    pub maximize: bool,
}
```
Model に追加/変更:
```rust
pub objective_ast: Option<crate::expr::Expr>,
```
Model メソッド（旧 `evaluate_expr` 等を削除し置換）:
```rust
pub fn ctx(&self) -> crate::expr::Ctx {
    crate::expr::Ctx { var_map: &self.var_map, params: &self.params, sets: &self.sets }
}

pub fn evaluate_objective(&self, x: &[f64]) -> f64 {
    let env = std::collections::HashMap::new();
    if let Some(ref e) = self.objective_ast {
        crate::expr::eval(e, x, &env, &self.ctx())
    } else if let Some(o) = self.objectives.first() {
        crate::expr::eval(&o.ast, x, &env, &self.ctx())
    } else {
        x.iter().map(|&v| v * v).sum()
    }
}

pub fn check_constraints(&self, x: &[f64]) -> (bool, f64) {
    let env = std::collections::HashMap::new();
    let ctx = self.ctx();
    let mut feasible = true;
    let mut total = 0.0;
    for c in &self.constraints {
        let l = crate::expr::eval(&c.lhs, x, &env, &ctx);
        let r = crate::expr::eval(&c.rhs, x, &env, &ctx);
        let v = match c.op {
            ConstraintOp::Le => (l - r).max(0.0),
            ConstraintOp::Ge => (r - l).max(0.0),
            ConstraintOp::Eq => (l - r).abs(),
        };
        if v > 1e-9 { feasible = false; total += v; }
    }
    (feasible, total)
}
```
`parse_objective` / `parse_objective_named` / `parse_constraint` を、文字列ではなく `crate::expr::compile(...)` で AST を作るように変更する。目的式は `compile(expr_str)?` を `objective_ast` に格納。制約は `lhs_str`/`rhs_str` をそれぞれ `compile` する（RHS も式で可）。`compile` 失敗時は `parse()` が `Err` を返す。
- 削除する Model メソッド: `evaluate_expr`, `eval_if`, `eval_condition`, `eval_comparison`, `eval_arith`, `eval_symbol`, `evaluate_sum`。

- [ ] **Step 4: `compute_fitness`（solver）を AST 経由に更新**

`src/solver/mod.rs` の `compute_fitness` 内の `model.evaluate_expr(&obj.expr, x, &HashMap::new())` 呼び出しを、`crate::expr::eval(&obj.ast, x, &env, &model.ctx())` に置換する（weighted/epsilon/default の3経路すべて）。`model.evaluate_objective(x)` / `model.check_constraints(x)` はそのまま（内部が AST 化済み）。

- [ ] **Step 4b: 未知シンボル検証（parse 時に明示エラー）**

`src/expr.rs` に、Expr 内の（スコープ外＝loop 変数でない）シンボル基底名を収集する関数を追加:
```rust
pub fn collect_free_syms(e: &Expr, scope: &mut Vec<String>, out: &mut Vec<String>) {
    match e {
        Expr::Num(_) => {}
        Expr::Sym { name, .. } => { if !scope.iter().any(|s| s == name) { out.push(name.clone()); } }
        Expr::Neg(a) => collect_free_syms(a, scope, out),
        Expr::Bin(_, a, b) => { collect_free_syms(a, scope, out); collect_free_syms(b, scope, out); }
        Expr::Func(_, args) => { for a in args { collect_free_syms(a, scope, out); } }
        Expr::Sum(iters, body) => {
            let n = scope.len();
            for (v, _) in iters { scope.push(v.clone()); }
            collect_free_syms(body, scope, out);
            scope.truncate(n);
        }
        Expr::If(c, a, b) => {
            collect_free_syms(&c.lhs, scope, out); collect_free_syms(&c.rhs, scope, out);
            collect_free_syms(a, scope, out); collect_free_syms(b, scope, out);
        }
    }
}
```
`src/parser.rs` の `parse()` 末尾（var_map 構築後）で検証する。既知名 = パラメータ名 ∪ 集合名 ∪ 変数基底名（`var_names` の `[` 前）。目的と各制約について、`collect_free_syms`（forall 制約は `scope` に forall 変数を preload）で未知の基底名が出たら `Err(format!("unknown symbol: {name}"))`:
```rust
let mut known: std::collections::HashSet<String> = model.params.keys().cloned().collect();
for s in model.sets.keys() { known.insert(s.clone()); }
for vn in &model.var_names { known.insert(vn.split('[').next().unwrap().to_string()); }
let check = |e: &crate::expr::Expr, forall_vars: &[String]| -> Result<(), String> {
    let mut scope: Vec<String> = forall_vars.to_vec();
    let mut free = Vec::new();
    crate::expr::collect_free_syms(e, &mut scope, &mut free);
    for name in free {
        // env 由来の数値添字（純数字）や既知名は許可
        if name.parse::<f64>().is_err() && !known.contains(&name) {
            return Err(format!("unknown symbol: {}", name));
        }
    }
    Ok(())
};
if let Some(ref e) = model.objective_ast { check(e, &[])?; }
for o in &model.objectives { check(&o.ast, &[])?; }
for c in &model.constraints {
    let fv: Vec<String> = c.env.keys().cloned().collect();
    check(&c.lhs, &fv)?; check(&c.rhs, &fv)?;
}
```
（`c.env` は Task 3 で追加。Task 2 時点では forall 未実装なので `check(&c.lhs, &[])` / `check(&c.rhs, &[])` とし、Task 3 で `c.env` を渡すよう更新する。）

- [ ] **Step 4c: 未知シンボルのテストを追加**

`tests/parse_eval.rs` に追加:
```rust
#[test]
fn unknown_symbol_is_error() {
    let dir = std::env::temp_dir().join(format!("optica_unk_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("m.optica");
    std::fs::write(&path, "var y >= 0 <= 1;\nminimize obj: y + zzz;\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_optica")).arg(path.to_str().unwrap()).output().unwrap();
    let all = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(all.contains("unknown symbol") || !out.status.success(), "expected parse error, got: {all}");
}
```

- [ ] **Step 5: ビルド＆既存挙動確認**

Run: `. "$HOME/.cargo/env" && cargo build && cargo clippy --all-targets -- -D warnings 2>&1 | tail -5`
Expected: green（旧メソッド削除に伴う未使用 import があれば除去）

- [ ] **Step 6: knapsack が LP 緩和最適を返すか確認（手動）**

Run: `. "$HOME/.cargo/env" && cargo run --release -- examples/knapsack.optica`
Expected: `Objective` が正の profit（連続緩和）で、変数が表示される（0 ではない）。

- [ ] **Step 6b: parse_eval テストが緑になることを確認**

Run: `. "$HOME/.cargo/env" && cargo test --test parse_eval 2>&1 | tail -20`
Expected: PASS（`minimize (y-3)^2` は最適値 0 のため旧 early-return でも正しく y≈3 に収束。`^` 配線と未知シンボル検証を確認）。

- [ ] **Step 7: Commit**

```bash
git add src/parser.rs src/solver/mod.rs tests/parse_eval.rs
git commit -m "refactor(parser,solver): store compiled Expr AST and evaluate via expr::eval"
```

---

## Task 3: 複数行 `maximize:`/`subject to:` ブロック＋`forall` 制約展開

**Files:**
- Modify: `src/parser.rs`（`parse()` を「トップレベルキーワードまで論理行を連結」する前処理に変更、`forall` 展開を追加）
- Test: `tests/multiline.rs`

**方針:**
- パース前に、ソースを **論理文**に再構成する。行を走査し、`maximize`/`minimize`/制約ラベル/`forall` の本体は、次のトップレベルキーワード（`var`/`param`/`set`/`maximize`/`minimize`/`subject to`/`objectives:`/`data:`/`pareto`/EOF）またはインデントが戻るまで連結する。
- 制約行内に `forall <i> in <SET>[, <j> in <SET2>]:` があれば、対象集合のデカルト積で展開し、各展開で添字を `env` として lhs/rhs をコンパイル時に定数畳み込みせず、**制約ごとに `env` 束縛済みの Expr** を作る。実装簡便のため、`forall` 展開は「集合要素を添字トークンに束縛した Sym を持つ Expr を、要素ごとに複製」ではなく、**制約に `env`（固定束縛）を持たせる**方式にする:
  - `Constraint` に `env: HashMap<String,String>` を追加し、`check_constraints` で `eval(lhs, x, &c.env, ctx)` を使う。
  - forall 1本 → 集合直積の要素数だけ `Constraint`（同一 lhs/rhs Expr、異なる env）を push。

- [ ] **Step 1: 失敗するテストを書く**

`tests/multiline.rs`:
```rust
use std::process::Command;
fn run(src: &str, args: &[&str]) -> String {
    let dir = std::env::temp_dir().join(format!("optica_ml_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("m.optica");
    std::fs::write(&path, src).unwrap();
    let mut a = vec![path.to_str().unwrap().to_string()];
    for s in args { a.push(s.to_string()); }
    let out = Command::new(env!("CARGO_BIN_EXE_optica")).args(&a).output().unwrap();
    String::from_utf8_lossy(&out.stdout).to_string()
}

// 注: これらは「複数行の取り込み/forall 展開」を検証する。収束修正(Task 4)に依存しないよう、
// 最適 fitness が 0 or 正の問題を使う（旧 early-return でも正しく収束する）。

#[test]
fn multiline_objective_is_captured() {
    // 目的が次行。captured なら x→3（最適0）、dropped なら既定 Sphere で x→0。x で判別。
    let src = "set S = {1, 2};\nvar x[S] >= 0 <= 5;\nminimize c:\n    sum(i in S) (x[i] - 3) ^ 2\n";
    let out = run(src, &["-m", "de", "-i", "2000"]);
    assert!(out.contains("= 3.") || out.contains("= 2.9"), "objective dropped? x should be ~3: {out}");
}

#[test]
fn forall_constraint_expands() {
    // forall i in S: x[i] >= 2 を各要素へ展開。minimize sum x[i] は展開時のみ最適 6、
    // 展開されないと 0。最適 fitness=6(正) なので収束修正前でも収束する。
    let src = "set S = {1, 2, 3};\nvar x[S] >= 0 <= 10;\nminimize c:\n    sum(i in S) x[i]\nsubject to:\n    lo:\n        forall i in S:\n            x[i] >= 2\n";
    let out = run(src, &["-m", "de", "-i", "2000"]);
    let o: f64 = out.lines().find(|l| l.starts_with("Objective:")).unwrap()
        .split_whitespace().last().unwrap().parse().unwrap();
    assert!((o - 6.0).abs() < 0.1, "forall not expanded? expected ~6: {out}");
}
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `. "$HOME/.cargo/env" && cargo test --test multiline 2>&1 | tail -20`
Expected: FAIL（複数行目的が空・forall 未対応）

- [ ] **Step 3: 論理行連結の前処理を実装**

`src/parser.rs` の `parse()` 冒頭で、`source.lines()` を走査して論理文 `Vec<String>` を構築する関数 `fn logical_statements(source: &str) -> Vec<String>` を追加し、以降のパースはその論理文に対して行う。連結規則:
- コメント/空行は除去。
- トップレベルキーワードで始まる行は新しい論理文を開始。
- `maximize:`/`minimize:`/制約ラベル（`ident:` で末尾コロン）/`forall ...:` の後続で、次のトップレベルキーワードが来るまでの行を、前の論理文に空白連結する。
- `subject to:` は制約セクション開始マーカーとして単独の論理文。

- [ ] **Step 4: `forall` 展開を実装**

制約パース `parse_constraint` を、`forall` プレフィックスを剥がして集合を集めるよう拡張:
```rust
// 例: "cap: forall i in S: x[i] <= 1"
// → env {i: 各要素} を持つ Constraint を要素数ぶん生成
```
`Constraint` に `pub env: std::collections::HashMap<String, String>`（forall 無しは空）を追加。`check_constraints` を `eval(&c.lhs, x, &c.env, &ctx)` に更新。`cartesian`/`expand_indices` 既存関数を再利用して直積を作る。

- [ ] **Step 5: テストが通ることを確認**

Run: `. "$HOME/.cargo/env" && cargo test --test multiline 2>&1 | tail -20`
Expected: PASS（テストは収束修正に依存しない設計。最適 fitness が 0/正の問題を使用）

- [ ] **Step 6: Commit**

```bash
git add src/parser.rs tests/multiline.rs
git commit -m "feat(parser): multi-line maximize/subject-to blocks and forall constraint expansion"
```

---

## Task 4: 早期収束バグ修正（`best_fit < TOLERANCE` 撤去）

**Files:**
- Modify: `src/config.rs`（`STALL_ITERS` 追加）
- Modify: `src/solver/mod.rs`（`de_single`/`de_parallel`/`pso` の早期 return を停滞ベースに）
- Test: `tests/convergence.rs`

- [ ] **Step 1: 失敗するテストを書く**

`tests/convergence.rs`:
```rust
use std::process::Command;
fn run(src: &str) -> String {
    let dir = std::env::temp_dir().join(format!("optica_cv_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("m.optica");
    std::fs::write(&path, src).unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_optica")).args([path.to_str().unwrap(), "-m", "de"]).output().unwrap();
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn obj(out: &str) -> f64 {
    out.lines().find(|l| l.starts_with("Objective:")).unwrap()
        .split_whitespace().last().unwrap().parse().unwrap()
}

#[test]
fn maximize_interior_optimum_converges() {
    // maximize 10 - (y-3)^2 → 最適 y=3, obj=10。早期終了バグがあると 10 に届かない。
    let out = run("var y >= 0 <= 10;\nmaximize obj: 10 - (y - 3) ^ 2;\n");
    assert!((obj(&out) - 10.0).abs() < 1e-3, "expected ~10, got: {out}");
    assert!(!out.contains("Iterations: 1\n"), "must not stop at iteration 1: {out}");
}

#[test]
fn minimize_negative_optimum_converges() {
    // minimize (y-3)^2 - 100 → 最適 y=3, obj=-100。
    let out = run("var y >= 0 <= 10;\nminimize obj: (y - 3) ^ 2 - 100;\n");
    assert!((obj(&out) - (-100.0)).abs() < 1e-3, "expected ~-100, got: {out}");
}
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `. "$HOME/.cargo/env" && cargo test --test convergence 2>&1 | tail -20`
Expected: FAIL（`Iterations: 1` で早期終了、obj が最適に届かない）

- [ ] **Step 3: `config.rs` に停滞閾値を追加**

`src/config.rs`:
```rust
/// 改善が見られない反復数がこれを超えたら停止（早期収束の代替）
pub const STALL_ITERS: usize = 200;
```

- [ ] **Step 4: `de_single` の早期 return を停滞ベースに置換**

`src/solver/mod.rs` `de_single`：`if best_fit < TOLERANCE { return (best, best_fit, iter + 1); }` を削除し、改善なし反復カウンタを導入:
```rust
let mut stall = 0usize;
for iter in 0..max_iter {
    let mut improved = false;
    for i in 0..POP_SIZE {
        // ... 既存の変異・選択 ...
        if trial_fit <= pop.fit[i] {
            pop.update(i, &trial, trial_fit);
            if trial_fit < best_fit - TOLERANCE {
                best_fit = trial_fit;
                best.copy_from_slice(&trial);
                improved = true;
            }
        }
    }
    if improved { stall = 0; } else { stall += 1; }
    if stall >= STALL_ITERS { return (best, best_fit, iter + 1); }
}
(best, best_fit, max_iter)
```
（負値 fitness を収束扱いにしない。`TOLERANCE` は「有意な改善幅」判定にのみ使用。）

- [ ] **Step 5: `de_parallel` と `pso` も同様に修正**

`de_parallel` の各スレッドループから `if best_fit < TOLERANCE { return (best, best_fit); }` を削除し、同じ stall ロジックに置換（返り値は `(best, best_fit)`）。`pso` の `if gbest_fit < TOLERANCE { return (gbest, gbest_fit, iter + 1); }` も同様に停滞ベースへ置換。

- [ ] **Step 6: テストが通ることを確認**

Run: `. "$HOME/.cargo/env" && cargo test --test convergence 2>&1 | tail -20`
Expected: PASS。加えて `cargo test --test parse_eval --test multiline` も PASS。

- [ ] **Step 7: Commit**

```bash
git add src/config.rs src/solver/mod.rs tests/convergence.rs
git commit -m "fix(solver): replace premature best_fit<TOLERANCE exit with stall-based stopping"
```

---

## Task 5: 整数性の丸め込み修復（`binary`/`int`）

**Files:**
- Modify: `src/parser.rs`（`Model.var_int: Vec<bool>`、`parse_var`/`parse_state_or_decision` で捕捉）
- Modify: `src/solver/mod.rs`（`compute_fitness` 冒頭で repair、報告解も repair）
- Modify: `src/main.rs`（`print_result` で報告前に repair・目的再評価）
- Test: `tests/integrality.rs`

- [ ] **Step 1: 失敗するテストを書く**

`tests/integrality.rs`:
```rust
use std::process::Command;
fn run(path: &str) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_optica")).args([path, "-m", "de"]).output().unwrap();
    String::from_utf8_lossy(&out.stdout).to_string()
}

#[test]
fn binary_vars_are_integral() {
    // simple_knapsack: var x[ITEMS] binary; maximize count sum x[i]; sum<=2 → 2 個が 1、他 0
    let out = run("examples/simple_knapsack.optica");
    // 印字される各変数は 0 か 1 のみ（小数点以下が 0）
    for line in out.lines().filter(|l| l.contains(" = ")) {
        let v: f64 = line.split('=').nth(1).unwrap().trim().parse().unwrap();
        assert!((v - v.round()).abs() < 1e-9, "non-integral binary var: {line}");
    }
    assert!(out.contains("Objective: 2") || out.contains("2.000000e0"), "count should be 2: {out}");
}
```

- [ ] **Step 2: テストが失敗することを確認**

Run: `. "$HOME/.cargo/env" && cargo test --test integrality 2>&1 | tail -20`
Expected: FAIL（x が小数、count≠2）

- [ ] **Step 3: `Model.var_int` を追加し捕捉**

`src/parser.rs`：Model に `pub var_int: Vec<bool>` を追加（`Model::new` で空、`parse()` 末尾で長さを `dim` に合わせる）。`parse_var` で `binary`/`int`/`Integer` を検出し、変数ごとに push（`binary` は `[0,1]`＋int）。`parse_state_or_decision` の `_is_int` も反映。

- [ ] **Step 4: `compute_fitness` に repair を追加**

`src/solver/mod.rs` の `compute_fitness(model, x)` 冒頭に、整数変数があれば丸めた作業ベクトルへ差し替える
シャドーイングを入れる（ライフタイム回避のため所有 Vec を使う）:
```rust
fn compute_fitness(model: &Model, x: &[f64]) -> f64 {
    let mut repaired: Vec<f64>;
    let x: &[f64] = if model.var_int.iter().any(|&b| b) {
        repaired = x.to_vec();
        for (j, &is_int) in model.var_int.iter().enumerate() {
            if is_int {
                repaired[j] = x[j].round().clamp(model.lb[j], model.ub[j]);
            }
        }
        &repaired
    } else {
        x
    };
    // ↓ 既存の目的・制約・ペナルティ評価はこの `x`（整数丸め済み）を使う
    // ... 既存コード ...
}
```
（`repaired` を if の外で宣言し、branch 内で代入・借用することで所有権とライフタイムを両立させる。）

報告用に、同じロジックの公開ヘルパも用意して Task 5 Step 5 で再利用する:
```rust
pub fn round_integers(model: &Model, x: &[f64]) -> Vec<f64> {
    let mut v = x.to_vec();
    for (j, &is_int) in model.var_int.iter().enumerate() {
        if is_int { v[j] = x[j].round().clamp(model.lb[j], model.ub[j]); }
    }
    v
}
```

- [ ] **Step 5: 報告解の repair＋目的再評価**

`src/main.rs` `cmd_solve` で、solver が返した `best` を `let best = crate::solver::round_integers(&model, &best);` で整数丸めしてから、
`let fitness = /* repair 済み best で目的再評価 */` として `print_result` に渡す。目的の再評価は
`model.evaluate_objective(&best)`（maximize なら符号反転して表示用 `obj` を計算）を用い、表示の目的値と変数が整合するようにする。

- [ ] **Step 6: テストが通ることを確認**

Run: `. "$HOME/.cargo/env" && cargo test --test integrality 2>&1 | tail -20`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add src/parser.rs src/solver/mod.rs src/main.rs tests/integrality.rs
git commit -m "feat(solver): enforce integrality via rounding repair for binary/int vars"
```

---

## Task 6: 厳選 golden 例（data 内蔵）＋統合テスト

**Files:**
- Create: `examples/f1_lp_production.optica`, `examples/f1_knapsack_binary.optica`, `examples/f1_nlp_curve.optica`
- Test: `tests/golden.rs`

**golden 値（解析的に確定）:**
- `f1_knapsack_binary`: value={A:60,B:100,C:120}, weight={A:10,B:20,C:30}, capacity=50 の 0/1 ナップサック。最適は B+C=220（weight 50）。→ **既知最適 220**。
- `f1_lp_production`: 1 資源・2 製品の小 LP（下に定義）。頂点解を手計算して golden を確定。
- `f1_nlp_curve`: `minimize (y-2)^2 + 1`, y∈[0,5] → 最適 y=2, obj=1（`^` を使う）。

- [ ] **Step 1: golden 例ファイルを作成**

`examples/f1_knapsack_binary.optica`:
```optica
set ITEMS = {A, B, C};
param value[ITEMS] = {A: 60, B: 100, C: 120};
param weight[ITEMS] = {A: 10, B: 20, C: 30};
param capacity = 50;
var x[ITEMS] binary;
maximize profit: sum{i in ITEMS} value[i] * x[i];
subject to cap: sum{i in ITEMS} weight[i] * x[i] <= capacity;
```

`examples/f1_lp_production.optica`:
```optica
set PROD = {A, B};
param profit[PROD] = {A: 3, B: 5};
param usage[PROD] = {A: 1, B: 2};
param cap = 10;
var x[PROD] >= 0 <= 100;
maximize total: sum{p in PROD} profit[p] * x[p];
subject to res: sum{p in PROD} usage[p] * x[p] <= cap;
```
（解析: profit/usage 比は A=3, B=2.5 なので A を優先。A は上限 100 まで作れるが資源 usage[A]=1×x[A] ≤ 10 → x[A]=10, x[B]=0 → total=30。golden=**30**。）

`examples/f1_nlp_curve.optica`:
```optica
var y >= 0 <= 5;
minimize obj: (y - 2) ^ 2 + 1;
```
（golden: y=2, obj=1）

- [ ] **Step 2: 失敗する golden テストを書く**

`tests/golden.rs`:
```rust
use std::process::Command;
fn obj_of(path: &str) -> f64 {
    let out = Command::new(env!("CARGO_BIN_EXE_optica")).args([path, "-m", "de", "-i", "2000"]).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout);
    s.lines().find(|l| l.starts_with("Objective:")).unwrap_or("Objective: nan")
        .split_whitespace().last().unwrap().parse().unwrap_or(f64::NAN)
}

#[test]
fn knapsack_binary_optimum() { assert!((obj_of("examples/f1_knapsack_binary.optica") - 220.0).abs() < 1e-6); }

#[test]
fn lp_production_optimum() { assert!((obj_of("examples/f1_lp_production.optica") - 30.0).abs() < 1e-3); }

#[test]
fn nlp_curve_optimum() { assert!((obj_of("examples/f1_nlp_curve.optica") - 1.0).abs() < 1e-3); }

#[test]
fn existing_simple_knapsack_count() { assert!((obj_of("examples/simple_knapsack.optica") - 2.0).abs() < 1e-6); }
```

- [ ] **Step 3: テストを実行**

Run: `. "$HOME/.cargo/env" && cargo test --test golden 2>&1 | tail -20`
Expected: 最初は一部 FAIL の可能性（例: 整数最適に丸め修復が届かない）。届かない場合は反復数を上げる（`-i 5000`）か、
`f1_knapsack_binary` の期待に「実行可能かつ obj ≤ 220 かつ ≥ 既知下界」を許容する形へ緩めるのではなく、
**まず `-i` を上げて厳密最適到達を狙う**。3変数 0/1 なので容易に到達するはず。

- [ ] **Step 4: 全テスト green を確認**

Run: `. "$HOME/.cargo/env" && cargo test 2>&1 | tail -20`
Expected: 全 `test result: ok.`

- [ ] **Step 5: Commit**

```bash
git add examples/f1_*.optica tests/golden.rs
git commit -m "test: add data-embedded golden examples (LP/binary-knapsack/NLP) with known optima"
```

---

## Task 7: ドキュメント・experimental 明示・CHANGELOG

**Files:**
- Modify: `README.md`（言語仕様・関数一覧・複数行/forall 対応・整数性・golden 例・experimental 注記）
- Modify: `CHANGELOG.md`
- Modify: experimental な既存例のヘッダ（`06_dp_inventory` 等）にコメント注記

- [ ] **Step 1: README を実態に更新**

`README.md` に次を反映（該当節を編集）:
- サポート関数: `min max abs sqrt exp log pow`、演算子 `+ - * / ^`、`sum{..}`/`sum(..)`、`if..then..else`、`forall ..:`。
- 変数: `binary`/`int` は丸め込み修復で整数化（真の MILP ではない旨）。
- 動作確認済み例（golden）: `knapsack` `simple_knapsack` `f1_lp_production` `f1_knapsack_binary` `f1_nlp_curve`。
- **experimental**: `06_dp_inventory` `07_stochastic_farmer` `08_combinatorial_tsp` `09_metaheuristic_vrp` `10_cp_scheduling` `11_moo_supply_chain` `12_ml_optimization` `13_largescale_decomposition` `juku_timetabling`（未対応構文/データ未整備。`def`/`import` はパースエラーになる）。

- [ ] **Step 2: experimental 例のヘッダに注記**

各 experimental `.optica` の先頭コメントに1行追加:
```optica
# [EXPERIMENTAL] 未対応構文/データ未整備。現行 Optica では正しく解けません（Fase2 対象）。
```

- [ ] **Step 3: CHANGELOG 追記**

`CHANGELOG.md` 冒頭に:
```markdown
## Unreleased - Fase1: Trustworthy Core
- fix: 早期収束バグ（best_fit<TOLERANCE で1反復停止）を停滞ベース停止に置換
- feat: AST 式評価器へ置換（min 修正、abs/sqrt/exp/log/pow・^ 追加、未知関数はエラー）
- feat: 複数行 maximize:/subject to: と forall 制約展開に対応
- feat: binary/int を丸め込み修復で整数化
- test: golden 例（LP/0-1ナップサック/NLP）と回帰テストを追加
- chore: stale な追跡バイナリを撤去し .gitignore を追加
- docs: 未対応構文/データ未整備の例を experimental 明示
```

- [ ] **Step 4: 最終確認（CI 相当）**

Run:
```bash
. "$HOME/.cargo/env"
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```
Expected: すべて green。

- [ ] **Step 5: Commit**

```bash
git add README.md CHANGELOG.md examples/*.optica
git commit -m "docs: update README/CHANGELOG, mark experimental examples"
```

---

## 完了チェック（DoD 対応）

- [ ] `cargo fmt --check` / `cargo clippy --all-targets -- -D warnings` / `cargo test` 緑（テストあり）
- [ ] golden 5 例（`knapsack` `simple_knapsack` `f1_lp_production` `f1_knapsack_binary` `f1_nlp_curve`）が既知最適 ± 許容内
- [ ] 回帰テスト（複数行・forall・収束・関数/^・整数性）緑
- [ ] 未知関数/構文が明示エラー
- [ ] 追跡バイナリ撤去・README/CHANGELOG 更新・experimental 明示
