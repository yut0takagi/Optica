//! AST ベースの式評価器（Pratt パーサ）。
//! 文字列を1度だけ Expr にコンパイルし、以後は再帰評価する。

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cmp {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Func {
    Min,
    Max,
    Abs,
    Sqrt,
    Exp,
    Log,
    Pow,
}

/// 集約演算子（`max(i in S) body` / `min(i in S) body` の縮約種別）。
/// 2引数関数の `Func::Min`/`Func::Max` とは別物（#13）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AggOp {
    Min,
    Max,
}

#[derive(Debug, Clone)]
pub enum SetRef {
    Named(String),
    #[allow(dead_code)]
    Range(i64, i64),
}

#[derive(Debug, Clone)]
pub enum Expr {
    Num(f64),
    Sym {
        name: String,
        idx: Vec<String>,
    },
    Neg(Box<Expr>),
    Bin(Op, Box<Expr>, Box<Expr>),
    Func(Func, Vec<Expr>),
    Sum(Vec<(String, SetRef)>, Box<Expr>),
    /// 集約 min/max: `max(i in S[, j in T...]) body` / `min(...)`。
    /// 2引数の `Func::Min`/`Func::Max` とは別物（#13）。
    Agg(AggOp, Vec<(String, SetRef)>, Box<Expr>),
    If(Box<Cond>, Box<Expr>, Box<Expr>),
}

#[derive(Debug, Clone)]
pub struct Cond {
    pub lhs: Expr,
    pub cmp: Cmp,
    pub rhs: Expr,
}

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
    LPar,
    RPar,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Comma,
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Lt,
    Le,
    Gt,
    Ge,
    EqEq,
    Ne,
}

fn lex(src: &str) -> Result<Vec<Tok>, String> {
    let b = src.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < b.len() {
        let c = b[i] as char;
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        match c {
            '(' => {
                out.push(Tok::LPar);
                i += 1;
            }
            ')' => {
                out.push(Tok::RPar);
                i += 1;
            }
            '{' => {
                out.push(Tok::LBrace);
                i += 1;
            }
            '}' => {
                out.push(Tok::RBrace);
                i += 1;
            }
            '[' => {
                out.push(Tok::LBracket);
                i += 1;
            }
            ']' => {
                out.push(Tok::RBracket);
                i += 1;
            }
            ',' => {
                out.push(Tok::Comma);
                i += 1;
            }
            '+' => {
                out.push(Tok::Plus);
                i += 1;
            }
            '-' => {
                out.push(Tok::Minus);
                i += 1;
            }
            '*' => {
                out.push(Tok::Star);
                i += 1;
            }
            '/' => {
                out.push(Tok::Slash);
                i += 1;
            }
            '^' => {
                out.push(Tok::Caret);
                i += 1;
            }
            '<' => {
                if i + 1 < b.len() && b[i + 1] == b'=' {
                    out.push(Tok::Le);
                    i += 2;
                } else {
                    out.push(Tok::Lt);
                    i += 1;
                }
            }
            '>' => {
                if i + 1 < b.len() && b[i + 1] == b'=' {
                    out.push(Tok::Ge);
                    i += 2;
                } else {
                    out.push(Tok::Gt);
                    i += 1;
                }
            }
            '=' => {
                if i + 1 < b.len() && b[i + 1] == b'=' {
                    out.push(Tok::EqEq);
                    i += 2;
                } else {
                    return Err("unexpected '='".into());
                }
            }
            '!' => {
                if i + 1 < b.len() && b[i + 1] == b'=' {
                    out.push(Tok::Ne);
                    i += 2;
                } else {
                    return Err("unexpected '!'".into());
                }
            }
            _ if c.is_ascii_digit() => {
                let s = i;
                while i < b.len() && (b[i] as char).is_ascii_digit() {
                    i += 1;
                }
                // 小数点は「. の直後が数字」のときだけ消費（範囲 ".." を食わない）
                if i + 1 < b.len() && b[i] == b'.' && (b[i + 1] as char).is_ascii_digit() {
                    i += 1;
                    while i < b.len() && (b[i] as char).is_ascii_digit() {
                        i += 1;
                    }
                }
                let n = src[s..i].parse::<f64>().map_err(|e| e.to_string())?;
                out.push(Tok::Num(n));
            }
            _ if c.is_ascii_alphabetic() || c == '_' => {
                let s = i;
                i += 1;
                while i < b.len() {
                    let ch = b[i] as char;
                    if ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' {
                        i += 1;
                    } else {
                        break;
                    }
                }
                out.push(Tok::Ident(src[s..i].to_string()));
            }
            other => return Err(format!("unexpected char '{}'", other)),
        }
    }
    Ok(out)
}

