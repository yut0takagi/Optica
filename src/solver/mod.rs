//! 超高速最適化ソルバー
//!
//! 最適化技術:
//! - ゼロアロケーション（内部ループ）
//! - ループ展開（16要素ずつ）
//! - キャッシュフレンドリーなデータ配置
//! - 効率的な並列処理
//! - 分岐予測最適化

mod objective;
mod rng;

use crate::config::*;
use crate::parser::{ConstraintOp, Model, ParetoMethod};
use std::sync::{Arc, OnceLock};
use std::thread;
pub mod cpsat;
#[cfg(feature = "cp-sat")]
use crate::solver::cpsat::solve_cp;
#[cfg(not(feature = "cp-sat"))]
fn solve_cp(_model: &Model) -> Option<(Vec<f64>, f64, usize)> {
    None
}

pub fn solve_cp_entry(
    model: &Model,
    max_iter: usize,
    threads: usize,
) -> Option<(Vec<f64>, f64, usize)> {
    let _ = (max_iter, threads);
    solve_cp(model)
}

pub use rng::Rng;

const PENALTY_COEFF: f64 = 1e6;
static PENALTY_ENV: OnceLock<f64> = OnceLock::new();

pub mod cpsat;

// =============================================================================
// 差分進化（DE）
// =============================================================================

/// DE最適化（モデルを考慮）
pub fn de(model: &Model, max_iter: usize, threads: usize) -> (Vec<f64>, f64, usize) {
    if !model.cp_globals.is_empty() {
        if let Some(res) = solve_cp(model) {
            return res;
        }
    }
    let dim = model.dim;

    if threads <= 1 || dim < PARALLEL_MIN_DIM || max_iter < PARALLEL_MIN_ITER {
        de_single(model, max_iter)
    } else {
        de_parallel(model, max_iter, threads)
    }
}

fn de_single(model: &Model, max_iter: usize) -> (Vec<f64>, f64, usize) {
    let dim = model.dim;
    let lb = &model.lb;
    let ub = &model.ub;
    let mut rng = Rng::new(12345);

    // 集団初期化
    let mut pop = Population::new(dim, POP_SIZE);
    pop.initialize(&mut rng, lb, ub, |cand| compute_fitness(model, cand));

    // 最良解
    let mut best = pop.find_best();
    let mut best_fit = compute_fitness(model, &best);

    // 作業用バッファ
    let mut trial = vec![0.0; dim];
    let mut rnd_cr = vec![0.0; dim];

    // メインループ
    for iter in 0..max_iter {
        for i in 0..POP_SIZE {
            // 親選択
            let (r1, r2) = pop.select_parents(&mut rng, i);
            let j_rand = rng.usize(dim);

            // 一括乱数生成
            rng.fill_f64(&mut rnd_cr);

            // 変異 + 交叉
            de_crossover(&pop, i, r1, r2, j_rand, &best, &rnd_cr, lb, ub, &mut trial);

            // 評価 + 選択
            let trial_fit = compute_fitness(model, &trial);
            if trial_fit <= pop.fit[i] {
                pop.update(i, &trial, trial_fit);

                if trial_fit < best_fit {
                    best_fit = trial_fit;
                    best.copy_from_slice(&trial);

                    if best_fit < TOLERANCE {
                        return (best, best_fit, iter + 1);
                    }
                }
            }
        }
    }

    (best, best_fit, max_iter)
}

