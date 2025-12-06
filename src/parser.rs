//! Optica言語パーサー（拡張版）

use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// パース済みモデル
#[derive(Debug, Clone)]
pub struct Model {
    pub dim: usize,
    pub lb: Vec<f64>,
    pub ub: Vec<f64>,
    pub var_names: Vec<String>,
    pub var_map: HashMap<String, usize>, // 変数名 -> インデックス
    pub maximize: bool,
    pub params: HashMap<String, HashMap<String, f64>>, // パラメータ値
    pub sets: HashMap<String, Vec<String>>,            // 集合
    pub objective_expr: Option<String>,                // 目的関数式
    pub constraints: Vec<Constraint>,                  // 制約
    pub objectives: Vec<Objective>,                    // 多目的
    pub pareto: ParetoMethod,
    pub cp_globals: Vec<String>, // CPグローバル制約（no_overlap, disjunctive, cumulative）
}

#[derive(Debug, Clone)]
pub struct Constraint {
    #[allow(dead_code)]
    pub name: String,
    pub expr: String,
    pub op: ConstraintOp,
    pub rhs: f64,
}

#[derive(Debug, Clone, Copy)]
pub enum ConstraintOp {
    Le, // <=
    Ge, // >=
    Eq, // ==
}

#[derive(Debug, Clone)]
pub struct Objective {
    pub name: String,
    pub expr: String,
    pub maximize: bool,
}

#[derive(Debug, Clone)]
pub enum ParetoMethod {
    Single,
    WeightedSum(Vec<(String, f64)>), // (name, weight)
    Epsilon {
        primary: String,
        eps: Vec<(String, ConstraintOp, f64)>,
    },
}

impl Model {
    pub fn new() -> Self {
        Self {
            dim: 0,
            lb: Vec::new(),
            ub: Vec::new(),
            var_names: Vec::new(),
            var_map: HashMap::new(),
            maximize: false,
            params: HashMap::new(),
            sets: HashMap::new(),
            objective_expr: None,
            constraints: Vec::new(),
            objectives: Vec::new(),
            pareto: ParetoMethod::Single,
            cp_globals: Vec::new(),
        }
    }

    /// 目的関数を評価
    pub fn evaluate_objective(&self, x: &[f64]) -> f64 {
        // 単一目的（従来互換）か、多目的の重み付け/epsilonを後段で処理する
        if let Some(ref expr) = self.objective_expr {
            self.evaluate_expr(expr, x, &HashMap::new())
        } else if !self.objectives.is_empty() {
            // 一旦最初の目的を返す（互換のため）。実際の組み合わせはcompute_fitness側で処理。
            let expr = &self.objectives[0].expr;
            self.evaluate_expr(expr, x, &HashMap::new())
        } else {
            // デフォルト: Sphere関数
            x.iter().map(|&v| v * v).sum()
        }
    }

    /// 制約違反をチェック
    pub fn check_constraints(&self, x: &[f64]) -> (bool, f64) {
        let mut feasible = true;
        let mut total_violation = 0.0;

        for constraint in &self.constraints {
            let lhs = self.evaluate_expr(&constraint.expr, x, &HashMap::new());
            let v = match constraint.op {
                ConstraintOp::Le => (lhs - constraint.rhs).max(0.0),
                ConstraintOp::Ge => (constraint.rhs - lhs).max(0.0),
                ConstraintOp::Eq => (lhs - constraint.rhs).abs(),
            };
            if v > 1e-9 {
                feasible = false;
                total_violation += v;
            }
        }

        (feasible, total_violation)
    }

    /// 式を評価（簡易版）
    pub fn evaluate_expr(&self, expr: &str, x: &[f64], env: &HashMap<String, String>) -> f64 {
        let expr = expr.trim();

        // if-then-else
        if let Some(val) = self.eval_if(expr, x, env) {
            return val;
        }

        // sum(...)
        if expr.starts_with("sum(") || expr.starts_with("sum{") {
            return self.evaluate_sum(expr, x, env);
        }

        // 比較（条件用）
        if let Some(val) = self.eval_comparison(expr, x, env) {
            return val;
        }

        // 四則演算
        self.eval_arith(expr, x, env)
    }

