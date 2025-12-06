//! 高速乱数生成器（XorShift128+）

#[derive(Clone, Copy)]
pub struct Rng {
    s0: u64,
    s1: u64,
}

impl Rng {
    #[inline(always)]
    pub fn new(seed: u64) -> Self {
        Self {
            s0: seed,
            s1: seed.wrapping_mul(0x9E3779B97F4A7C15),
        }
    }

    #[inline(always)]
    fn next_u64(&mut self) -> u64 {
        let s0 = self.s0;
        let mut s1 = self.s1;
        let result = s0.wrapping_add(s1);
        s1 ^= s0;
        self.s0 = s0.rotate_left(24) ^ s1 ^ (s1 << 16);
        self.s1 = s1.rotate_left(37);
        result
    }

    #[inline(always)]
    pub fn f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    #[inline(always)]
    pub fn usize(&mut self, max: usize) -> usize {
        ((self.next_u64() as u128 * max as u128) >> 64) as usize
    }

    #[inline(always)]
    pub fn fill_f64(&mut self, out: &mut [f64]) {
        for x in out {
            *x = self.f64();
        }
    }
}
