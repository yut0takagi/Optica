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
    pub var_int: Vec<bool>,
    pub var_names: Vec<String>,
    pub var_map: HashMap<String, usize>, // 変数名 -> インデックス
    pub maximize: bool,
    pub params: HashMap<String, HashMap<String, f64>>, // パラメータ値
    pub sets: HashMap<String, Vec<String>>,            // 集合
    pub objective_ast: Option<crate::expr::Expr>,      // 目的関数AST
    pub constraints: Vec<Constraint>,                  // 制約
    pub objectives: Vec<Objective>,                    // 多目的
    pub pareto: ParetoMethod,
    pub cp_globals: Vec<String>, // CPグローバル制約（no_overlap, disjunctive, cumulative）
}

#[derive(Debug, Clone)]
pub struct Constraint {
    #[allow(dead_code)]
    pub name: String,
    pub lhs: crate::expr::Expr,
    pub rhs: crate::expr::Expr,
    pub op: ConstraintOp,
    /// forall で束縛された添字変数（例: {"i": "2"}）。forall を伴わない制約では空。
    pub env: HashMap<String, String>,
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
    pub ast: crate::expr::Expr,
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
            var_int: Vec::new(),
            var_names: Vec::new(),
            var_map: HashMap::new(),
            maximize: false,
            params: HashMap::new(),
            sets: HashMap::new(),
            objective_ast: None,
            constraints: Vec::new(),
            objectives: Vec::new(),
            pareto: ParetoMethod::Single,
            cp_globals: Vec::new(),
        }
    }

    /// 式評価用の Ctx を構築
    pub fn ctx(&self) -> crate::expr::Ctx<'_> {
        crate::expr::Ctx {
            var_map: &self.var_map,
            params: &self.params,
            sets: &self.sets,
        }
    }

    /// 目的関数を評価
    pub fn evaluate_objective(&self, x: &[f64]) -> f64 {
        let env = HashMap::new();
        // 単一目的（従来互換）か、多目的の重み付け/epsilonを後段で処理する
        if let Some(ref e) = self.objective_ast {
            crate::expr::eval(e, x, &env, &self.ctx())
        } else if let Some(o) = self.objectives.first() {
            // 一旦最初の目的を返す（互換のため）。実際の組み合わせはcompute_fitness側で処理。
            crate::expr::eval(&o.ast, x, &env, &self.ctx())
        } else {
            // デフォルト: Sphere関数
            x.iter().map(|&v| v * v).sum()
        }
    }

    /// 制約違反をチェック
    pub fn check_constraints(&self, x: &[f64]) -> (bool, f64) {
        let ctx = self.ctx();
        let mut feasible = true;
        let mut total_violation = 0.0;

        for constraint in &self.constraints {
            let l = crate::expr::eval(&constraint.lhs, x, &constraint.env, &ctx);
            let r = crate::expr::eval(&constraint.rhs, x, &constraint.env, &ctx);
            let v = match constraint.op {
                ConstraintOp::Le => (l - r).max(0.0),
                ConstraintOp::Ge => (r - l).max(0.0),
                ConstraintOp::Eq => (l - r).abs(),
            };
            if v > 1e-9 {
                feasible = false;
                total_violation += v;
            }
        }

        (feasible, total_violation)
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

    let statements = logical_statements(source);
    for stmt in &statements {
        let line = stmt.trim();

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
                let (name, ast) = parse_objective_named(line)?;
                model.objectives.push(Objective {
                    name: name.clone(),
                    ast,
                    maximize,
                });
                if model.objective_ast.is_none() {
                    model.maximize = maximize;
                    model.objective_ast = Some(model.objectives.last().unwrap().ast.clone());
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
        } else if let Some(stripped) = line.strip_prefix("subject to") {
            in_subject_to = true;
            // "subject to" の直後を見る。":"（または空）で始まる場合はブロック
            // マーカー単体（"subject to:" / "subject to"）とみなし、従来通り
            // 何もしない（後続のインデント行が個別の制約として下の
            // `in_subject_to` 分岐で parse_constraint に渡る）。
            // それ以外の非空文字列が続く場合は "subject to weight_limit:
            // expr OP rhs;" 形式のインライン制約であり、従来はここで
            // 握り潰されて制約が一切適用されないバグがあった。同じ
            // parse_constraint（name: ラベルと forall を扱う）に渡して修正する。
            let rest = stripped.trim();
            if !rest.is_empty() && !rest.starts_with(':') {
                parse_constraint(rest, &mut model)?;
            }
        } else if in_subject_to && !line.is_empty() {
            parse_constraint(line, &mut model)?;
        } else if line.starts_with("forall") {
            // ここに到達する `forall` は subject to コンテキスト外にある（ブロック形式は
            // 直前の `in_subject_to` 分岐、インライン形式は `subject to ...` 分岐で
            // 既に parse_constraint に渡っているため、両方の正当形式はここに来ない）。
            // 従来はどの分岐にもマッチせずサイレントに読み捨てられ、制約が一切
            // 適用されないまま「無制約の最適値」を feasible として報告していた。
            // README の「サイレントエラー禁止」保証に反するため明示エラーにする。
            return Err(format!(
                "'forall' must appear inside a 'subject to' block: {}",
                line
            ));
        }
    }

    // 変数マップを構築
    for (i, name) in model.var_names.iter().enumerate() {
        model.var_map.insert(name.clone(), i);
    }

    // 未知シンボル検証（既知名 = パラメータ名 ∪ 集合名 ∪ 変数基底名）
    let mut known: std::collections::HashSet<String> = model.params.keys().cloned().collect();
    for s in model.sets.keys() {
        known.insert(s.clone());
    }
    for vn in &model.var_names {
        known.insert(vn.split('[').next().unwrap().to_string());
    }
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
    if let Some(ref e) = model.objective_ast {
        check(e, &[])?;
    }
    for o in &model.objectives {
        check(&o.ast, &[])?;
    }
    for c in &model.constraints {
        // forall で束縛された添字変数名は既知スコープとして許可する
        let fv: Vec<String> = c.env.keys().cloned().collect();
        check(&c.lhs, &fv)?;
        check(&c.rhs, &fv)?;
    }

    model.dim = model.lb.len();
    model.var_int.resize(model.dim, false);
    Ok(model)
}