    fn eval_if(&self, expr: &str, x: &[f64], env: &HashMap<String, String>) -> Option<f64> {
        let lower = expr.to_lowercase();
        if let (Some(t_pos), Some(e_pos)) = (lower.find(" then "), lower.find(" else ")) {
            let cond_str = &expr[..t_pos];
            let then_str = &expr[t_pos + 6..e_pos];
            let else_str = &expr[e_pos + 6..];
            let cond_val = self.eval_condition(cond_str.trim(), x, env);
            if cond_val {
                Some(self.evaluate_expr(then_str.trim(), x, env))
            } else {
                Some(self.evaluate_expr(else_str.trim(), x, env))
            }
        } else {
            None
        }
    }

    fn eval_condition(&self, cond: &str, x: &[f64], env: &HashMap<String, String>) -> bool {
        // サポート: <, <=, >, >=, ==, !=
        let ops = ["<=", ">=", "==", "!=", "<", ">"];
        for op in ops {
            if let Some(pos) = cond.find(op) {
                let lhs = cond[..pos].trim();
                let rhs = cond[pos + op.len()..].trim();
                let a = self.evaluate_expr(lhs, x, env);
                let b = self.evaluate_expr(rhs, x, env);
                return match op {
                    "<" => a < b,
                    "<=" => a <= b,
                    ">" => a > b,
                    ">=" => a >= b,
                    "==" => (a - b).abs() < 1e-9,
                    "!=" => (a - b).abs() >= 1e-9,
                    _ => false,
                };
            }
        }
        self.evaluate_expr(cond, x, env) != 0.0
    }

    fn eval_comparison(&self, expr: &str, x: &[f64], env: &HashMap<String, String>) -> Option<f64> {
        let ops = ["<=", ">=", "==", "!=", "<", ">"];
        for op in ops {
            if let Some(pos) = expr.find(op) {
                let lhs = expr[..pos].trim();
                let rhs = expr[pos + op.len()..].trim();
                let a = self.evaluate_expr(lhs, x, env);
                let b = self.evaluate_expr(rhs, x, env);
                let res = match op {
                    "<" => a < b,
                    "<=" => a <= b,
                    ">" => a > b,
                    ">=" => a >= b,
                    "==" => (a - b).abs() < 1e-9,
                    "!=" => (a - b).abs() >= 1e-9,
                    _ => false,
                };
                return Some(if res { 1.0 } else { 0.0 });
            }
        }
        None
    }

    fn eval_arith(&self, expr: &str, x: &[f64], env: &HashMap<String, String>) -> f64 {
        // 逆ポーランドへの簡易変換（+ - * / と括弧、単項-）
        #[derive(Debug, Clone)]
        enum Tok {
            Num(f64),
            Sym(String),
            Op(char),
            LPar,
            RPar,
            Comma,
        }
        fn prec(op: char) -> i32 {
            match op {
                '+' | '-' => 1,
                '*' | '/' => 2,
                _ => 0,
            }
        }
        // トークナイズ
        let mut toks: Vec<Tok> = Vec::new();
        let mut i = 0;
        let bytes = expr.as_bytes();
        while i < bytes.len() {
            let c = bytes[i] as char;
            if c.is_whitespace() {
                i += 1;
                continue;
            }
            if c.is_ascii_digit() || c == '.' {
                let start = i;
                i += 1;
                while i < bytes.len()
                    && ((bytes[i] as char).is_ascii_digit() || bytes[i] as char == '.')
                {
                    i += 1;
                }
                let s = &expr[start..i];
                if let Ok(v) = s.parse::<f64>() {
                    toks.push(Tok::Num(v));
                }
                continue;
            }
            if c == '(' {
                toks.push(Tok::LPar);
                i += 1;
                continue;
            }
            if c == ')' {
                toks.push(Tok::RPar);
                i += 1;
                continue;
            }
            if c == ',' {
                toks.push(Tok::Comma);
                i += 1;
                continue;
            }
            if "+-*/".contains(c) {
                toks.push(Tok::Op(c));
                i += 1;
                continue;
            }
            // identifier or function or symbol with brackets
            let start = i;
            i += 1;
            while i < bytes.len() {
                let ch = bytes[i] as char;
                if ch.is_ascii_alphanumeric() || ch == '_' || ch == '[' || ch == ']' || ch == '.' {
                    i += 1;
                } else {
                    break;
                }
            }
            toks.push(Tok::Sym(expr[start..i].to_string()));
        }

        // Shunting-yard to RPN
        let mut output: Vec<Tok> = Vec::new();
        let mut stack: Vec<Tok> = Vec::new();
        let mut prev_was_op = true;
        for t in toks {
            match t {
                Tok::Num(_) | Tok::Sym(_) => {
                    output.push(t);
                    prev_was_op = false;
                }
                Tok::Op(op) => {
                    let mut op_char = op;
                    // 単項マイナス対応: 前が演算子/左括弧の場合は0を挿入
                    if op == '-' && prev_was_op {
                        output.push(Tok::Num(0.0));
                        op_char = '-';
                    }
                    while let Some(Tok::Op(top)) = stack.last() {
                        if prec(*top) >= prec(op_char) {
                            output.push(stack.pop().unwrap());
                        } else {
                            break;
                        }
                    }
                    stack.push(Tok::Op(op_char));
                    prev_was_op = true;
                }
                Tok::LPar => {
                    stack.push(Tok::LPar);
                    prev_was_op = true;
                }
                Tok::RPar => {
                    while let Some(tok) = stack.pop() {
                        if let Tok::LPar = tok {
                            break;
                        }
                        output.push(tok);
                    }
                    prev_was_op = false;
                }
                Tok::Comma => {
                    // treat as low-precedence separator
                    while let Some(tok) = stack.last() {
                        if let Tok::LPar = tok {
                            break;
                        }
                        output.push(stack.pop().unwrap());
                    }
                    prev_was_op = true;
                }
            }
        }
        while let Some(tok) = stack.pop() {
            output.push(tok);
        }

        // Evaluate RPN
        let mut st: Vec<f64> = Vec::new();
        for tok in output {
            match tok {
                Tok::Num(v) => st.push(v),
                Tok::Sym(s) => {
                    // max/min functions with one comma arg are handled by Op + special names; but here treat as symbol
                    let v = self.eval_symbol(&s, x, env);
                    st.push(v);
                }
                Tok::Op(op) => {
                    if st.len() < 2 {
                        return 0.0;
                    }
                    let b = st.pop().unwrap();
                    let a = st.pop().unwrap();
                    let v = match op {
                        '+' => a + b,
                        '-' => a - b,
                        '*' => a * b,
                        '/' => {
                            if b.abs() < 1e-12 {
                                0.0
                            } else {
                                a / b
                            }
                        }
                        _ => 0.0,
                    };
                    st.push(v);
                }
                _ => {}
            }
        }
        st.pop().unwrap_or(0.0)
    }