fn de_parallel(model: &Model, max_iter: usize, threads: usize) -> (Vec<f64>, f64, usize) {
    let dim = model.dim;
    let lb = model.lb.clone();
    let ub = model.ub.clone();
    let model = Arc::new(model.clone());

    let handles: Vec<_> = (0..threads)
        .map(|t| {
            let lb = lb.clone();
            let ub = ub.clone();
            let model = Arc::clone(&model);
            thread::spawn(move || {
                let mut rng = Rng::new(12345 + t as u64 * 7919);
                let sub_pop = (POP_SIZE / threads).max(10);

                let mut pop = Population::new(dim, sub_pop);
                pop.initialize(&mut rng, &lb, &ub, |cand| compute_fitness(&model, cand));

                let mut best = pop.find_best();
                let mut best_fit = compute_fitness(&model, &best);
                let mut trial = vec![0.0; dim];
                let mut rnd_cr = vec![0.0; dim];

                for _iter in 0..max_iter {
                    for i in 0..sub_pop {
                        let (r1, r2) = pop.select_parents(&mut rng, i);
                        let j_rand = rng.usize(dim);
                        rng.fill_f64(&mut rnd_cr);

                        de_crossover(
                            &pop, i, r1, r2, j_rand, &best, &rnd_cr, &lb, &ub, &mut trial,
                        );

                        let trial_fit = compute_fitness(&model, &trial);
                        if trial_fit <= pop.fit[i] {
                            pop.update(i, &trial, trial_fit);
                            if trial_fit < best_fit {
                                best_fit = trial_fit;
                                best.copy_from_slice(&trial);
                                if best_fit < TOLERANCE {
                                    return (best, best_fit);
                                }
                            }
                        }
                    }
                }

                (best, best_fit)
            })
        })
        .collect();

    // 結果集約
    handles
        .into_iter()
        .map(|h| h.join().unwrap())
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        .map(|(best, fit)| (best, fit, max_iter))
        .unwrap()
}

#[inline(always)]
#[allow(clippy::too_many_arguments)]
fn de_crossover(
    pop: &Population,
    i: usize,
    r1: usize,
    r2: usize,
    j_rand: usize,
    best: &[f64],
    rnd_cr: &[f64],
    lb: &[f64],
    ub: &[f64],
    trial: &mut [f64],
) {
    let dim = lb.len();
    let pop_i = i * dim;
    let pop_r1 = r1 * dim;
    let pop_r2 = r2 * dim;

    // j_randを先に処理（分岐予測最適化）
    if j_rand < dim {
        let v = best[j_rand] + DE_F * (pop.data[pop_r1 + j_rand] - pop.data[pop_r2 + j_rand]);
        trial[j_rand] = v.clamp(lb[j_rand], ub[j_rand]);
    }

    // 残りを一括処理
    for j in 0..dim {
        if j != j_rand {
            trial[j] = if rnd_cr[j] < DE_CR {
                let v = best[j] + DE_F * (pop.data[pop_r1 + j] - pop.data[pop_r2 + j]);
                v.clamp(lb[j], ub[j])
            } else {
                pop.data[pop_i + j]
            };
        }
    }
}

// =============================================================================
// 粒子群最適化（PSO）
// =============================================================================

/// PSO最適化
pub fn pso(model: &Model, max_iter: usize) -> (Vec<f64>, f64, usize) {
    if !model.cp_globals.is_empty() {
        if let Some(res) = solve_cp(model) {
            return res;
        }
    }
    let dim = model.dim;
    let lb = &model.lb;
    let ub = &model.ub;
    let mut rng = Rng::new(67890);

    // v_max
    let v_max: Vec<f64> = lb.iter().zip(ub).map(|(l, u)| (u - l) * 0.5).collect();

    // 初期化
    let mut swarm = Swarm::new(dim, N_PARTICLES);
    swarm.initialize(&mut rng, lb, ub);
    swarm.pbest_fit = (0..N_PARTICLES)
        .map(|i| compute_fitness(model, &swarm.pos[i * dim..(i + 1) * dim]))
        .collect();

    let mut gbest = swarm.find_global_best(|cand| compute_fitness(model, cand));
    let mut gbest_fit = compute_fitness(model, &gbest);
    let mut w = PSO_W_INIT;

    // 作業用バッファ
    let mut r1_buf = vec![0.0; dim];
    let mut r2_buf = vec![0.0; dim];

    // メインループ
    for iter in 0..max_iter {
        for i in 0..N_PARTICLES {
            let offset = i * dim;

            // 一括乱数生成
            rng.fill_f64(&mut r1_buf);
            rng.fill_f64(&mut r2_buf);

            // 速度・位置更新
            pso_update_velocity_position(
                &mut swarm, i, offset, &gbest, &v_max, lb, ub, w, &r1_buf, &r2_buf,
            );

            // 評価
            let fit = compute_fitness(model, &swarm.pos[offset..offset + dim]);

            // pbest更新
            if fit < swarm.pbest_fit[i] {
                swarm.pbest[offset..offset + dim].copy_from_slice(&swarm.pos[offset..offset + dim]);
                swarm.pbest_fit[i] = fit;

                // gbest更新
                if fit < gbest_fit {
                    gbest_fit = fit;
                    gbest.copy_from_slice(&swarm.pos[offset..offset + dim]);

                    if gbest_fit < TOLERANCE {
                        return (gbest, gbest_fit, iter + 1);
                    }
                }
            }
        }

        w = (w * PSO_W_DECAY).max(PSO_W_MIN);
    }

    (gbest, gbest_fit, max_iter)
}

