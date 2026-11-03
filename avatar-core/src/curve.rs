use crate::model::{AnimationCurve, Keyframe, WeightedMode};

const DEFAULT_WEIGHT: f32 = 1.0 / 3.0;

impl AnimationCurve {
    /// Unity's evaluation, clamped to the first and last keys.
    pub fn evaluate(&self, time: f32) -> f32 {
        let keys = &self.keys;
        let Some(first) = keys.first() else { return 0.0 };
        if keys.len() == 1 || time <= first.time {
            return first.value;
        }
        let last = &keys[keys.len() - 1];
        if time >= last.time {
            return last.value;
        }
        let i = keys.partition_point(|k| k.time <= time).max(1);
        segment(&keys[i - 1], &keys[i], time)
    }
}

fn segment(a: &Keyframe, b: &Keyframe, time: f32) -> f32 {
    let span = b.time - a.time;
    if span <= 0.0 || !a.out_tangent.is_finite() || !b.in_tangent.is_finite() {
        return a.value;
    }
    let out_weighted = matches!(a.weighted_mode, WeightedMode::Out | WeightedMode::Both);
    let in_weighted = matches!(b.weighted_mode, WeightedMode::In | WeightedMode::Both);
    let s = (time - a.time) / span;
    if !out_weighted && !in_weighted {
        let (m0, m1) = (a.out_tangent * span, b.in_tangent * span);
        let (s2, s3) = (s * s, s * s * s);
        return (2.0 * s3 - 3.0 * s2 + 1.0) * a.value
            + (s3 - 2.0 * s2 + s) * m0
            + (s3 - s2) * m1
            + (-2.0 * s3 + 3.0 * s2) * b.value;
    }
    let w0 = if out_weighted { a.out_weight } else { DEFAULT_WEIGHT };
    let w1 = if in_weighted { b.in_weight } else { DEFAULT_WEIGHT };
    let (x1, x2) = (w0, 1.0 - w1);
    let (y0, y1) = (a.value, a.value + a.out_tangent * span * w0);
    let (y2, y3) = (b.value - b.in_tangent * span * w1, b.value);
    let bezier = |p0: f32, p1: f32, p2: f32, p3: f32, u: f32| {
        let v = 1.0 - u;
        v * v * v * p0 + 3.0 * v * v * u * p1 + 3.0 * v * u * u * p2 + u * u * u * p3
    };
    let (mut lo, mut hi) = (0.0f32, 1.0f32);
    for _ in 0..32 {
        let mid = 0.5 * (lo + hi);
        if bezier(0.0, x1, x2, 1.0, mid) < s {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    bezier(y0, y1, y2, y3, 0.5 * (lo + hi))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(time: f32, value: f32, slope: f32) -> Keyframe {
        Keyframe {
            time,
            value,
            in_tangent: slope,
            out_tangent: slope,
            in_weight: DEFAULT_WEIGHT,
            out_weight: DEFAULT_WEIGHT,
            weighted_mode: WeightedMode::None,
        }
    }

    #[test]
    fn hermite_linear_stepped_and_weighted() {
        let line = AnimationCurve { keys: vec![key(0.0, 1.0, -0.5), key(1.0, 0.5, -0.5)] };
        for t in [0.0, 0.25, 0.5, 1.0] {
            assert!((line.evaluate(t) - (1.0 - 0.5 * t)).abs() < 1e-6);
        }
        assert_eq!(line.evaluate(-1.0), 1.0);
        assert_eq!(line.evaluate(2.0), 0.5);
        let mut stepped = line.clone();
        stepped.keys[0].out_tangent = f32::INFINITY;
        assert_eq!(stepped.evaluate(0.7), 1.0);
        let mut weighted = line.clone();
        weighted.keys[0].weighted_mode = WeightedMode::Out;
        assert!((weighted.evaluate(0.5) - 0.75).abs() < 1e-4);
        let ease = AnimationCurve { keys: vec![key(0.0, 0.0, 0.0), key(1.0, 1.0, 0.0)] };
        assert!((ease.evaluate(0.5) - 0.5).abs() < 1e-6);
        assert!((ease.evaluate(0.25) - 0.15625).abs() < 1e-6);
    }
}