    fn eval_symbol(&self, sym: &str, x: &[f64], env: &HashMap<String, String>) -> f64 {
        // max(...) / min(...)
        if sym.starts_with("max(") && sym.ends_with(')') {
            let inner = &sym[4..sym.len() - 1];
            let parts: Vec<&str> = inner.split(',').collect();
            if parts.len() == 2 {
                let a = self.evaluate_expr(parts[0].trim(), x, env);
                let b = self.evaluate_expr(parts[1].trim(), x, env);
                return a.max(b);
            }
        }
        if sym.starts_with("min(") && sym.ends_with(')') {
            let inner = &sym[4..sym.len() - 1];
            let parts: Vec<&str> = inner.split(',').collect();
            if parts.len() == 2 {
                let a = self.evaluate_expr(parts[0].trim(), x, env);
                let b = self.evaluate_expr(parts[1].trim(), x, env);
                return a.min(b);
            }
        }

        // identifier with optional [..]
        if let Some(b) = sym.find('[') {
            let name = &sym[..b];
            let idx_part = sym[b + 1..].trim_end_matches(']');
            let idx_tokens: Vec<String> = idx_part
                .split(',')
                .map(|t| t.trim())
                .map(|t| env.get(t).cloned().unwrap_or_else(|| t.to_string()))
                .collect();
            let idx_key = idx_tokens.join(",");

            // var
            let var_key = format!("{}[{}]", name, idx_key);
            if let Some(idx) = self.var_map.get(&var_key) {
                return x[*idx];
            }

            // param
            if let Some(param_map) = self.params.get(name) {
                if let Some(v) = param_map.get(&idx_key) {
                    return *v;
                }
            }
            return 0.0;
        }

        // スカラーパラメータ
        if let Some(param_map) = self.params.get(sym) {
            if let Some(v) = param_map.get("_") {
                return *v;
            }
        }

        // 変数
        if let Some(idx) = self.var_map.get(sym) {
            return x[*idx];
        }

        // 環境（インデックス値を数値化可能なら）
        if let Some(sv) = env.get(sym) {
            if let Ok(v) = sv.parse::<f64>() {
                return v;
            }
        }

        0.0
    }

