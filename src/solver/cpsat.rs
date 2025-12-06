//! CP-SAT (OR-Tools) で CP 制約を厳密に解く
#![cfg(feature = "cp-sat")]

use crate::parser::{ConstraintOp, Model};
use or_tools::sat::*;

pub fn solve_cp(
    model: &Model,
    _max_iter: usize,
    _threads: usize,
) -> Result<(Vec<f64>, f64, usize), String> {
    let mut solver = CpModelBuilder::new();
    // 実数をスケールして整数化
    let scale = 1000.0;

    // 変数: すべて整数化
    let mut vars: Vec<IntVar> = Vec::with_capacity(model.dim);
    for i in 0..model.dim {
        let lb = (model.lb[i] * scale) as i64;
        let ub = (model.ub[i] * scale) as i64;
        vars.push(solver.new_int_var(lb, ub, format!("v{}", i)));
    }

    // 目的（先頭目的 or weighted/epsilon は簡易に先頭のみ）
    let mut objective_terms = Vec::new();
    if !model.objectives.is_empty() {
        let obj = &model.objectives[0];
        let lin = linearize_expr(model, obj.expr.as_str(), &vars, scale);
        objective_terms.extend(lin);
    } else if let Some(expr) = &model.objective_expr {
        let lin = linearize_expr(model, expr.as_str(), &vars, scale);
        objective_terms.extend(lin);
    }
    if model.maximize {
        // OR-Toolsは最小化のみ。符号反転
        for (c, v) in objective_terms.iter_mut() {
            *c = -*c;
        }
    }
    let obj = objective_terms
        .iter()
        .map(|(c, v)| LinearExpr::from(*v) * *c)
        .fold(LinearExpr::from(0), |acc, e| acc + e);
    solver.minimize(obj);

    // 線形制約
    for c in &model.constraints {
        let lin = linearize_expr(model, c.expr.as_str(), &vars, scale);
        let lhs = lin
            .iter()
            .map(|(coef, var)| LinearExpr::from(*var) * *coef)
            .fold(LinearExpr::from(0), |acc, e| acc + e);
        let rhs = (c.rhs * scale) as i64;
        match c.op {
            ConstraintOp::Le => {
                solver.add_linear_constraint(lhs <= rhs);
            }
            ConstraintOp::Ge => {
                solver.add_linear_constraint(lhs >= rhs);
            }
            ConstraintOp::Eq => {
                solver.add_linear_constraint(lhs == rhs);
            }
        }
    }

    // CPグローバル: disjunctive/no_overlap/cumulative (簡易)
    // 期待する変数名: start[...], end[...], duration[...]
    for g in &model.cp_globals {
        if g.contains("disjunctive") || g.contains("no_overlap") {
            let mut intervals = Vec::new();
            for (name, &idx) in &model.var_map {
                if name.starts_with("start[") {
                    let start = vars[idx];
                    let idx_str = name["start[".len()..].to_string();
                    // duration or end
                    let dur_key = format!("duration[{}", idx_str);
                    let end_key = format!("end[{}", idx_str);
                    let duration = if let Some(&didx) = model.var_map.get(&dur_key) {
                        Some(vars[didx])
                    } else {
                        None
                    };
                    let end = if let Some(&eidx) = model.var_map.get(&end_key) {
                        Some(vars[eidx])
                    } else {
                        None
                    };
                    let dur = if let Some(d) = duration {
                        d
                    } else if let Some(e) = end {
                        // end - start
                        let d =
                            solver.new_int_var(0, i64::MAX, format!("dur_tmp_{}", intervals.len()));
                        solver.add_linear_constraint(e - start == d);
                        d
                    } else {
                        // fallback duration=1
                        solver.new_constant(1)
                    };
                    let interval = solver.new_interval_var(
                        start,
                        dur,
                        LinearExpr::from(start) + dur,
                        format!("iv_{}", intervals.len()),
                    );
                    intervals.push(interval);
                }
            }
            if !intervals.is_empty() {
                solver.add_no_overlap(intervals);
            }
        } else if g.contains("cumulative") {
            // 期待: cumulative(start[j], duration[j], demand, capacity)
            let mut starts = Vec::new();
            let mut durations = Vec::new();
            let mut demands = Vec::new();
            // capacityは行から抽出（既存ペナルティと同様）: 最初の2つの数値
            let nums: Vec<i64> = g
                .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
                .filter_map(|s| {
                    if s.is_empty() {
                        None
                    } else {
                        s.parse::<f64>().ok().map(|v| (v * scale) as i64)
                    }
                })
                .collect();
            let demand_val = nums.get(0).copied().unwrap_or(scale as i64);
            let capacity_val = nums.get(1).copied().unwrap_or(scale as i64);

            for (name, &idx) in &model.var_map {
                if name.starts_with("start[") {
                    let start = vars[idx];
                    let idx_str = name["start[".len()..].to_string();
                    let dkey = format!("duration[{}", idx_str);
                    if let Some(&didx) = model.var_map.get(&dkey) {
                        let dur = vars[didx];
                        starts.push(start);
                        durations.push(dur);
                        demands.push(demand_val);
                    }
                }
            }
            if !starts.is_empty() {
                solver.add_cumulative(starts, durations, demands, capacity_val);
            }
        }
    }

    // solve
    let mut opt = CpSolver::new();
    opt.set_num_search_workers(4);
    let result = opt.solve(&solver.build());
    if result != CpSolverStatus::Optimal && result != CpSolverStatus::Feasible {
        return Err(format!("cp-sat status: {:?}", result));
    }
    let mut best = vec![0.0; model.dim];
    for i in 0..model.dim {
        best[i] = opt.value(vars[i]) as f64 / scale;
    }
    let fitness = opt.objective_value() as f64 / scale;
    Ok((best, fitness, 0))
}

// 線形化（非常に限定的：x[i], 定数、単純な足し算のみを想定）
fn linearize_expr(model: &Model, expr: &str, vars: &[IntVar], scale: f64) -> Vec<(i64, IntVar)> {
    let mut terms: Vec<(i64, IntVar)> = Vec::new();
    for token in expr.split('+') {
        let t = token.trim();
        if let Some(idx) = model.var_map.get(t) {
            terms.push((1, vars[*idx]));
        } else if let Some(v) = model.params.get(t).and_then(|m| m.get("_")) {
            let c = (v * scale) as i64;
            let const_var = vars.get(0).cloned().unwrap_or_else(|| {
                // もし変数がない場合のダミー
                // ここでは0~0の定数を返す
                // ただし通常dim>0の前提
                unimplemented!()
            });
            terms.push((c, const_var)); // const_var * c（後で符号反転含む）
        }
    }
    terms
}
