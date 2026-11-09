//! Sunlight Shimmer's light follows the handheld: tilt it and the glare
//! and rainbow move over the screen, as a real light would. The way the
//! handheld is held is its rest position (the light in the middle); the
//! light moves with every tilt away from it, and the rest slowly follows
//! a new way of holding it.

/// How far the light travels for a tilt: Matt Akins' default range 0.6
/// of the picture for a full tilt.
const RANGE: f32 = 0.6;
/// A tilt of about 30 degrees takes the light to the end of its range.
const SENSITIVITY: f32 = 2.0;
/// The rest position follows the handheld in about three seconds.
const REST_FOLLOW: f32 = 0.006;
/// The light eases to where it goes, without the sensor's jitter.
const EASE: f32 = 0.2;

#[derive(Debug, Clone, Copy)]
pub struct TiltLight {
    rest: Option<[f32; 2]>,
    at: [f32; 2],
}

impl Default for TiltLight {
    fn default() -> Self {
        Self { rest: None, at: [0.5, 0.5] }
    }
}

impl TiltLight {
    /// Back to the middle: the next reading is the new rest position.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// The light, 0..1 over the picture, for one frame. `gravity` is the
    /// accelerometer along the screen's X, Y and Z (None without one: the
    /// light stays in the middle, Matt Akins' default).
    pub fn step(&mut self, gravity: Option<[f32; 3]>) -> [f32; 2] {
        let Some([x, y, z]) = gravity else { return self.at };
        let n = (x * x + y * y + z * z).sqrt();
        if n < 0.1 { return self.at; }
        let now = [x / n, y / n];
        let rest = self.rest.get_or_insert(now);
        for i in 0..2 { rest[i] += (now[i] - rest[i]) * REST_FOLLOW; }
        let d = [0, 1].map(|i| ((now[i] - rest[i]) * SENSITIVITY).clamp(-1.0, 1.0));
        // The light moves toward the edge that goes up.
        let target = [0.5 - d[0] * RANGE, 0.5 + d[1] * RANGE];
        for i in 0..2 { self.at[i] += (target[i] - self.at[i]) * EASE; }
        self.at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_light_starts_in_the_middle_and_moves_with_a_tilt() {
        let mut t = TiltLight::default();
        assert_eq!(t.step(None), [0.5, 0.5]);
        // Held at an angle: that is the rest position.
        let held = [0.0, 0.7, 0.7];
        for _ in 0..10 { assert!((t.step(Some(held))[0] - 0.5).abs() < 1e-6); }
        // Tilted to one side: the light goes the other way along X.
        let tilted = [0.3, 0.7, 0.65];
        let mut at = [0.5, 0.5];
        for _ in 0..30 { at = t.step(Some(tilted)); }
        assert!(at[0] < 0.4, "{at:?}");
        assert!((0.0..=1.0).contains(&at[0]) && (0.0..=1.0).contains(&at[1]));
    }

    #[test]
    fn a_new_way_of_holding_it_becomes_the_rest() {
        let mut t = TiltLight::default();
        t.step(Some([0.0, 0.7, 0.7]));
        let mut at = [0.5, 0.5];
        for _ in 0..3000 { at = t.step(Some([0.2, 0.7, 0.68])); }
        assert!((at[0] - 0.5).abs() < 0.02, "{at:?}");
    }
}