// ---- Parser (Pratt) ----
struct P {
    t: Vec<Tok>,
    i: usize,
}

impl P {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }
    fn next(&mut self) -> Option<Tok> {
        let v = self.t.get(self.i).cloned();
        if v.is_some() {
            self.i += 1;
        }
        v
    }
    fn eat(&mut self, t: &Tok) -> Result<(), String> {
        if self.peek() == Some(t) {
            self.i += 1;
            Ok(())
        } else {
            Err(format!("expected {:?}", t))
        }
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
            if lbp < min_bp {
                break;
            }
            self.i += 1;
            let rhs = self.parse_expr(rbp)?;
            lhs = Expr::Bin(op, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn parse_prefix(&mut self) -> Result<Expr, String> {
        match self.next() {
            Some(Tok::Num(n)) => Ok(Expr::Num(n)),
            Some(Tok::Minus) => {
                let e = self.parse_expr(25)?;
                Ok(Expr::Neg(Box::new(e)))
            }
            Some(Tok::LPar) => {
                let e = self.parse_expr(0)?;
                self.eat(&Tok::RPar)?;
                Ok(e)
            }
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
                Ok(Expr::If(
                    Box::new(Cond { lhs, cmp, rhs }),
                    Box::new(a),
                    Box::new(b),
                ))
            }
            "sum" => {
                let close = match self.next() {
                    Some(Tok::LBrace) => Tok::RBrace,
                    Some(Tok::LPar) => Tok::RPar,
                    other => return Err(format!("sum expects {{ or (, got {:?}", other)),
                };
                let iters = self.parse_iters(&close)?;
                self.eat(&close)?;
                // NOTE(deviation from plan): 計画コードは min_bp=10 だったが、
                // Add/Sub の lbp も 10 のため `lbp < min_bp` が偽になり `+` を飲み込む
                // オフバイワンバグがあった（sum_added_terms_split_correctly が 6.0 != 4.0 で失敗）。
                // Add/Sub の rbp と同じ 11 にして「+ / - の手前で確実に停止」という
                // コメントの意図通りに修正。
                let body = self.parse_expr(11)?; // + / - の手前で停止

                Ok(Expr::Sum(iters, Box::new(body)))
            }
            "min" | "max" => {
                self.eat(&Tok::LPar)?;
                // 2トークン先読みで「集約」か「2引数関数」かを判定する（#13）。
                // `min(i in S) body` / `max(i in S) body` の集約形は、
                // 直後が Ident でさらにその次が Ident("in") という形にしかならない。
                // `min(a, b)` や `min(x[i], y)` 等の2引数関数呼び出しはこの形にならない
                // （2番目のトークンが `,` や `[` になるため）。
                // 覗き見のみでトークンは消費しない。
                let is_agg = matches!(self.t.get(self.i), Some(Tok::Ident(_)))
                    && matches!(self.t.get(self.i + 1), Some(Tok::Ident(w)) if w == "in");
                let op = if id == "min" { AggOp::Min } else { AggOp::Max };
                if is_agg {
                    let iters = self.parse_iters(&Tok::RPar)?;
                    self.eat(&Tok::RPar)?;
                    // sum と同じ規約: + / - の手前で本体パースを止める
                    // （`max(i in S) a[i] + b` が「max の後に + b」と誤読されないようにする）。
                    let body = self.parse_expr(11)?;
                    Ok(Expr::Agg(op, iters, Box::new(body)))
                } else {
                    let mut args = Vec::new();
                    if self.peek() != Some(&Tok::RPar) {
                        args.push(self.parse_expr(0)?);
                        while self.peek() == Some(&Tok::Comma) {
                            self.i += 1;
                            args.push(self.parse_expr(0)?);
                        }
                    }
                    self.eat(&Tok::RPar)?;
                    if args.len() != 2 {
                        return Err(format!("{} expects 2 argument(s), got {}", id, args.len()));
                    }
                    let f = if id == "min" { Func::Min } else { Func::Max };
                    Ok(Expr::Func(f, args))
                }
            }
            "abs" | "sqrt" | "exp" | "log" | "pow" => {
                self.eat(&Tok::LPar)?;
                let mut args = Vec::new();
                if self.peek() != Some(&Tok::RPar) {
                    args.push(self.parse_expr(0)?);
                    while self.peek() == Some(&Tok::Comma) {
                        self.i += 1;
                        args.push(self.parse_expr(0)?);
                    }
                }
                self.eat(&Tok::RPar)?;
                let f = match id.as_str() {
                    "abs" => Func::Abs,
                    "sqrt" => Func::Sqrt,
                    "exp" => Func::Exp,
                    "log" => Func::Log,
                    "pow" => Func::Pow,
                    _ => unreachable!(),
                };
                let expected_arity = match f {
                    Func::Pow => 2,
                    Func::Abs | Func::Sqrt | Func::Exp | Func::Log => 1,
                    Func::Min | Func::Max => unreachable!(),
                };
                if args.len() != expected_arity {
                    return Err(format!(
                        "{} expects {} argument(s), got {}",
                        id,
                        expected_arity,
                        args.len()
                    ));
                }
                Ok(Expr::Func(f, args))
            }
            _ => {
                // 組み込み関数でない識別子の直後に `(` が来たらユーザー定義関数呼び出し。
                // 未対応（Issue #3）なので曖昧なシンボル解釈（trailing tokens）に落とさず
                // 明示エラーにする。正当な添字参照 `x[i]` は `[` なので影響しない。
                if self.peek() == Some(&Tok::LPar) {
                    return Err(format!(
                        "unknown function '{}': user-defined functions are not supported \
                         (see docs/SPEC_SUPPORT.md)",
                        id
                    ));
                }
                // symbol with optional [idx, ...]（添字は識別子/数値のカンマ区切り）。
                // 各要素は base に続けて任意で `± 整数リテラル` を許可する（#9: `t-1`/`t+1`）。
                // それより複雑な添字式（`t*2`, `t+1.5`, `t-s` 等）は明示エラーにする。
                let mut idx = Vec::new();
                if self.peek() == Some(&Tok::LBracket) {
                    self.i += 1; // consume '['
                    loop {
                        let base = match self.next() {
                            Some(Tok::Ident(s)) => s,
                            Some(Tok::Num(n)) => fmt_index(n),
                            o => return Err(format!("bad index token {:?}", o)),
                        };
                        // 添字算術 base ± N（整数リテラルのみ）。`"t-1"` の形で保持し、
                        // 評価時に base を env 解決してからオフセットを適用する。
                        let term = match self.peek() {
                            Some(Tok::Plus) | Some(Tok::Minus) => {
                                let is_plus = self.peek() == Some(&Tok::Plus);
                                self.i += 1; // consume +/-
                                match self.next() {
                                    Some(Tok::Num(n)) if n.fract() == 0.0 => format!(
                                        "{}{}{}",
                                        base,
                                        if is_plus { '+' } else { '-' },
                                        n as i64
                                    ),
                                    o => {
                                        return Err(format!(
                                            "unsupported subscript expression after '{}': only 'base +/- integer' is allowed (e.g. t-1), got {:?}",
                                            base, o
                                        ))
                                    }
                                }
                            }
                            _ => base,
                        };
                        idx.push(term);
                        match self.peek() {
                            Some(Tok::Comma) => {
                                self.i += 1;
                            }
                            Some(Tok::RBracket) => {
                                self.i += 1;
                                break;
                            }
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
            let name = match self.next() {
                Some(Tok::Ident(s)) => s,
                o => return Err(format!("iter var expected, got {:?}", o)),
            };
            self.expect_ident("in")?;
            let set = self.parse_setref()?;
            v.push((name, set));
            if self.peek() == Some(&Tok::Comma) {
                self.i += 1;
                continue;
            }
            if self.peek() == Some(close) {
                break;
            }
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
            Some(Tok::Lt) => Ok(Cmp::Lt),
            Some(Tok::Le) => Ok(Cmp::Le),
            Some(Tok::Gt) => Ok(Cmp::Gt),
            Some(Tok::Ge) => Ok(Cmp::Ge),
            Some(Tok::EqEq) => Ok(Cmp::Eq),
            Some(Tok::Ne) => Ok(Cmp::Ne),
            o => Err(format!("comparison expected, got {:?}", o)),
        }
    }

    fn expect_ident(&mut self, kw: &str) -> Result<(), String> {
        match self.next() {
            Some(Tok::Ident(s)) if s == kw => Ok(()),
            o => Err(format!("expected '{}', got {:?}", kw, o)),
        }
    }
}

fn fmt_index(n: f64) -> String {
    if n.fract() == 0.0 {
        (n as i64).to_string()
    } else {
        n.to_string()
    }
}

/// 添字トークンを具体値に解決する（#9）。`"t-1"` の形（base ± 整数オフセット）は
/// base を env 解決した上で整数オフセットを適用する。プレーンな添字は従来通り env 解決。
fn resolve_index_token(t: &str, env: &HashMap<String, String>) -> String {
    if let Some((base, off)) = split_index_offset(t) {
        let base_val = env.get(base).map(String::as_str).unwrap_or(base);
        if let Ok(n) = base_val.parse::<i64>() {
            return (n + off).to_string();
        }
        // 非数値 base にはオフセットを適用できない。base をそのまま解決する。
        return env.get(base).cloned().unwrap_or_else(|| base.to_string());
    }
    env.get(t).cloned().unwrap_or_else(|| t.to_string())
}

/// `"t-1"` -> Some(("t", -1)), `"t+2"` -> Some(("t", 2)), プレーンな添字 -> None。
/// 位置1以降で最初の +/- を境に base と整数オフセットへ分割する（先頭の負号は対象外）。
fn split_index_offset(t: &str) -> Option<(&str, i64)> {
    let pos = t
        .char_indices()
        .skip(1)
        .find(|(_, c)| *c == '+' || *c == '-')
        .map(|(i, _)| i)?;
    let base = &t[..pos];
    let sign = t.as_bytes()[pos];
    let mag: i64 = t[pos + 1..].parse().ok()?;
    let off = if sign == b'-' { -mag } else { mag };
    Some((base, off))
}

pub fn compile(src: &str) -> Result<Expr, String> {
    let toks = lex(src)?;
    let mut p = P { t: toks, i: 0 };
    let e = p.parse_expr(0)?;
    if p.i != p.t.len() {
        return Err(format!("trailing tokens from position {}", p.i));
    }
    Ok(e)
}

/// 単独の比較条件 `lhs <cmp> rhs` をコンパイルする（#10 の `forall ... where <cond>` 用）。
pub fn compile_cond(src: &str) -> Result<Cond, String> {
    let toks = lex(src)?;
    let mut p = P { t: toks, i: 0 };
    let lhs = p.parse_expr(0)?;
    let cmp = p.parse_cmp()?;
    let rhs = p.parse_expr(0)?;
    if p.i != p.t.len() {
        return Err(format!(
            "trailing tokens in condition from position {}",
            p.i
        ));
    }
    Ok(Cond { lhs, cmp, rhs })
}

/// 条件を評価する（#10）。パース時の `where` フィルタでは x=&[] を渡す。
pub fn eval_cond_now(c: &Cond, x: &[f64], env: &HashMap<String, String>, ctx: &Ctx) -> bool {
    eval_cond(c, x, env, ctx)
}

/// Expr 内の（スコープ外＝loop 変数でない）シンボル基底名を収集する。
pub fn collect_free_syms(e: &Expr, scope: &mut Vec<String>, out: &mut Vec<String>) {
    match e {
        Expr::Num(_) => {}
        Expr::Sym { name, .. } => {
            if !scope.iter().any(|s| s == name) {
                out.push(name.clone());
            }
        }
        Expr::Neg(a) => collect_free_syms(a, scope, out),
        Expr::Bin(_, a, b) => {
            collect_free_syms(a, scope, out);
            collect_free_syms(b, scope, out);
        }
        Expr::Func(_, args) => {
            for a in args {
                collect_free_syms(a, scope, out);
            }
        }
        Expr::Sum(iters, body) => {
            // 集合名はループ変数のスコープとは独立した「参照」なので、スコープに
            // 積む前に free symbol として登録する。これにより parser.rs の未知
            // シンボル検証（`known` に model.sets.keys() を含む）が typo'd な集合名
            // （例: `sum{i in Itemz}` の `Itemz`）を捕捉できるようになる。
            // 修正前は集合名がどこにも記録されず、評価時に
            // `ctx.sets.get(name).unwrap_or_default()` が空集合を返して sum が
            // 黙って 0 になっていた（README の「サイレントエラー禁止」保証に反する）。
            // 添字トークン（`x[i]` の `i`）はここでは検証しない（`x[A]` のような
            // リテラル集合要素と区別できないため。Fase2 の既知の限界）。
            for (_, set_ref) in iters {
                if let SetRef::Named(name) = set_ref {
                    out.push(name.clone());
                }
            }
            let n = scope.len();
            for (v, _) in iters {
                scope.push(v.clone());
            }
            collect_free_syms(body, scope, out);
            scope.truncate(n);
        }
        Expr::Agg(_, iters, body) => {
            // Expr::Sum と全く同じ扱い（#13）: 集合名は free symbol として登録し、
            // ループ変数は本体パースの間だけスコープに積む。これにより
            // `max(i in Itemz) ...` の未知集合名 typo も sum 同様に検出できる。
            for (_, set_ref) in iters {
                if let SetRef::Named(name) = set_ref {
                    out.push(name.clone());
                }
            }
            let n = scope.len();
            for (v, _) in iters {
                scope.push(v.clone());
            }
            collect_free_syms(body, scope, out);
            scope.truncate(n);
        }
        Expr::If(c, a, b) => {
            collect_free_syms(&c.lhs, scope, out);
            collect_free_syms(&c.rhs, scope, out);
            collect_free_syms(a, scope, out);
            collect_free_syms(b, scope, out);
        }
    }
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
                Op::Add => av + bv,
                Op::Sub => av - bv,
                Op::Mul => av * bv,
                Op::Div => {
                    if bv.abs() < 1e-12 {
                        0.0
                    } else {
                        av / bv
                    }
                }
                Op::Pow => {
                    let r = av.powf(bv);
                    if r.is_finite() {
                        r
                    } else {
                        0.0
                    }
                }
            }
        }
        Expr::Func(f, args) => eval_func(*f, args, x, env, ctx),
        Expr::Sum(iters, body) => eval_sum(iters, body, x, env, ctx),
        Expr::Agg(op, iters, body) => eval_agg(*op, iters, body, x, env, ctx),
        Expr::If(c, a, b) => {
            if eval_cond(c, x, env, ctx) {
                eval(a, x, env, ctx)
            } else {
                eval(b, x, env, ctx)
            }
        }
    }
}

fn eval_cond(c: &Cond, x: &[f64], env: &HashMap<String, String>, ctx: &Ctx) -> bool {
    let a = eval(&c.lhs, x, env, ctx);
    let b = eval(&c.rhs, x, env, ctx);
    match c.cmp {
        Cmp::Lt => a < b,
        Cmp::Le => a <= b,
        Cmp::Gt => a > b,
        Cmp::Ge => a >= b,
        Cmp::Eq => (a - b).abs() < 1e-9,
        Cmp::Ne => (a - b).abs() >= 1e-9,
    }
}

fn eval_func(f: Func, args: &[Expr], x: &[f64], env: &HashMap<String, String>, ctx: &Ctx) -> f64 {
    let a = |k: usize| eval(&args[k], x, env, ctx);
    match f {
        Func::Min if args.len() == 2 => a(0).min(a(1)),
        Func::Max if args.len() == 2 => a(0).max(a(1)),
        Func::Abs if args.len() == 1 => a(0).abs(),
        Func::Sqrt if args.len() == 1 => {
            let v = a(0);
            if v < 0.0 {
                0.0
            } else {
                v.sqrt()
            }
        }
        Func::Exp if args.len() == 1 => a(0).exp(),
        Func::Log if args.len() == 1 => {
            let v = a(0);
            if v <= 0.0 {
                0.0
            } else {
                v.ln()
            }
        }
        Func::Pow if args.len() == 2 => {
            let r = a(0).powf(a(1));
            if r.is_finite() {
                r
            } else {
                0.0
            }
        }
        _ => 0.0,
    }
}

fn eval_sym(
    name: &str,
    idx: &[String],
    x: &[f64],
    env: &HashMap<String, String>,
    ctx: &Ctx,
) -> f64 {
    if idx.is_empty() {
        if let Some(m) = ctx.params.get(name) {
            if let Some(v) = m.get("_") {
                return *v;
            }
        }
        if let Some(i) = ctx.var_map.get(name) {
            return x.get(*i).copied().unwrap_or(0.0);
        }
        if let Some(sv) = env.get(name) {
            if let Ok(v) = sv.parse::<f64>() {
                return v;
            }
        }
        return 0.0;
    }
    let key: Vec<String> = idx.iter().map(|t| resolve_index_token(t, env)).collect();
    let k = key.join(",");
    let vk = format!("{}[{}]", name, k);
    if let Some(i) = ctx.var_map.get(&vk) {
        return x.get(*i).copied().unwrap_or(0.0);
    }
    if let Some(m) = ctx.params.get(name) {
        if let Some(v) = m.get(&k) {
            return *v;
        }
    }
    0.0
}

fn eval_sum(
    iters: &[(String, SetRef)],
    body: &Expr,
    x: &[f64],
    env: &HashMap<String, String>,
    ctx: &Ctx,
) -> f64 {
    let mut acc = 0.0;
    let mut e2 = env.clone();
    sum_rec(iters, 0, body, x, &mut e2, ctx, &mut acc);
    acc
}

#[allow(clippy::too_many_arguments)]
fn sum_rec(
    iters: &[(String, SetRef)],
    k: usize,
    body: &Expr,
    x: &[f64],
    env: &mut HashMap<String, String>,
    ctx: &Ctx,
    acc: &mut f64,
) {
    if k == iters.len() {
        *acc += eval(body, x, env, ctx);
        return;
    }
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

fn eval_agg(
    op: AggOp,
    iters: &[(String, SetRef)],
    body: &Expr,
    x: &[f64],
    env: &HashMap<String, String>,
    ctx: &Ctx,
) -> f64 {
    let mut acc: Option<f64> = None;
    let mut e2 = env.clone();
    agg_rec(op, iters, 0, body, x, &mut e2, ctx, &mut acc);
    // NOTE: 反復集合（の直積）が空の場合、min/max は数学的に未定義。
    // sum が空集合で 0 を返す挙動に合わせ、ここも 0.0 を返す（#13 の仕様）。
    acc.unwrap_or(0.0)
}

#[allow(clippy::too_many_arguments)]
fn agg_rec(
    op: AggOp,
    iters: &[(String, SetRef)],
    k: usize,
    body: &Expr,
    x: &[f64],
    env: &mut HashMap<String, String>,
    ctx: &Ctx,
    acc: &mut Option<f64>,
) {
    if k == iters.len() {
        let v = eval(body, x, env, ctx);
        *acc = Some(match *acc {
            None => v,
            Some(cur) => match op {
                AggOp::Min => cur.min(v),
                AggOp::Max => cur.max(v),
            },
        });
        return;
    }
    let (ref var, ref sref) = iters[k];
    let vals: Vec<String> = match sref {
        SetRef::Named(s) => ctx.sets.get(s).cloned().unwrap_or_default(),
        SetRef::Range(a, b) => (*a..=*b).map(|v| v.to_string()).collect(),
    };
    for v in vals {
        env.insert(var.clone(), v);
        agg_rec(op, iters, k + 1, body, x, env, ctx, acc);
    }
    env.remove(var);
}

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
        let ctx = Ctx {
            var_map: &vm,
            params: &params,
            sets: &sets,
        };
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

    /// Issue #3: ユーザー定義関数呼び出し `total_cost(i)` は明快なエラーにする。
    #[test]
    fn user_defined_function_call_errors_clearly() {
        let err = compile("total_cost(i)").unwrap_err();
        assert!(
            err.contains("unknown function") && err.contains("total_cost"),
            "should name the unknown function, got: {}",
            err
        );
    }

    /// Issue #9: 添字算術 `t-1` / `t+1` が env のループ変数を解決してオフセット適用される。
    #[test]
    fn subscript_arithmetic_resolves_offset() {
        let sets = HashMap::new();
        let params = HashMap::new();
        // 変数 x[1], x[2], x[3]
        let mut vm = HashMap::new();
        vm.insert("x[1]".to_string(), 0usize);
        vm.insert("x[2]".to_string(), 1usize);
        vm.insert("x[3]".to_string(), 2usize);
        let x = [10.0, 20.0, 30.0];
        let ctx = Ctx {
            var_map: &vm,
            params: &params,
            sets: &sets,
        };
        // t=3 のとき x[t-1] は x[2]=20
        let mut env = HashMap::new();
        env.insert("t".to_string(), "3".to_string());
        assert_eq!(eval(&compile("x[t-1]").unwrap(), &x, &env, &ctx), 20.0);
        // t=1 のとき x[t+1] は x[2]=20
        env.insert("t".to_string(), "1".to_string());
        assert_eq!(eval(&compile("x[t+1]").unwrap(), &x, &env, &ctx), 20.0);
        // オフセット無しの従来の添字も維持（回帰）
        env.insert("t".to_string(), "3".to_string());
        assert_eq!(eval(&compile("x[t]").unwrap(), &x, &env, &ctx), 30.0);
    }

    /// Issue #9: `base ± 整数リテラル` を超える複雑な添字式は明示エラー。
    #[test]
    fn complex_subscript_expressions_error() {
        assert!(compile("x[t*2]").is_err(), "multiplication in subscript");
        assert!(compile("x[t-s]").is_err(), "identifier offset in subscript");
        assert!(compile("x[t+1.5]").is_err(), "non-integer offset in subscript");
    }

    #[test]
    fn split_index_offset_parses_arithmetic() {
        assert_eq!(super::split_index_offset("t-1"), Some(("t", -1)));
        assert_eq!(super::split_index_offset("t+2"), Some(("t", 2)));
        assert_eq!(super::split_index_offset("t"), None);
        assert_eq!(super::split_index_offset("3"), None);
    }

    /// Issue #10: 単独条件のコンパイルと評価（where フィルタで使う）。
    #[test]
    fn compile_and_eval_condition() {
        let sets = HashMap::new();
        let mut pm = HashMap::new();
        pm.insert("A".to_string(), 5.0);
        let mut params = HashMap::new();
        params.insert("p".to_string(), pm);
        let vm = HashMap::new();
        let ctx = Ctx {
            var_map: &vm,
            params: &params,
            sets: &sets,
        };
        let mut env = HashMap::new();
        env.insert("i".to_string(), "A".to_string());
        // p[i] > 0 with i=A, p[A]=5 => true
        let c = compile_cond("p[i] > 0").unwrap();
        assert!(eval_cond_now(&c, &[], &env, &ctx));
        // p[i] > 10 => false
        let c2 = compile_cond("p[i] > 10").unwrap();
        assert!(!eval_cond_now(&c2, &[], &env, &ctx));
        // 条件でない式はエラー
        assert!(compile_cond("p[i] + 1").is_err());
    }

    #[test]
    fn wrong_arity_is_compile_error() {
        assert!(compile("sqrt(1,2)").is_err());
        assert!(compile("min(1)").is_err());
    }

    #[test]
    fn sum_over_set_with_params_and_vars() {
        // objective: sum{i in S} p[i]*x[i], S={1,2,3}, p={10,40,30}, x=[1,0,1]
        let mut sets = HashMap::new();
        sets.insert("S".to_string(), vec!["1".into(), "2".into(), "3".into()]);
        let mut params = HashMap::new();
        let mut p = HashMap::new();
        p.insert("1".into(), 10.0);
        p.insert("2".into(), 40.0);
        p.insert("3".into(), 30.0);
        params.insert("p".to_string(), p);
        let mut vm = HashMap::new();
        vm.insert("x[1]".to_string(), 0usize);
        vm.insert("x[2]".to_string(), 1usize);
        vm.insert("x[3]".to_string(), 2usize);
        let x = [1.0, 0.0, 1.0];
        let env = HashMap::new();
        let ctx = Ctx {
            var_map: &vm,
            params: &params,
            sets: &sets,
        };
        let e = compile("sum{i in S} p[i] * x[i]").unwrap();
        assert_eq!(eval(&e, &x, &env, &ctx), 40.0); // 10*1 + 40*0 + 30*1
    }

    #[test]
    fn collect_free_syms_includes_sum_set_name() {
        // 回帰テスト（Fix 2）: `sum{i in Itemz}` の集合名 `Itemz` が free symbol として
        // 収集されること。修正前は集合名がどこにも記録されず、parser.rs の未知シンボル
        // 検証をすり抜けて typo'd な集合名（例: `Items` の typo `Itemz`）が黙って空集合
        // として評価され、sum が黙って 0 になっていた。
        let e = compile("sum{i in Itemz} x[i]").unwrap();
        let mut scope = Vec::new();
        let mut free = Vec::new();
        collect_free_syms(&e, &mut scope, &mut free);
        assert!(
            free.contains(&"Itemz".to_string()),
            "expected sum's set name 'Itemz' to appear in free syms, got {:?}",
            free
        );
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
        let ctx = Ctx {
            var_map: &vm,
            params: &params,
            sets: &sets,
        };
        let e = compile("sum(i in S) x[i] + sum(i in S) x[i]").unwrap();
        assert_eq!(eval(&e, &x, &env, &ctx), 4.0);
    }

    // ---- #13: aggregate max(i in S)/min(i in S) reductions ----

    #[test]
    fn aggregate_max_min_over_set() {
        // S = {1,2,3}, x = [2,9,4] -> max=9, min=2
        let mut sets = HashMap::new();
        sets.insert("S".to_string(), vec!["1".into(), "2".into(), "3".into()]);
        let mut vm = HashMap::new();
        vm.insert("x[1]".to_string(), 0usize);
        vm.insert("x[2]".to_string(), 1usize);
        vm.insert("x[3]".to_string(), 2usize);
        let x = [2.0, 9.0, 4.0];
        let env = HashMap::new();
        let params = HashMap::new();
        let ctx = Ctx {
            var_map: &vm,
            params: &params,
            sets: &sets,
        };

        let e_max = compile("max(i in S) x[i]").unwrap();
        assert_eq!(eval(&e_max, &x, &env, &ctx), 9.0);

        let e_min = compile("min(i in S) x[i]").unwrap();
        assert_eq!(eval(&e_min, &x, &env, &ctx), 2.0);
    }

    #[test]
    fn aggregate_body_with_arithmetic() {
        // max(i in S) (x[i] + 1), S={1,2,3}, x=[2,9,4] -> max(3,10,5) = 10
        let mut sets = HashMap::new();
        sets.insert("S".to_string(), vec!["1".into(), "2".into(), "3".into()]);
        let mut vm = HashMap::new();
        vm.insert("x[1]".to_string(), 0usize);
        vm.insert("x[2]".to_string(), 1usize);
        vm.insert("x[3]".to_string(), 2usize);
        let x = [2.0, 9.0, 4.0];
        let env = HashMap::new();
        let params = HashMap::new();
        let ctx = Ctx {
            var_map: &vm,
            params: &params,
            sets: &sets,
        };
        let e = compile("max(i in S) (x[i] + 1)").unwrap();
        assert_eq!(eval(&e, &x, &env, &ctx), 10.0);
    }

    #[test]
    fn aggregate_nested_min_max() {
        // max(i in S) min(j in T) p[i, j]
        // S=T={1,2}, p[1,1]=5 p[1,2]=1 p[2,1]=3 p[2,2]=8
        // -> min over j: i=1 -> 1, i=2 -> 3; max over i: 3
        let mut sets = HashMap::new();
        sets.insert("S".to_string(), vec!["1".into(), "2".into()]);
        sets.insert("T".to_string(), vec!["1".into(), "2".into()]);
        let mut params = HashMap::new();
        let mut p = HashMap::new();
        p.insert("1,1".into(), 5.0);
        p.insert("1,2".into(), 1.0);
        p.insert("2,1".into(), 3.0);
        p.insert("2,2".into(), 8.0);
        params.insert("p".to_string(), p);
        let vm = HashMap::new();
        let x: [f64; 0] = [];
        let env = HashMap::new();
        let ctx = Ctx {
            var_map: &vm,
            params: &params,
            sets: &sets,
        };
        let e = compile("max(i in S) min(j in T) p[i, j]").unwrap();
        assert_eq!(eval(&e, &x, &env, &ctx), 3.0);
    }

    #[test]
    fn aggregate_empty_set_returns_zero() {
        // 反復集合が空の場合、min/max は未定義。sum の空集合=0 の挙動に合わせ 0.0 を返す。
        let mut sets = HashMap::new();
        sets.insert("Empty".to_string(), Vec::<String>::new());
        let vm = HashMap::new();
        let params = HashMap::new();
        let x: [f64; 0] = [];
        let env = HashMap::new();
        let ctx = Ctx {
            var_map: &vm,
            params: &params,
            sets: &sets,
        };
        let e = compile("max(i in Empty) i").unwrap();
        assert_eq!(eval(&e, &x, &env, &ctx), 0.0);
    }

    #[test]
    fn aggregate_vs_two_arg_function_do_not_cross_trigger() {
        // 2引数関数形は引き続き動作し、集約と誤認されないこと（#13 の要）。
        assert_eq!(ev("max(2, 7)"), 7.0);
        assert_eq!(ev("min(2, 7)"), 2.0);
        // 第1引数が裸の識別子でも（2番目のトークンが `in` ではなく `,`）関数形と判定される。
        assert!(compile("max(a, b)").is_ok());
        assert_eq!(ev("max(a, b)"), 0.0); // a, b は未定義シンボル -> 0 にフォールバック
                                          // 第1引数が添字付きシンボルでも（2番目のトークンが `[`）関数形と判定される。
        assert!(compile("max(p[1], q[2])").is_ok());
        // 集約形は引き続き2トークン先読みで正しく判定される。
        assert!(compile("max(i in S) x[i]").is_ok());
    }

    #[test]
    fn collect_free_syms_includes_aggregate_set_name() {
        // 回帰テスト（#13）: sum と同様、集約 min/max のヘッダ集合名も free symbol として
        // 収集され、typo'd な集合名が未知シンボル検証で捕捉できること。
        let e = compile("max(i in Itemz) x[i]").unwrap();
        let mut scope = Vec::new();
        let mut free = Vec::new();
        collect_free_syms(&e, &mut scope, &mut free);
        assert!(
            free.contains(&"Itemz".to_string()),
            "expected aggregate's set name 'Itemz' to appear in free syms, got {:?}",
            free
        );
    }

    #[test]
    fn issue13_nested_max_min_from_moo_supply_chain_parses() {
        // examples/11_moo_supply_chain.optica の max_lead_time 目的関数と同じ形。
        // 修正前は `max`/`min` が2引数関数専用だったため
        // "parse error: expected RPar" になっていた。集約対応後はパースが成功すること
        // （このオブジェクトの意味論的な妥当性は本 issue のスコープ外）。
        let src = "max(c in CUSTOMERS)\n    min(s in SUPPLIERS, f in FACTORIES, w in WAREHOUSES)\n        lead_time[s, f] + production_time[f] + transport_time[f, w] + delivery_time[w, c]";
        let result = compile(src);
        assert!(
            result.is_ok(),
            "expected nested aggregate max/min to parse without RPar error, got {:?}",
            result
        );
    }
}