    /// sum式を評価
    fn evaluate_sum(&self, expr: &str, x: &[f64], env: &HashMap<String, String>) -> f64 {
        // 形式: sum(i in SET, j in SET2) body
        let (header, body) = if let Some(start) = expr.find('(') {
            if let Some(end) = expr.find(')') {
                (&expr[start + 1..end], expr[end + 1..].trim())
            } else {
                return 0.0;
            }
        } else if let Some(start) = expr.find('{') {
            if let Some(end) = expr.find('}') {
                (&expr[start + 1..end], expr[end + 1..].trim())
            } else {
                return 0.0;
            }
        } else {
            return 0.0;
        };

        let mut loops: Vec<(String, Vec<String>)> = Vec::new();
        for part in header.split(',') {
            if let Some(pos) = part.find(" in ") {
                let var = part[..pos].trim().to_string();
                let set_name = part[pos + 4..].trim();
                let vals = if let Some(set) = self.sets.get(set_name) {
                    set.clone()
                } else if let Some(dd) = set_name.find("..") {
                    let a = set_name[..dd].trim().parse::<i32>().unwrap_or(0);
                    let b = set_name[dd + 2..].trim().parse::<i32>().unwrap_or(0);
                    (a..=b).map(|v| v.to_string()).collect()
                } else {
                    vec![set_name.to_string()]
                };
                loops.push((var, vals));
            }
        }

        let mut total = 0.0;
        fn dfs(
            model: &Model,
            loops: &[(String, Vec<String>)],
            idx: usize,
            env: &mut HashMap<String, String>,
            body: &str,
            x: &[f64],
            acc: &mut f64,
        ) {
            if idx == loops.len() {
                *acc += model.evaluate_expr(body, x, env);
                return;
            }
            let (ref var, ref vals) = loops[idx];
            for v in vals {
                env.insert(var.clone(), v.clone());
                dfs(model, loops, idx + 1, env, body, x, acc);
            }
            env.remove(var);
        }
        let mut env2 = env.clone();
        dfs(self, &loops, 0, &mut env2, body, x, &mut total);
        total
    }
}

