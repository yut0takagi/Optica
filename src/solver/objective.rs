//! 目的関数

/// Sphere関数（ループ展開最適化版）
#[allow(dead_code)]
#[inline(always)]
pub fn sphere(x: &[f64]) -> f64 {
    let mut sum = 0.0;
    let len = x.len();
    let ptr = x.as_ptr();

    // 16要素ずつ処理（ループ展開）
    let chunks_16 = len / 16;
    let remainder_16 = len % 16;

    unsafe {
        for i in 0..chunks_16 {
            let base = ptr.add(i * 16);
            sum += *base.add(0) * *base.add(0)
                + *base.add(1) * *base.add(1)
                + *base.add(2) * *base.add(2)
                + *base.add(3) * *base.add(3)
                + *base.add(4) * *base.add(4)
                + *base.add(5) * *base.add(5)
                + *base.add(6) * *base.add(6)
                + *base.add(7) * *base.add(7)
                + *base.add(8) * *base.add(8)
                + *base.add(9) * *base.add(9)
                + *base.add(10) * *base.add(10)
                + *base.add(11) * *base.add(11)
                + *base.add(12) * *base.add(12)
                + *base.add(13) * *base.add(13)
                + *base.add(14) * *base.add(14)
                + *base.add(15) * *base.add(15);
        }

        // 残り
        let rem_base = ptr.add(chunks_16 * 16);
        for i in 0..remainder_16 {
            let v = *rem_base.add(i);
            sum += v * v;
        }
    }

    sum
}