/// `subject to`/`objectives:`/`data:`/`epsilon:` はブロック開始マーカーであり、
/// 自身の後続行を吸収（連結）しない（各後続行は parse() が個別に解釈する）。
/// 特に `epsilon:` は閾値行（`name <= v`）を後続インデント行に持つため、
/// ここで除外しないと閾値が連結され `epsilon:` スキップに巻き込まれて落ちる。
fn is_section_marker(line: &str) -> bool {
    line.starts_with("subject to")
        || line.starts_with("objectives:")
        || line.starts_with("data:")
        || line.starts_with("epsilon:")
}

/// この行から新しい論理文が始まる（＝直前の論理文への連結を止める）べきトップレベルキーワードか。
fn is_top_level_start(line: &str) -> bool {
    line.starts_with("var ")
        || line.starts_with("param ")
        || line.starts_with("set ")
        || line.starts_with("maximize")
        || line.starts_with("minimize")
        || line.starts_with("pareto")
        || is_section_marker(line)
}

/// ソースを「論理文」の列に再構成する。
/// コメント・空行を除去した上で、`maximize:`/`minimize:`/制約ラベル/`forall ...:` のように
/// 行末が単独の `:` で終わる行（＝本体が次行以降にある行。ただし `subject to:`/`objectives:`/
/// `data:` は除く）は、次のトップレベルキーワード、またはインデントが自身以下に戻るまで、
/// 後続行を空白連結して1つの論理文にする。
fn logical_statements(source: &str) -> Vec<String> {
    let mut raw: Vec<(usize, String)> = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        raw.push((indent, trimmed.to_string()));
    }

    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < raw.len() {
        let (base_indent, mut joined) = raw[i].clone();
        i += 1;

        // 行末が（`::` ではなく）単独の `:` で終わる本体待ち行は、後続行を吸収する。
        let awaits_body =
            joined.ends_with(':') && !joined.ends_with("::") && !is_section_marker(&joined);

        if awaits_body {
            while i < raw.len() {
                let (indent, next_text) = raw[i].clone();
                if indent <= base_indent || is_top_level_start(&next_text) {
                    break;
                }
                joined.push(' ');
                joined.push_str(&next_text);
                i += 1;
            }
        }
        out.push(joined);
    }
    out
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
    let (lb, ub, is_int) = parse_bounds(line)?;

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
        model.var_int.push(is_int);
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

    // intキーワードの確認（parse_bounds と同一のトークン単位判定を共有）
    let is_int = is_integer_decl(line);

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
        model.var_int.push(is_int);
        model.var_names.push(var_name);
    }

    Ok(())
}

