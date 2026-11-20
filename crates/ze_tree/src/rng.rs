//! Repeatable random numbers compatible with ez-tree.

/// Upper limit that keeps rounded `f32` draws strictly below 1.0.
const BELOW_ONE: f32 = 1.0 - f32::EPSILON / 2.0;

/// A seeded pseudo-random number generator matching ez-tree's random sequence.
///
/// Use the same seed and the same order of method calls to repeat a sequence.
/// Each draw advances the generator once. Floating-point draws made with
/// [`next_f32`](Self::next_f32) have less precision than ez-tree's JavaScript values.
/// This generator is not suitable for cryptography.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rng {
    m_w: i32,
    m_z: i32,
}

impl Rng {
    /// Starts a new sequence from `seed`. Any `i32`, including a negative value, is valid.
    pub fn new(seed: i32) -> Self {
        Self {
            m_w: 123_456_789_i32.wrapping_add(seed),
            m_z: 987_654_321_i32.wrapping_sub(seed),
        }
    }

    /// Returns the next unsigned 32-bit value and advances the sequence once.
    pub fn next_u32(&mut self) -> u32 {
        // Update both state values using signed shifts, as JavaScript does. The
        // i64 sums avoid overflow before the casts retain the low 32 bits.
        self.m_z = (36_969 * i64::from(self.m_z & 0xffff) + i64::from(self.m_z >> 16)) as i32;
        self.m_w = (18_000 * i64::from(self.m_w & 0xffff) + i64::from(self.m_w >> 16)) as i32;

        // Join the low 16 bits of each state value into the next output.
        ((self.m_z << 16) as u32).wrapping_add((self.m_w & 0xffff) as u32)
    }

    /// Returns a value from 0.0 (inclusive) to 1.0 (exclusive), preserving all
    /// 32 bits of the random draw.
    pub fn next_f64(&mut self) -> f64 {
        f64::from(self.next_u32()) / 4_294_967_296.0
    }

    /// Returns a value from 0.0 (inclusive) to 1.0 (exclusive), rounded to `f32`.
    /// Values that would round to 1.0 are limited to the largest `f32` below it.
    pub fn next_f32(&mut self) -> f32 {
        unit_f32(self.next_u32())
    }

    /// Scales a random draw using `min + (max - min) * next_f32()`.
    ///
    /// For example, `range(-2.0, 6.0)` draws between -2.0 and 6.0. Reversed bounds
    /// are allowed, and rounding can include `max`. Use finite bounds whose
    /// difference fits in `f32`; other inputs can produce infinity or NaN.
    pub fn range(&mut self, min: f32, max: f32) -> f32 {
        (max - min) * self.next_f32() + min
    }
}

/// Converts a raw draw to `f32` in `[0, 1)`, clamping any rounded 1.0 downward.
fn unit_f32(raw: u32) -> f32 {
    ((f64::from(raw) / 4_294_967_296.0) as f32).min(BELOW_ONE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_f32_stays_below_one() {
        assert!(unit_f32(u32::MAX) < 1.0);
        assert_eq!(unit_f32(u32::MAX), BELOW_ONE);
        assert_eq!(unit_f32(0), 0.0);
        assert_eq!(unit_f32(1 << 31), 0.5);
    }

    #[test]
    fn draws_advance_one_step_each() {
        let mut a = Rng::new(7);
        let mut b = Rng::new(7);
        let raw = a.next_u32();
        assert_eq!(b.next_f64(), f64::from(raw) / 4_294_967_296.0);
        assert_eq!(a.next_f32(), unit_f32(b.next_u32()));
        assert_eq!(a, b);
    }

    #[test]
    fn range_maps_unit_interval() {
        let mut a = Rng::new(99);
        let mut b = Rng::new(99);
        let unit = b.next_f32();
        assert_eq!(a.range(-2.0, 6.0), 8.0 * unit - 2.0);
    }
}