/// ソースコードをパース
pub fn parse(source: &str) -> Result<Model, String> {
    let mut model = Model::new();
    let mut in_subject_to = false;
    let mut in_data = false;
    let mut in_objectives = false;
    let mut weights: Vec<(String, f64)> = Vec::new();
    let mut eps_constraints: Vec<(String, ConstraintOp, f64)> = Vec::new();
    let mut primary_obj: Option<String> = None;
    let mut pareto_mode: Option<String> = None;

    for line in source.lines() {
        let line = line.trim();

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

        if line.starts_with("objectives:") {
            in_objectives = true;
            in_subject_to = false;
            continue;
        }

        if line.starts_with("data:") {
            in_data = true;
            continue;
        }

        // dataブロックの処理
        if in_data {
            // dataブロック終了条件: 空行や次のセクション開始
            if line.is_empty()
                || line.starts_with("param ")
                || line.starts_with("var ")
                || line.starts_with("set ")
                || line.starts_with("subject to")
                || line.starts_with("maximize")
                || line.starts_with("minimize")
            {
                in_data = false;
                // この行を再処理するためにfall-through
            } else {
                parse_data_assignment(line, &mut model.params)?;
                continue;
            }
        }

        if in_objectives {
            if line.starts_with("subject to") {
                in_objectives = false;
                in_subject_to = true;
                // decide pareto method
                if let Some(mode) = pareto_mode.clone() {
                    if mode == "weighted_sum" && !weights.is_empty() {
                        model.pareto = ParetoMethod::WeightedSum(weights.clone());
                    } else if mode == "epsilon_constraint" {
                        if let Some(p) = primary_obj.clone() {
                            model.pareto = ParetoMethod::Epsilon {
                                primary: p,
                                eps: eps_constraints.clone(),
                            };
                        }
                    }
                }
                continue;
            }
            // pareto method lines areしばしばobjectivesブロック内にある
            if line.starts_with("pareto method:") {
                if line.contains("weighted_sum") {
                    pareto_mode = Some("weighted_sum".to_string());
                } else if line.contains("epsilon_constraint") {
                    pareto_mode = Some("epsilon_constraint".to_string());
                }
                continue;
            }
            if pareto_mode.as_deref() == Some("weighted_sum") && line.starts_with("weight ") {
                if let Some(colon) = line.find(':') {
                    let name = line[7..colon].trim().to_string();
                    let val = line[colon + 1..].trim().parse::<f64>().unwrap_or(0.0);
                    weights.push((name, val));
                }
                continue;
            }
            if pareto_mode.as_deref() == Some("epsilon_constraint") {
                if let Some(rest) = line.strip_prefix("primary:") {
                    primary_obj = Some(rest.trim().to_string());
                    continue;
                }
                if line.starts_with("epsilon:") {
                    continue;
                }
                if line.contains("<=") {
                    let s = line.replace(':', "");
                    if let Some(op_pos) = s.find("<=") {
                        let name = s[..op_pos].trim().to_string();
                        let rhs = s[op_pos + 2..].trim().parse::<f64>().unwrap_or(0.0);
                        eps_constraints.push((name, ConstraintOp::Le, rhs));
                        continue;
                    }
                }
            }
            if line.starts_with("maximize") || line.starts_with("minimize") {
                // 多目的: とりあえず最初の目的だけを採用
                let mut maximize = false;
                if line.starts_with("maximize") {
                    maximize = true;
                }
                let (name, expr) = parse_objective_named(line);
                model.objectives.push(Objective {
                    name: name.clone(),
                    expr,
                    maximize,
                });
                if model.objective_expr.is_none() {
                    model.maximize = maximize;
                    model.objective_expr = Some(model.objectives.last().unwrap().expr.clone());
                }
            }
            continue;
        }

        // pareto method
        if line.starts_with("pareto method:") {
            if line.contains("weighted_sum") {
                pareto_mode = Some("weighted_sum".to_string());
            } else if line.contains("epsilon_constraint") {
                pareto_mode = Some("epsilon_constraint".to_string());
            }
            continue;
        }
        if line.starts_with("weight ") {
            // weight total_cost: 0.5
            if let Some(colon) = line.find(':') {
                let name = line[7..colon].trim().to_string();
                let val = line[colon + 1..].trim().parse::<f64>().unwrap_or(0.0);
                weights.push((name, val));
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("primary:") {
            primary_obj = Some(rest.trim().to_string());
            continue;
        }
        if line.starts_with("epsilon:") {
            continue;
        }
        if pareto_mode.as_deref() == Some("epsilon_constraint")
            && (line.starts_with("total_co2")
                || line.starts_with("max_lead_time")
                || line.contains("<="))
        {
            // epsilon行
            let s = line.replace(':', "");
            if let Some(op_pos) = s.find("<=") {
                let name = s[..op_pos].trim().to_string();
                let rhs = s[op_pos + 2..].trim().parse::<f64>().unwrap_or(0.0);
                eps_constraints.push((name, ConstraintOp::Le, rhs));
                continue;
            }
        }

        if line.starts_with("set ") {
            parse_set(line, &mut model.sets)?;
        } else if line.starts_with("stage ") {
            parse_stage(line, &mut model.sets)?;
        } else if line.starts_with("state ") {
            let sets = model.sets.clone();
            parse_state_or_decision(line, &mut model, &sets, true)?;
        } else if line.starts_with("decision ") {
            let sets = model.sets.clone();
            parse_state_or_decision(line, &mut model, &sets, false)?;
        } else if line.starts_with("param ") {
            let sets = model.sets.clone();
            parse_param(line, &mut model.params, &sets)?;
        } else if line.starts_with("var ") {
            let sets = model.sets.clone();
            parse_var(line, &mut model, &sets)?;
        } else if line.starts_with("maximize") {
            model.maximize = true;
            parse_objective(line, &mut model)?;
        } else if line.starts_with("minimize") {
            model.maximize = false;
            parse_objective(line, &mut model)?;
        } else if line.starts_with("subject to") {
            in_subject_to = true;
        } else if in_subject_to && !line.is_empty() {
            parse_constraint(line, &mut model)?;
        }
    }

    // 変数マップを構築
    for (i, name) in model.var_names.iter().enumerate() {
        model.var_map.insert(name.clone(), i);
    }

    model.dim = model.lb.len();
    Ok(model)
}

fn expand_indices(idx_list: Vec<&str>, sets: &HashMap<String, Vec<String>>) -> Vec<Vec<String>> {
    let mut values: Vec<Vec<String>> = Vec::new();
    for idx in idx_list {
        if let Some(set) = sets.get(idx) {
            values.push(set.clone());
        } else if let Some(dotdot) = idx.find("..") {
            let start_str = idx[..dotdot].trim();
            let end_str = idx[dotdot + 2..].trim();
            if let (Ok(start), Ok(end)) = (start_str.parse::<i32>(), end_str.parse::<i32>()) {
                let v: Vec<String> = (start..=end).map(|i| i.to_string()).collect();
                values.push(v);
            }
        } else {
            values.push(vec![idx.to_string()]);
        }
    }
    values
}

fn cartesian(lists: &[Vec<String>]) -> Vec<Vec<String>> {
    let mut res: Vec<Vec<String>> = vec![Vec::new()];
    for list in lists {
        let mut next = Vec::new();
        for prefix in &res {
            for v in list {
                let mut p = prefix.clone();
                p.push(v.clone());
                next.push(p);
            }
        }
        res = next;
    }
    res
}

fn parse_set(line: &str, sets: &mut HashMap<String, Vec<String>>) -> Result<(), String> {
    // set Items = {1, 2, 3};
    // set CUSTOMERS = 1..5;
    if let Some(eq) = line.find('=') {
        let name = line[4..eq].trim().to_string();
        let value = line[eq + 1..].trim().trim_end_matches(';');

        // 範囲表記: 1..5
        if let Some(dotdot) = value.find("..") {
            let start_str = value[..dotdot].trim();
            let end_str = value[dotdot + 2..].trim();
            if let (Ok(start), Ok(end)) = (start_str.parse::<i32>(), end_str.parse::<i32>()) {
                let elems: Vec<String> = (start..=end).map(|i| i.to_string()).collect();
                sets.insert(name, elems);
                return Ok(());
            }
        }

        // 集合表記: {1, 2, 3}
        let elems_str = value.trim_matches(|c| c == '{' || c == '}');
        let elems: Vec<String> = elems_str
            .split(',')
            .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
            .filter(|s| !s.is_empty())
            .collect();

        sets.insert(name, elems);
    }
    Ok(())
}

fn parse_param(
    line: &str,
    params: &mut HashMap<String, HashMap<String, f64>>,
    _sets: &HashMap<String, Vec<String>>,
) -> Result<(), String> {
    // param value[Items] = {1: 10, 2: 20};
    // param capacity = 10;
    let line = line.trim_end_matches(';');

    // スカラーパラメータ
    if let Some(eq) = line.find('=') {
        let name_part = line[6..eq].trim();
        let value_str = line[eq + 1..].trim();

        if !name_part.contains('[') {
            // スカラー: param capacity = 10;
            if let Ok(val) = value_str.parse::<f64>() {
                let mut map = HashMap::new();
                map.insert("_".to_string(), val);
                params.insert(name_part.to_string(), map);
            }
            return Ok(());
        }

        // インデックス付き: param value[Items] = {1: 10, 2: 20};
        if let Some(bracket) = name_part.find('[') {
            let name = name_part[..bracket].trim().to_string();
            let _idx_name = name_part[bracket + 1..].trim_end_matches(']').trim();

            let mut map = HashMap::new();
            let value_str = value_str.trim_matches(|c| c == '{' || c == '}');

            for pair in value_str.split(',') {
                let pair = pair.trim();
                if let Some(colon) = pair.find(':') {
                    let key = pair[..colon]
                        .trim()
                        .trim_matches('"')
                        .trim_matches('\'')
                        .to_string();
                    if let Ok(val) = pair[colon + 1..].trim().parse::<f64>() {
                        map.insert(key, val);
                    }
                }
            }

            params.insert(name, map);
        }
    } else {
        // 値なし: param value[Items] real;
        let name_part = line[6..].trim();
        if let Some(bracket) = name_part.find('[') {
            let name = name_part[..bracket].trim().to_string();
            params.insert(name, HashMap::new());
        }
    }

    Ok(())
}

fn parse_data_assignment(
    line: &str,
    params: &mut HashMap<String, HashMap<String, f64>>,
) -> Result<(), String> {
    // 例: capacity = 100, cost[A] = 10
    if let Some(eq) = line.find('=') {
        let name_part = line[..eq].trim();
        let value_str = line[eq + 1..].trim();

        if !name_part.contains('[') {
            if let Ok(val) = value_str.parse::<f64>() {
                let mut map = HashMap::new();
                map.insert("_".to_string(), val);
                params.insert(name_part.to_string(), map);
            }
            return Ok(());
        }

        if let Some(b) = name_part.find('[') {
            let name = name_part[..b].trim().to_string();
            let idx = name_part[b + 1..].trim_end_matches(']').trim().to_string();
            let val = value_str.parse::<f64>().unwrap_or(0.0);
            let entry = params.entry(name).or_default();
            entry.insert(idx, val);
        }
    }
    Ok(())
}

/// JSONファイルからパラメータを読み込む（サイドカー）
pub fn load_json_into(model: &mut Model, path: &Path) -> Result<(), String> {
    let data = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(&data).map_err(|e| e.to_string())?;
    if let Some(obj) = v.as_object() {
        for (pname, val) in obj {
            let entry = model.params.entry(pname.clone()).or_default();
            match val {
                Value::Number(n) => {
                    if let Some(fv) = n.as_f64() {
                        entry.insert("_".to_string(), fv);
                    }
                }
                Value::Object(map) => {
                    for (k, v2) in map {
                        if let Some(fv) = v2.as_f64() {
                            entry.insert(k.clone(), fv);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn parse_var(
    line: &str,
    model: &mut Model,
    sets: &HashMap<String, Vec<String>>,
) -> Result<(), String> {
    // var x[Items] >= 0 <= 10;
    // var y Binary;
    // var z[ITEMS, PERIODS] int;
    let line = &line[4..].trim_end_matches(';');

    let (name, indices) = if let Some(b) = line.find('[') {
        let e = line.find(']').unwrap_or(line.len());
        let indices_str = &line[b + 1..e];

        // 複数インデックス: x[ITEMS, PERIODS]
        let indices: Vec<&str> = indices_str.split(',').map(|s| s.trim()).collect();
        (line[..b].trim(), Some(indices))
    } else {
        (line.split_whitespace().next().unwrap_or(""), None)
    };

    // 境界値の解析
    let (lb, ub) = parse_bounds(line)?;

    // インデックスの展開
    let mut combos: Vec<String> = Vec::new();
    if let Some(idx_list) = indices {
        let values = expand_indices(idx_list, sets);
        for combo in cartesian(&values) {
            combos.push(format!("{}[{}]", name, combo.join(",")));
        }
    } else {
        combos.push(name.to_string());
    }

    for var_name in combos {
        model.lb.push(lb);
        model.ub.push(ub);
        model.var_names.push(var_name);
    }

    Ok(())
}

fn parse_stage(line: &str, sets: &mut HashMap<String, Vec<String>>) -> Result<(), String> {
    // stage t in 1..12;
    let line = line.trim_end_matches(';');

    if let Some(in_pos) = line.find(" in ") {
        let var_name = line[6..in_pos].trim().to_string();
        let range_str = line[in_pos + 4..].trim();

        // 範囲表記: 1..12
        if let Some(dotdot) = range_str.find("..") {
            let start_str = range_str[..dotdot].trim();
            let end_str = range_str[dotdot + 2..].trim();
            if let (Ok(start), Ok(end)) = (start_str.parse::<i32>(), end_str.parse::<i32>()) {
                let elems: Vec<String> = (start..=end).map(|i| i.to_string()).collect();
                sets.insert(var_name, elems);
            }
        }
    }

    Ok(())
}

fn parse_state_or_decision(
    line: &str,
    model: &mut Model,
    sets: &HashMap<String, Vec<String>>,
    is_state: bool,
) -> Result<(), String> {
    // state S[t] in 0..100 int;
    // decision order[t] in 0..50 int;
    let line = line.trim_end_matches(';');
    let prefix = if is_state { "state " } else { "decision " };
    let line = &line[prefix.len()..];

    // 変数名とインデックスを抽出
    let (name, indices) = if let Some(b) = line.find('[') {
        let e = line.find(']').unwrap_or(line.len());
        let indices_str = &line[b + 1..e];
        let indices: Vec<&str> = indices_str.split(',').map(|s| s.trim()).collect();
        (line[..b].trim(), Some(indices))
    } else {
        (line.split_whitespace().next().unwrap_or(""), None)
    };

    // in キーワードの後の範囲を抽出
    let mut lb = 0.0f64;
    let mut ub = 1000.0f64;

    if let Some(in_pos) = line.find(" in ") {
        let range_str = &line[in_pos + 4..];

        // 範囲表記: 0..100
        if let Some(dotdot) = range_str.find("..") {
            let start_str = range_str[..dotdot].trim();
            let end_str = range_str[dotdot + 2..]
                .split_whitespace()
                .next()
                .unwrap_or("");
            if let (Ok(start), Ok(end)) = (start_str.parse::<f64>(), end_str.parse::<f64>()) {
                lb = start;
                ub = end;
            }
        }
    }

    // intキーワードの確認（将来の拡張用）
    let _is_int = line.contains(" int") || line.contains(" Integer");

    let mut combos: Vec<String> = Vec::new();
    if let Some(idx_list) = indices {
        let values = expand_indices(idx_list, sets);
        for combo in cartesian(&values) {
            combos.push(format!("{}[{}]", name, combo.join(",")));
        }
    } else {
        combos.push(name.to_string());
    }

    for var_name in combos {
        model.lb.push(lb);
        model.ub.push(ub);
        model.var_names.push(var_name);
    }

    Ok(())
}

fn parse_objective(line: &str, model: &mut Model) -> Result<(), String> {
    // maximize profit: sum{i in Items} value[i] * x[i];
    if let Some(colon) = line.find(':') {
        let expr = line[colon + 1..].trim().trim_end_matches(';');
        model.objective_expr = Some(expr.to_string());
    } else {
        // コロンなしの場合
        if let Some(rest) = line.strip_prefix("maximize ") {
            model
                .objective_expr
                .replace(rest.trim().trim_end_matches(';').to_string());
        } else if let Some(rest) = line.strip_prefix("minimize ") {
            model
                .objective_expr
                .replace(rest.trim().trim_end_matches(';').to_string());
        }
    }
    Ok(())
}

fn parse_objective_named(line: &str) -> (String, String) {
    // minimize total_cost: expr
    if let Some(colon) = line.find(':') {
        let head = line[..colon].trim();
        let expr = line[colon + 1..].trim().trim_end_matches(';').to_string();
        let mut parts = head.split_whitespace();
        let _ = parts.next(); // minimize / maximize
        let name = parts.next().unwrap_or("obj").trim().to_string();
        (name, expr)
    } else {
        ("obj".to_string(), line.trim_end_matches(';').to_string())
    }
}

fn parse_constraint(line: &str, model: &mut Model) -> Result<(), String> {
    // weight_limit: sum{i in Items} weight[i] * x[i] <= capacity;
    let line = line.trim_end_matches(';');

    // CPグローバル制約は記録のみ（簡易ペナルティ用）
    if line.contains("no_overlap") || line.contains("disjunctive") || line.contains("cumulative") {
        model.cp_globals.push(line.to_string());
        return Ok(());
    }

    let (name, expr_part) = if let Some(colon) = line.find(':') {
        (line[..colon].trim().to_string(), &line[colon + 1..])
    } else {
        ("".to_string(), line)
    };

    let expr_part = expr_part.trim();

    // 演算子を探す
    let (op, op_str) = if expr_part.contains("<=") {
        (ConstraintOp::Le, "<=")
    } else if expr_part.contains(">=") {
        (ConstraintOp::Ge, ">=")
    } else if expr_part.contains("==") {
        (ConstraintOp::Eq, "==")
    } else {
        return Ok(()); // 制約ではない
    };

    let parts: Vec<&str> = expr_part.split(op_str).collect();
    if parts.len() != 2 {
        return Ok(());
    }

    let lhs = parts[0].trim();
    let rhs_str = parts[1].trim();

    // RHSを数値に変換
    let rhs = if let Ok(val) = rhs_str.parse::<f64>() {
        val
    } else {
        // パラメータ参照の可能性
        if let Some(param_map) = model.params.get(rhs_str) {
            param_map.get("_").copied().unwrap_or(0.0)
        } else {
            0.0
        }
    };

    model.constraints.push(Constraint {
        name: if name.is_empty() {
            format!("c{}", model.constraints.len())
        } else {
            name
        },
        expr: lhs.to_string(),
        op,
        rhs,
    });

    Ok(())
}

fn parse_bounds(line: &str) -> Result<(f64, f64), String> {
    let mut lb = 0.0f64;
    let mut ub = 1000.0f64;

    // Binary変数
    if line.contains("binary") || line.contains("Binary") {
        return Ok((0.0, 1.0));
    }

    // Integer変数（境界はそのまま）
    let is_int = line.contains("int") || line.contains("Integer");

    // >= パターン
    if let Some(p) = line.find(">=") {
        lb = line[p + 2..]
            .split_whitespace()
            .next()
            .and_then(|s| s.trim_end_matches(';').parse().ok())
            .unwrap_or(0.0);
    }

    // <= パターン
    if let Some(p) = line.find("<=") {
        ub = line[p + 2..]
            .split_whitespace()
            .next()
            .and_then(|s| s.trim_end_matches(';').parse().ok())
            .unwrap_or(1000.0);
    }

    // real変数で境界がない場合はデフォルト
    if !is_int && lb == 0.0 && ub == 1000.0 {
        // デフォルトのまま
    }

    Ok((lb, ub))
}
