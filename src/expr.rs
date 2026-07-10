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

#[derive(Debug, Clone)]
pub enum SetRef {
    Named(String),
    #[allow(dead_code)]
    Range(i64, i64),
}

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
            "min" | "max" | "abs" | "sqrt" | "exp" | "log" | "pow" => {
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
                    "min" => Func::Min,
                    "max" => Func::Max,
                    "abs" => Func::Abs,
                    "sqrt" => Func::Sqrt,
                    "exp" => Func::Exp,
                    "log" => Func::Log,
                    "pow" => Func::Pow,
                    _ => unreachable!(),
                };
                let expected_arity = match f {
                    Func::Min | Func::Max | Func::Pow => 2,
                    Func::Abs | Func::Sqrt | Func::Exp | Func::Log => 1,
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

pub fn compile(src: &str) -> Result<Expr, String> {
    let toks = lex(src)?;
    let mut p = P { t: toks, i: 0 };
    let e = p.parse_expr(0)?;
    if p.i != p.t.len() {
        return Err(format!("trailing tokens from position {}", p.i));
    }
    Ok(e)
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
    let key: Vec<String> = idx
        .iter()
        .map(|t| env.get(t).cloned().unwrap_or_else(|| t.clone()))
        .collect();
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
}