#[allow(clippy::too_many_arguments)]
#[inline(always)]
fn pso_update_velocity_position(
    swarm: &mut Swarm,
    _i: usize,
    offset: usize,
    gbest: &[f64],
    v_max: &[f64],
    lb: &[f64],
    ub: &[f64],
    w: f64,
    r1: &[f64],
    r2: &[f64],
) {
    let dim = lb.len();

    for j in 0..dim {
        let mut v = w * swarm.vel[offset + j]
            + PSO_C1 * r1[j] * (swarm.pbest[offset + j] - swarm.pos[offset + j])
            + PSO_C2 * r2[j] * (gbest[j] - swarm.pos[offset + j]);

        v = v.clamp(-v_max[j], v_max[j]);
        swarm.vel[offset + j] = v;

        let mut p = swarm.pos[offset + j] + v;
        if p < lb[j] {
            p = lb[j];
            swarm.vel[offset + j] = 0.0;
        }
        if p > ub[j] {
            p = ub[j];
            swarm.vel[offset + j] = 0.0;
        }
        swarm.pos[offset + j] = p;
    }
}

// =============================================================================
// 評価関数（目的 + 制約ペナルティ）
// =============================================================================

fn compute_fitness(model: &Model, x: &[f64]) -> f64 {
    // 多目的対応
    if !model.objectives.is_empty() {
        match &model.pareto {
            ParetoMethod::WeightedSum(weights) if !weights.is_empty() => {
                // 重み付き和
                let mut total = 0.0;
                for (name, w) in weights {
                    if let Some(obj) = model.objectives.iter().find(|o| &o.name == name) {
                        let mut v =
                            model.evaluate_expr(&obj.expr, x, &std::collections::HashMap::new());
                        if obj.maximize {
                            v = -v;
                        }
                        total += w * v;
                    }
                }
                let (_f, vio) = model.check_constraints(x);
                return total + vio * penalty_coeff();
            }
            ParetoMethod::Epsilon { primary, eps } => {
                // epsilon制約: primaryを最適化、他は閾値超過にペナルティ
                let mut v_primary = 0.0;
                if let Some(obj) = model.objectives.iter().find(|o| &o.name == primary) {
                    v_primary =
                        model.evaluate_expr(&obj.expr, x, &std::collections::HashMap::new());
                    if obj.maximize {
                        v_primary = -v_primary;
                    }
                }
                let mut vio_eps = 0.0;
                for (name, op, rhs) in eps {
                    if let Some(obj) = model.objectives.iter().find(|o| &o.name == name) {
                        let mut v =
                            model.evaluate_expr(&obj.expr, x, &std::collections::HashMap::new());
                        if obj.maximize {
                            v = -v;
                        }
                        let viol = match op {
                            ConstraintOp::Le => (v - rhs).max(0.0),
                            ConstraintOp::Ge => (rhs - v).max(0.0),
                            ConstraintOp::Eq => (v - rhs).abs(),
                        };
                        vio_eps += viol;
                    }
                }
                let (_f, vio) = model.check_constraints(x);
                return v_primary + (vio + vio_eps) * penalty_coeff();
            }
            _ => {
                // デフォルト: 先頭の目的を使用
                let obj = &model.objectives[0];
                let mut v = model.evaluate_expr(&obj.expr, x, &std::collections::HashMap::new());
                if obj.maximize {
                    v = -v;
                }
                let (_f, vio) = model.check_constraints(x);
                return v + vio * penalty_coeff();
            }
        }
    }
    // 単一目的（従来）
    let mut obj = model.evaluate_objective(x);
    if model.maximize {
        obj = -obj;
    }
    let (_feasible, violation) = model.check_constraints(x);
    let cp_penalty = compute_cp_penalty(model, x);
    obj + (violation + cp_penalty) * penalty_coeff()
}

fn penalty_coeff() -> f64 {
    *PENALTY_ENV.get_or_init(|| {
        std::env::var("OPTICA_PENALTY")
            .ok()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(PENALTY_COEFF)
    })
}

