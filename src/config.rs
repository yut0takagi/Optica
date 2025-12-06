//! 設定と定数

/// デフォルト値
pub const DEFAULT_MAX_ITER: usize = 1000;

/// ソルバー内部定数
pub const POP_SIZE: usize = 50;
pub const N_PARTICLES: usize = 50;

/// ソルバーパラメータ
pub const DE_F: f64 = 0.8;
pub const DE_CR: f64 = 0.9;
pub const PSO_C1: f64 = 2.0;
pub const PSO_C2: f64 = 2.0;
pub const PSO_W_INIT: f64 = 0.9;
pub const PSO_W_MIN: f64 = 0.4;
pub const PSO_W_DECAY: f64 = 0.995;

/// 収束判定
pub const TOLERANCE: f64 = 1e-10;
pub const DISPLAY_TOLERANCE: f64 = 1e-6;

/// 並列化の閾値
pub const PARALLEL_MIN_DIM: usize = 50;
pub const PARALLEL_MIN_ITER: usize = 200;
