/// `UnityEngine.Random`: xorshift128 seeded as `Random.InitState` seeds it.
#[derive(Clone, Debug)]
pub struct UnityRandom {
    s: [u32; 4],
}

impl UnityRandom {
    pub fn new(seed: u32) -> UnityRandom {
        let x = seed;
        let y = x.wrapping_mul(1812433253).wrapping_add(1);
        let z = y.wrapping_mul(1812433253).wrapping_add(1);
        let w = z.wrapping_mul(1812433253).wrapping_add(1);
        UnityRandom { s: [x, y, z, w] }
    }

    pub fn next_u32(&mut self) -> u32 {
        let [x, y, z, w] = self.s;
        let t = x ^ (x << 11);
        let nw = w ^ (w >> 19) ^ t ^ (t >> 8);
        self.s = [y, z, w, nw];
        nw
    }

    pub fn value(&mut self) -> f32 {
        (self.next_u32() & 0x007F_FFFF) as f32 * (1.0 / 8_388_607.0)
    }

    /// `Random.Range(float, float)`, both ends inclusive.
    pub fn range_float(&mut self, min: f32, max: f32) -> f32 {
        let t = self.value();
        min * t + (1.0 - t) * max
    }

    /// `Random.Range(int, int)`, the maximum exclusive.
    pub fn range_int(&mut self, min: i32, max: i32) -> i32 {
        if min < max {
            let range = (max as i64 - min as i64) as u32;
            (self.next_u32() % range) as i32 + min
        } else if min > max {
            let range = (min as i64 - max as i64) as u32;
            min - (self.next_u32() % range) as i32
        } else {
            min
        }
    }
}