// =============================================================================
// CPグローバル制約の簡易ペナルティ
// =============================================================================

fn compute_cp_penalty(model: &Model, x: &[f64]) -> f64 {
    let mut pen = 0.0;
    for g in &model.cp_globals {
        if g.contains("no_overlap") {
            pen += penalty_no_overlap(model, x, "start[", Some("end["), None);
        } else if g.contains("disjunctive") {
            pen += penalty_no_overlap(model, x, "start[", None, Some("duration["));
        } else if g.contains("cumulative") {
            pen += penalty_cumulative(model, x, g, "start[", "duration[");
        }
    }
    pen
}

fn penalty_no_overlap(
    model: &Model,
    x: &[f64],
    start_prefix: &str,
    end_prefix: Option<&str>,
    dur_prefix: Option<&str>,
) -> f64 {
    let mut intervals: Vec<(f64, f64)> = Vec::new();
    for (name, &idx) in &model.var_map {
        if let Some(idx_suffix) = name.strip_prefix(start_prefix) {
            let start = x[idx];
            let idx_str = idx_suffix.to_string(); // includes trailing ]
            let end = if let Some(ep) = end_prefix {
                let ekey = format!("{}{}", ep, idx_str);
                get_var_val(model, x, &ekey)
            } else if let Some(dp) = dur_prefix {
                let dkey = format!("{}{}", dp, idx_str);
                get_var_val(model, x, &dkey).map(|d| start + d)
            } else {
                None
            };
            if let Some(e) = end {
                if e > start {
                    intervals.push((start, e));
                }
            }
        }
    }
    let mut vio = 0.0;
    for i in 0..intervals.len() {
        for j in i + 1..intervals.len() {
            let (s1, e1) = intervals[i];
            let (s2, e2) = intervals[j];
            let overlap = (e1.min(e2) - s1.max(s2)).max(0.0);
            if overlap > 0.0 {
                vio += overlap;
            }
        }
    }
    vio
}

fn penalty_cumulative(
    model: &Model,
    x: &[f64],
    line: &str,
    start_prefix: &str,
    dur_prefix: &str,
) -> f64 {
    // heuristic: use all start/duration pairs; demand and capacityを行から抽出（3番目と4番目の数値）
    let nums: Vec<f64> = line
        .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
        .filter_map(|s| {
            if s.is_empty() {
                None
            } else {
                s.parse::<f64>().ok()
            }
        })
        .collect();
    let demand = nums.first().copied().unwrap_or(1.0);
    let capacity = nums.get(1).copied().unwrap_or(1.0);

    let mut intervals: Vec<(f64, f64)> = Vec::new();
    for (name, &idx) in &model.var_map {
        if let Some(idx_suffix) = name.strip_prefix(start_prefix) {
            let start = x[idx];
            let idx_str = idx_suffix.to_string();
            let dkey = format!("{}{}", dur_prefix, idx_str);
            if let Some(d) = get_var_val(model, x, &dkey) {
                if d > 0.0 {
                    intervals.push((start, start + d));
                }
            }
        }
    }
    if intervals.is_empty() {
        return 0.0;
    }
    // collect time points
    let mut pts: Vec<f64> = intervals.iter().flat_map(|(s, e)| vec![*s, *e]).collect();
    pts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    pts.dedup();
    let mut vio = 0.0;
    for w in pts.windows(2) {
        let t0 = w[0];
        let t1 = w[1];
        let mid = 0.5 * (t0 + t1);
        let mut load = 0.0;
        for &(s, e) in &intervals {
            if mid >= s && mid < e {
                load += demand;
            }
        }
        if load > capacity {
            vio += (load - capacity) * (t1 - t0);
        }
    }
    vio
}

fn get_var_val(model: &Model, x: &[f64], key: &str) -> Option<f64> {
    model.var_map.get(key).map(|&i| x[i])
}

// =============================================================================
// ハイブリッド
// =============================================================================