fn parse_objective(line: &str, model: &mut Model) -> Result<(), String> {
    // maximize profit: sum{i in Items} value[i] * x[i];
    if let Some(colon) = line.find(':') {
        let expr = line[colon + 1..].trim().trim_end_matches(';');
        model.objective_ast = Some(crate::expr::compile(expr)?);
    } else {
        // コロンなしの場合
        if let Some(rest) = line.strip_prefix("maximize ") {
            model.objective_ast = Some(crate::expr::compile(rest.trim().trim_end_matches(';'))?);
        } else if let Some(rest) = line.strip_prefix("minimize ") {
            model.objective_ast = Some(crate::expr::compile(rest.trim().trim_end_matches(';'))?);
        }
    }
    Ok(())
}

fn parse_objective_named(line: &str) -> Result<(String, crate::expr::Expr), String> {
    // minimize total_cost: expr
    if let Some(colon) = line.find(':') {
        let head = line[..colon].trim();
        let expr = line[colon + 1..].trim().trim_end_matches(';');
        let mut parts = head.split_whitespace();
        let _ = parts.next(); // minimize / maximize
        let name = parts.next().unwrap_or("obj").trim().to_string();
        let ast = crate::expr::compile(expr)?;
        Ok((name, ast))
    } else {
        let ast = crate::expr::compile(line.trim_end_matches(';'))?;
        Ok(("obj".to_string(), ast))
    }
}

/// `i in S, j in T` 形式の forall ヘッダを `[("i","S"), ("j","T")]` にパースする。
fn parse_forall_header(header: &str) -> Result<Vec<(String, String)>, String> {
    let mut v = Vec::new();
    for part in header.split(',') {
        let part = part.trim();
        if let Some(in_pos) = part.find(" in ") {
            let var = part[..in_pos].trim().to_string();
            let set = part[in_pos + 4..].trim().to_string();
            if var.is_empty() || set.is_empty() {
                return Err(format!("bad forall binding: {}", part));
            }
            v.push((var, set));
        } else {
            return Err(format!("bad forall binding: {}", part));
        }
    }
    Ok(v)
}