/// ハイブリッド最適化（DE + PSO）
pub fn hybrid(model: &Model, max_iter: usize, threads: usize) -> (Vec<f64>, f64, usize) {
    // Phase 1: DE for exploration
    let (x1, f1, _) = de(model, max_iter / 2, threads);

    // Phase 2: PSO for refinement
    let dim = model.dim;
    let scale = 0.1;

    let mut lb2: Vec<f64> = model.lb.clone();
    let mut ub2: Vec<f64> = model.ub.clone();

    // 探索範囲を最良解周辺に縮小
    for j in 0..dim {
        let range = (model.ub[j] - model.lb[j]) * scale;
        lb2[j] = (x1[j] - range).max(model.lb[j]);
        ub2[j] = (x1[j] + range).min(model.ub[j]);
    }

    let mut sub_model = model.clone();
    sub_model.lb = lb2;
    sub_model.ub = ub2;
    sub_model.dim = dim;

    let (x2, f2, _) = pso(&sub_model, max_iter / 2);

    if f2 < f1 {
        (x2, f2, max_iter)
    } else {
        (x1, f1, max_iter)
    }
}

// =============================================================================
// データ構造
// =============================================================================

/// DE集団
struct Population {
    data: Vec<f64>,
    fit: Vec<f64>,
    dim: usize,
    size: usize,
}

impl Population {
    fn new(dim: usize, size: usize) -> Self {
        Self {
            data: Vec::with_capacity(size * dim),
            fit: Vec::with_capacity(size),
            dim,
            size,
        }
    }

    fn initialize<F>(&mut self, rng: &mut Rng, lb: &[f64], ub: &[f64], mut fitness: F)
    where
        F: FnMut(&[f64]) -> f64,
    {
        let mut rnd_buf = vec![0.0; self.dim];

        for _ in 0..self.size {
            rng.fill_f64(&mut rnd_buf);
            for j in 0..self.dim {
                self.data.push(lb[j] + rnd_buf[j] * (ub[j] - lb[j]));
            }
        }

        for i in 0..self.size {
            let val = fitness(&self.data[i * self.dim..(i + 1) * self.dim]);
            self.fit.push(val);
        }
    }

    fn find_best(&self) -> Vec<f64> {
        let best_idx = self
            .fit
            .iter()
            .enumerate()
            .min_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap()
            .0;
        self.data[best_idx * self.dim..(best_idx + 1) * self.dim].to_vec()
    }

    fn select_parents(&self, rng: &mut Rng, i: usize) -> (usize, usize) {
        let mut r1 = rng.usize(self.size);
        while r1 == i {
            r1 = (r1 + 1) % self.size;
        }
        let mut r2 = rng.usize(self.size);
        while r2 == i || r2 == r1 {
            r2 = (r2 + 1) % self.size;
        }
        (r1, r2)
    }

    fn update(&mut self, i: usize, trial: &[f64], trial_fit: f64) {
        let offset = i * self.dim;
        self.data[offset..offset + self.dim].copy_from_slice(trial);
        self.fit[i] = trial_fit;
    }
}

/// PSO群
struct Swarm {
    pos: Vec<f64>,
    vel: Vec<f64>,
    pbest: Vec<f64>,
    pbest_fit: Vec<f64>,
    dim: usize,
}

impl Swarm {
    fn new(dim: usize, n_particles: usize) -> Self {
        Self {
            pos: Vec::with_capacity(n_particles * dim),
            vel: vec![0.0; n_particles * dim],
            pbest: Vec::with_capacity(n_particles * dim),
            pbest_fit: Vec::with_capacity(n_particles),
            dim,
        }
    }

    fn initialize(&mut self, rng: &mut Rng, lb: &[f64], ub: &[f64]) {
        let mut rnd_buf = vec![0.0; self.dim];

        for _ in 0..N_PARTICLES {
            rng.fill_f64(&mut rnd_buf);
            for j in 0..self.dim {
                let p = lb[j] + rnd_buf[j] * (ub[j] - lb[j]);
                self.pos.push(p);
                self.pbest.push(p);
            }
        }

        // pbest_fitは呼び出し側で設定する
    }

    fn find_global_best<F>(&self, mut fitness: F) -> Vec<f64>
    where
        F: FnMut(&[f64]) -> f64,
    {
        let mut best_idx = 0;
        let mut best_fit = f64::MAX;
        for i in 0..N_PARTICLES {
            let fit = fitness(&self.pbest[i * self.dim..(i + 1) * self.dim]);
            if fit < best_fit {
                best_fit = fit;
                best_idx = i;
            }
        }
        self.pbest[best_idx * self.dim..(best_idx + 1) * self.dim].to_vec()
    }
}