fn parse_constraint(line: &str, model: &mut Model) -> Result<(), String> {
    // weight_limit: sum{i in Items} weight[i] * x[i] <= capacity;
    // forall i in S: x[i] <= 1
    // cap: forall i in S: x[i] <= 1
    let line = line.trim_end_matches(';');

    // CPグローバル制約は記録のみ（簡易ペナルティ用）
    if line.contains("no_overlap") || line.contains("disjunctive") || line.contains("cumulative") {
        model.cp_globals.push(line.to_string());
        return Ok(());
    }

    // 先頭のラベル（"name:"）があれば取り出す。ただし "forall ...:" 自体をラベルと誤認しない。
    let (name, rest) = if let Some(colon) = line.find(':') {
        let head = line[..colon].trim();
        if !head.is_empty() && !head.starts_with("forall") {
            (head.to_string(), line[colon + 1..].trim())
        } else {
            ("".to_string(), line)
        }
    } else {
        ("".to_string(), line)
    };

    // forall プレフィックスを検出し、束縛変数と本体を分離する。
    let (forall_bindings, expr_part) = if let Some(body) = rest.trim().strip_prefix("forall ") {
        let colon = body
            .find(':')
            .ok_or_else(|| format!("forall missing ':' in constraint: {}", line))?;
        let header = body[..colon].trim();
        let tail = body[colon + 1..].trim().to_string();
        (parse_forall_header(header)?, tail)
    } else {
        (Vec::new(), rest.trim().to_string())
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

    let lhs_ast = crate::expr::compile(lhs)?;
    let rhs_ast = crate::expr::compile(rhs_str)?;

    let base_name = if name.is_empty() {
        format!("c{}", model.constraints.len())
    } else {
        name
    };

    if forall_bindings.is_empty() {
        model.constraints.push(Constraint {
            name: base_name,
            lhs: lhs_ast,
            rhs: rhs_ast,
            op,
            env: HashMap::new(),
        });
    } else {
        // forall の束縛集合をデカルト積へ展開し、組み合わせごとに Constraint を1本生成する。
        let bound_vars: Vec<String> = forall_bindings.iter().map(|(v, _)| v.clone()).collect();
        let set_refs: Vec<&str> = forall_bindings.iter().map(|(_, s)| s.as_str()).collect();
        let value_lists = expand_indices(set_refs, &model.sets);
        for combo in cartesian(&value_lists) {
            let mut env = HashMap::new();
            for (var, val) in bound_vars.iter().zip(combo.iter()) {
                env.insert(var.clone(), val.clone());
            }
            model.constraints.push(Constraint {
                name: base_name.clone(),
                lhs: lhs_ast.clone(),
                rhs: rhs_ast.clone(),
                op,
                env,
            });
        }
    }

    Ok(())
}

/// 宣言行が整数型（`int`/`Integer`/`integer`）を指定しているか。
/// 部分文字列一致は誤検出（`point`/`interval`/`print` 等の変数名）を招くため、
/// 空白区切りトークンとして厳密に判定する。
fn is_integer_decl(line: &str) -> bool {
    line.split_whitespace()
        .any(|t| matches!(t, "int" | "Integer" | "integer"))
}

/// 宣言行が binary 型（`binary`/`Binary`）を指定しているか（トークン単位で判定）。
fn is_binary_decl(line: &str) -> bool {
    line.split_whitespace()
        .any(|t| matches!(t, "binary" | "Binary"))
}

fn parse_bounds(line: &str) -> Result<(f64, f64, bool), String> {
    let mut lb = 0.0f64;
    let mut ub = 1000.0f64;

    // Binary変数（整数フラグも立てる）
    if is_binary_decl(line) {
        return Ok((0.0, 1.0, true));
    }

    // Integer変数（境界はそのまま）
    let is_int = is_integer_decl(line);

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

    Ok((lb, ub, is_int))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 回帰テスト: 論理行連結が `epsilon:` の閾値行（次のインデント行）を
    /// 吸収して落とさないこと。吸収すると parse() の `epsilon:` スキップに
    /// 巻き込まれ eps が空になる（Task 3 導入時のリグレッション）。
    #[test]
    fn epsilon_thresholds_are_captured() {
        let src = [
            "var x >= 0 <= 10",
            "var y >= 0 <= 10",
            "objectives:",
            "    minimize cost:",
            "        x",
            "    minimize spread:",
            "        y",
            "pareto method: \"epsilon_constraint\"",
            "    primary: cost",
            "    epsilon:",
            "        spread <= 1",
            "subject to:",
            "    c1: x >= 0",
        ]
        .join("\n");

        let model = parse(&src).expect("parse");
        match &model.pareto {
            ParetoMethod::Epsilon { primary, eps } => {
                assert_eq!(primary, "cost");
                assert_eq!(eps.len(), 1, "epsilon thresholds dropped: {:?}", eps);
                let (name, op, rhs) = &eps[0];
                assert_eq!(name, "spread");
                assert!(matches!(op, ConstraintOp::Le), "expected Le, got {:?}", op);
                assert_eq!(*rhs, 1.0);
            }
            other => panic!("expected Epsilon pareto method, got {:?}", other),
        }
    }
}
