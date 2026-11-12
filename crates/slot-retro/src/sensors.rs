//! Device motion and light for cores, through the libretro sensor
//! interface. Android writes the readings; the core reads them on the
//! emulation thread. Units and axes follow RetroArch's Android driver:
//! acceleration in g, rotation rate in rad/s, X/Y turned to the way the
//! handheld is held.
use std::ffi::c_uint;
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use std::sync::Mutex;

pub const ACCELEROMETER: u8 = 1;
pub const GYROSCOPE: u8 = 2;
pub const ILLUMINANCE: u8 = 4;

const GRAVITY: f32 = 9.80665;

// retro_sensor_action and the RETRO_SENSOR_* input ids.
const ACCELEROMETER_ENABLE: c_uint = 0;
const ACCELEROMETER_DISABLE: c_uint = 1;
const GYROSCOPE_ENABLE: c_uint = 2;
const GYROSCOPE_DISABLE: c_uint = 3;
const ILLUMINANCE_ENABLE: c_uint = 4;
const ILLUMINANCE_DISABLE: c_uint = 5;
const ACCELEROMETER_X: c_uint = 0;
const ACCELEROMETER_Z: c_uint = 2;
const GYROSCOPE_X: c_uint = 3;
const GYROSCOPE_Z: c_uint = 5;
const ILLUMINANCE_ID: c_uint = 6;

static AVAILABLE: AtomicU8 = AtomicU8::new(0);
// How many loaded cores want each sensor: the Run-Ahead and link
// instances enable and disable on their own.
static WANTED: [AtomicU32; 3] = [AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0)];
// f32 bits: raw accelerometer xyz (m/s²), gyroscope xyz, illuminance.
static VALUES: [AtomicU32; 7] = [const { AtomicU32::new(0) }; 7];
// Screen rotation as RetroArch detects it (0..3, clockwise quarters).
static ROTATION: AtomicU8 = AtomicU8::new(0);
static GRAVITY_SAMPLES: Mutex<Calibration> = Mutex::new(Calibration::new());
// A solar cartridge reading the light sensor through slot., not a core.
static LIGHT_WANTED: AtomicU8 = AtomicU8::new(0);
// mGBA's MBC7 (Kirby Tilt 'n' Tumble) negates both tilt axes that its
// GBA tilt cartridges take as they are.
static TILT_INVERTED: AtomicU8 = AtomicU8::new(0);
/// slot. itself reads the accelerometer (Sunlight Shimmer's light).
static TILT_WANTED: AtomicU8 = AtomicU8::new(0);
// A handheld without a gyroscope (the KONKR has only an accelerometer):
// cores still get one, its Z rotation worked out from the accelerometer.
static GYRO_FROM_TILT: AtomicU8 = AtomicU8::new(0);
static TILT_GYRO: Mutex<TiltGyro> = Mutex::new(TiltGyro::new());
// Motion games' sensitivity, f32 bits: what cores read is scaled by it.
static SENSITIVITY: AtomicU32 = AtomicU32::new(0x3f80_0000); // 1.0

/// Below this (m/s²) gravity is mostly across the screen: a handheld lying
/// flat, whose turn the accelerometer can't see.
const TILT_GYRO_MIN: f32 = 3.0;
/// Smoothing of gravity and of the rotation rate, in seconds: enough to
/// take out a shaking hand, short enough to follow a quick turn.
const TILT_GYRO_GRAVITY_TAU: f32 = 0.03;
const TILT_GYRO_RATE_TAU: f32 = 0.05;
/// A hand holding the handheld up shakes by about 4 degrees/s, which a
/// gyroscope worked out from the accelerometer reads as turning. Below
/// ~7 degrees/s upright (before sensitivity) the cartridge reads rest, and
/// faster turns lose that much so they still start from nothing; the angle
/// is noisier when less gravity lies in the screen plane. Only the synthetic
/// gyro: never tilt games, a real gyroscope or Sunlight Shimmer.
const TILT_GYRO_DEADZONE: f32 = 0.12;
// mGBA multiplies rad/s by -5.5e8 into an int32_t. Keep the synthetic
// value in range even after high sensitivity or a very quick turn.
const TILT_GYRO_LIMIT: f32 = 3.9;

/// The turn around the screen's axis (gyroscope Z), from how gravity turns
/// in the screen's plane: held up like a wheel, as WarioWare: Twisted! is
/// played, the accelerometer sees the whole turn.
struct TiltGyro {
    at: Option<u64>,
    x: f32,
    y: f32,
    angle: Option<f32>,
    rate: f32,
}

impl TiltGyro {
    const fn new() -> Self {
        Self { at: None, x: 0.0, y: 0.0, angle: None, rate: 0.0 }
    }

    /// Keep the timestamp after update: its first-sample and long-gap
    /// reset clears all state, including `at`. Storing it before update
    /// would make every reading look like the first one, always yielding 0.
    fn sample(&mut self, x: f32, y: f32, timestamp_ns: u64) -> f32 {
        // Duplicate or out-of-order Android samples must not corrupt the
        // reference or turn a tiny delivery interval into a rotation spike.
        if self.at.is_some_and(|before| timestamp_ns <= before) {
            return self.output();
        }
        let dt = self.at.map(|before| (timestamp_ns - before) as f32 * 1e-9);
        let rate = self.update(x, y, dt);
        self.at = Some(timestamp_ns);
        rate
    }

    fn output(&self) -> f32 {
        let projection = self.x.hypot(self.y).clamp(TILT_GYRO_MIN, GRAVITY);
        let deadzone = TILT_GYRO_DEADZONE * GRAVITY / projection;
        (self.rate.abs() - deadzone).max(0.0).copysign(self.rate)
    }

    /// One accelerometer reading (m/s², the device's own X and Y) `dt`
    /// seconds after the last: the rotation rate in rad/s, counter-clockwise
    /// positive as Android's gyroscope reports it.
    fn update(&mut self, x: f32, y: f32, dt: Option<f32>) -> f32 {
        let Some(dt) = dt.filter(|dt| *dt > 0.0 && *dt <= 0.25) else {
            *self = Self { x, y, ..Self::new() };
            return 0.0;
        };
        let a = dt / (TILT_GYRO_GRAVITY_TAU + dt);
        self.x += a * (x - self.x);
        self.y += a * (y - self.y);
        if self.x.hypot(self.y) < TILT_GYRO_MIN {
            self.angle = None;
            self.rate = 0.0;
            return 0.0;
        }
        // Gravity seen from the screen turns as much as the handheld,
        // the other way: the angle of "up" grows with a left turn.
        let angle = self.x.atan2(self.y);
        let turned = match self.angle.replace(angle) {
            Some(before) => {
                let d = angle - before;
                (d + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
            }
            None => 0.0,
        };
        let b = dt / (TILT_GYRO_RATE_TAU + dt);
        self.rate += b * (turned / dt - self.rate);
        // Keep the unrounded filter state so slow, deliberate turns can
        // build up normally. Only the value sent to the core has a deadzone.
        self.output()
    }
}

struct Calibration {
    done: bool,
    x: f32,
    y: f32,
    count: u32,
}

impl Calibration {
    const fn new() -> Self {
        Self { done: false, x: 0.0, y: 0.0, count: 0 }
    }

    /// RetroArch's gravity check: average 30 samples and, once the
    /// handheld is tilted at least ~30° from flat, take the axis gravity
    /// pulls on as "down". A flat handheld starts over.
    fn sample(&mut self, x: f32, y: f32) -> Option<u8> {
        if self.done {
            return None;
        }
        self.x += x;
        self.y += y;
        self.count += 1;
        if self.count < 30 {
            return None;
        }
        let (x, y) = (self.x / 30.0, self.y / 30.0);
        *self = Self::new();
        if x.abs() <= 5.0 && y.abs() <= 5.0 {
            return None;
        }
        self.done = true;
        Some(if y.abs() >= x.abs() {
            if y > 0.0 { 0 } else { 2 }
        } else if x < 0.0 { 1 } else { 3 })
    }
}

fn bit_index(sensor: u8) -> usize {
    match sensor {
        ACCELEROMETER => 0,
        GYROSCOPE => 1,
        _ => 2,
    }
}

fn store(index: usize, value: f32) {
    VALUES[index].store(value.to_bits(), Ordering::Relaxed);
}

fn load(index: usize) -> f32 {
    f32::from_bits(VALUES[index].load(Ordering::Relaxed))
}

/// Sensors this device has, as a mask of ACCELEROMETER, GYROSCOPE and
/// ILLUMINANCE. A core is refused any sensor that is not listed, except
/// the gyroscope: without one, the accelerometer stands in for it.
pub fn set_available(mask: u8) {
    let from_tilt = mask & ACCELEROMETER != 0 && mask & GYROSCOPE == 0;
    GYRO_FROM_TILT.store(from_tilt as u8, Ordering::Release);
    AVAILABLE.store(if from_tilt { mask | GYROSCOPE } else { mask }, Ordering::Release);
}

/// Whether the gyroscope is worked out from the accelerometer.
pub fn gyro_from_tilt() -> bool {
    GYRO_FROM_TILT.load(Ordering::Acquire) != 0
}

/// How strongly motion games feel a tilt or a turn: 1.0 as the sensors
/// read it.
pub fn set_sensitivity(factor: f32) {
    let factor = if factor.is_finite() && factor > 0.0 { factor } else { 1.0 };
    SENSITIVITY.store(factor.to_bits(), Ordering::Relaxed);
}

fn sensitivity() -> f32 {
    f32::from_bits(SENSITIVITY.load(Ordering::Relaxed))
}

pub fn available() -> u8 {
    AVAILABLE.load(Ordering::Acquire)
}

/// Sensors a loaded core has switched on, for Android to listen to.
pub fn wanted() -> u8 {
    [ACCELEROMETER, GYROSCOPE, ILLUMINANCE]
        .into_iter()
        .filter(|&s| WANTED[bit_index(s)].load(Ordering::Acquire) > 0)
        .fold(0, |mask, s| mask | s)
        | if LIGHT_WANTED.load(Ordering::Acquire) != 0 { ILLUMINANCE } else { 0 }
        | if TILT_WANTED.load(Ordering::Acquire) != 0 { ACCELEROMETER } else { 0 }
        | if gyro_from_tilt() && WANTED[bit_index(GYROSCOPE)].load(Ordering::Acquire) > 0 {
            ACCELEROMETER
        } else { 0 }
}

/// Keep the accelerometer on for slot.'s own use (Sunlight Shimmer).
pub fn set_tilt_wanted(wanted: bool) {
    let wanted = wanted && available() & ACCELEROMETER != 0;
    if TILT_WANTED.swap(wanted as u8, Ordering::AcqRel) == 0 && wanted
        && WANTED[bit_index(ACCELEROMETER)].load(Ordering::Acquire) == 0 {
        begin(ACCELEROMETER);
    }
}

/// The accelerometer in g along the screen's X, Y and Z, turned as games
/// read it. None before the first reading.
pub fn gravity() -> Option<[f32; 3]> {
    let raw = [load(0), load(1), load(2)];
    if raw == [0.0; 3] {
        return None;
    }
    let rotation = ROTATION.load(Ordering::Relaxed);
    Some([ACCELEROMETER_X, ACCELEROMETER_X + 1, ACCELEROMETER_Z].map(|id| turned(id, rotation, false)))
}

/// Keep the light sensor on for a solar cartridge whose Sun is SENSOR.
pub fn set_light_wanted(wanted: bool) {
    LIGHT_WANTED.store(wanted as u8, Ordering::Release);
}

/// The light sensor's last reading, in lux.
pub fn illuminance() -> f32 {
    load(6)
}

pub fn set_tilt_inverted(inverted: bool) {
    TILT_INVERTED.store(inverted as u8, Ordering::Release);
}

/// An Android accelerometer reading and its SensorEvent timestamp in ns.
pub fn push_accelerometer(x: f32, y: f32, z: f32, timestamp_ns: u64) {
    store(0, x);
    store(1, y);
    store(2, z);
    let rotation = GRAVITY_SAMPLES.lock().unwrap_or_else(|e| e.into_inner()).sample(x, y);
    if let Some(rotation) = rotation {
        ROTATION.store(rotation, Ordering::Relaxed);
    }
    if gyro_from_tilt() && WANTED[bit_index(GYROSCOPE)].load(Ordering::Acquire) > 0 {
        let mut gyro = TILT_GYRO.lock().unwrap_or_else(|e| e.into_inner());
        let rate = gyro.sample(x, y, timestamp_ns);
        store(3, 0.0);
        store(4, 0.0);
        store(5, rate);
    }
}

pub fn push_gyroscope(x: f32, y: f32, z: f32) {
    store(3, x);
    store(4, y);
    store(5, z);
}

pub fn push_illuminance(lux: f32) {
    store(6, lux);
}

/// One core's switched-on sensors, released when the core unloads.
#[derive(Default)]
pub(crate) struct Enabled(u8);

impl Enabled {
    fn set(&mut self, sensor: u8, on: bool) -> bool {
        if on && available() & sensor == 0 {
            return false;
        }
        if on == (self.0 & sensor != 0) {
            return true;
        }
        let count = &WANTED[bit_index(sensor)];
        if on {
            self.0 |= sensor;
            if count.fetch_add(1, Ordering::AcqRel) == 0 {
                begin(sensor);
            }
        } else {
            self.0 &= !sensor;
            count.fetch_sub(1, Ordering::AcqRel);
        }
        true
    }

    pub(crate) fn action(&mut self, action: c_uint) -> bool {
        match action {
            ACCELEROMETER_ENABLE => self.set(ACCELEROMETER, true),
            ACCELEROMETER_DISABLE => self.set(ACCELEROMETER, false),
            GYROSCOPE_ENABLE => self.set(GYROSCOPE, true),
            GYROSCOPE_DISABLE => self.set(GYROSCOPE, false),
            ILLUMINANCE_ENABLE => self.set(ILLUMINANCE, true),
            ILLUMINANCE_DISABLE => self.set(ILLUMINANCE, false),
            _ => false,
        }
    }

    pub(crate) fn release(&mut self) {
        for sensor in [ACCELEROMETER, GYROSCOPE, ILLUMINANCE] {
            self.set(sensor, false);
        }
    }

    pub(crate) fn input(&self, id: c_uint) -> f32 {
        let sensor = match id {
            ACCELEROMETER_X..=ACCELEROMETER_Z => ACCELEROMETER,
            GYROSCOPE_X..=GYROSCOPE_Z => GYROSCOPE,
            ILLUMINANCE_ID => ILLUMINANCE,
            _ => return 0.0,
        };
        if self.0 & sensor == 0 {
            return 0.0;
        }
        let value = read(id, ROTATION.load(Ordering::Relaxed));
        if sensor == ILLUMINANCE { return value; }
        let value = value * sensitivity();
        if id == GYROSCOPE_Z && gyro_from_tilt() {
            value.clamp(-TILT_GYRO_LIMIT, TILT_GYRO_LIMIT)
        } else { value }
    }
}

impl Drop for Enabled {
    fn drop(&mut self) {
        self.release();
    }
}

/// A sensor switched on from nothing starts from rest, and the
/// accelerometer looks for the screen's orientation again.
fn begin(sensor: u8) {
    match sensor {
        ACCELEROMETER => {
            (0..3).for_each(|i| store(i, 0.0));
            *GRAVITY_SAMPLES.lock().unwrap_or_else(|e| e.into_inner()) = Calibration::new();
            ROTATION.store(0, Ordering::Relaxed);
        }
        GYROSCOPE => {
            (3..6).for_each(|i| store(i, 0.0));
            *TILT_GYRO.lock().unwrap_or_else(|e| e.into_inner()) = TiltGyro::new();
        }
        _ => store(6, 0.0),
    }
}

/// RetroArch's input_get_sensor_state on Android: X and Y follow the
/// detected rotation (90°: X->Y, Y->-X), Z and light are untouched.
fn read(id: c_uint, rotation: u8) -> f32 {
    turned(id, rotation, true)
}

fn turned(id: c_uint, rotation: u8, invertible: bool) -> f32 {
    let (base, scale) = match id {
        ACCELEROMETER_X..=ACCELEROMETER_Z => (0, 1.0 / GRAVITY),
        GYROSCOPE_X..=GYROSCOPE_Z => (3, 1.0),
        _ => return load(6),
    };
    let axis = (id - if base == 0 { ACCELEROMETER_X } else { GYROSCOPE_X }) as usize;
    let (axis, sign) = match (axis, rotation) {
        (2, _) | (_, 0) => (axis, 1.0),
        (0, 1) => (1, 1.0),
        (1, 1) => (0, -1.0),
        (_, 2) => (axis, -1.0),
        (0, _) => (1, -1.0),
        _ => (0, 1.0),
    };
    let inverted = invertible && base == 0 && axis < 2 && TILT_INVERTED.load(Ordering::Relaxed) != 0;
    load(base + axis) * scale * if inverted { -sign } else { sign }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the cartridge reads for a steady upright turn of `rate` rad/s.
    fn felt(rate: f32) -> f32 {
        (rate.abs() - TILT_GYRO_DEADZONE).max(0.0).copysign(rate)
    }

    // Tests that exercise the public sensor bridge share its atomics.
    static SENSOR_TEST_STATE: Mutex<()> = Mutex::new(());

    #[test]
    fn calibration_picks_the_axis_gravity_pulls_on() {
        let detect = |x: f32, y: f32| {
            let mut c = Calibration::new();
            (0..30).find_map(|_| c.sample(x, y))
        };
        assert_eq!(detect(0.0, 8.0), Some(0));
        assert_eq!(detect(0.0, -8.0), Some(2));
        assert_eq!(detect(-8.0, 1.0), Some(1));
        assert_eq!(detect(8.0, 1.0), Some(3));
        assert_eq!(detect(1.0, 2.0), None, "a flat handheld is not calibrated");
    }

    #[test]
    fn calibration_happens_once() {
        let mut c = Calibration::new();
        assert_eq!((0..30).find_map(|_| c.sample(0.0, 9.0)), Some(0));
        assert_eq!((0..30).find_map(|_| c.sample(-9.0, 0.0)), None);
    }

    #[test]
    fn readings_follow_retroarch_rotation() {
        let _guard = SENSOR_TEST_STATE.lock().unwrap();
        push_gyroscope(1.0, 2.0, 3.0);
        let gyro = |rotation| [3, 4, 5].map(|id| read(id, rotation));
        assert_eq!(gyro(0), [1.0, 2.0, 3.0]);
        assert_eq!(gyro(1), [2.0, -1.0, 3.0]);
        assert_eq!(gyro(2), [-1.0, -2.0, 3.0]);
        assert_eq!(gyro(3), [-2.0, 1.0, 3.0]);
    }

    #[test]
    fn a_turn_held_up_like_a_wheel_gives_the_gyroscope_rate() {
        let mut gyro = TiltGyro::new();
        let dt = 0.01;
        gyro.update(0.0, GRAVITY, None);
        // A steady left turn of 1 rad/s: "up" seen from the screen turns
        // the same way, counter-clockwise.
        let mut rate = 0.0;
        for step in 1..=100 {
            let angle = step as f32 * dt;
            rate = gyro.update(GRAVITY * angle.sin(), GRAVITY * angle.cos(), Some(dt));
        }
        assert!((rate - felt(1.0)).abs() < 0.05, "{rate}");
        // Held still: the rate settles back to nothing.
        for _ in 0..100 { rate = gyro.update(GRAVITY, 0.0 * GRAVITY, Some(dt)); }
        assert!(rate.abs() < 0.05, "{rate}");
        // Lying flat, the turn can't be seen: no rotation, no drift.
        for _ in 0..50 { rate = gyro.update(0.5, 0.5, Some(dt)); }
        assert_eq!(rate, 0.0);
        // A pause in the readings starts over instead of a jump.
        assert_eq!(gyro.update(0.0, GRAVITY, Some(2.0)), 0.0);
    }

    #[test]
    fn a_core_gets_only_sensors_the_device_has_and_lets_them_go() {
        let _guard = SENSOR_TEST_STATE.lock().unwrap();
        set_available(ACCELEROMETER | GYROSCOPE);
        set_available(ACCELEROMETER);
        assert!(gyro_from_tilt(), "the accelerometer stands in for the gyroscope");
        set_available(ILLUMINANCE);
        assert!(!gyro_from_tilt());
        let mut core = Enabled::default();
        assert!(!core.action(GYROSCOPE_ENABLE), "nothing to work it out from");
        set_available(ACCELEROMETER);
        assert!(core.action(ACCELEROMETER_ENABLE));
        let mut other = Enabled::default();
        assert!(other.action(ACCELEROMETER_ENABLE));
        assert_eq!(wanted(), ACCELEROMETER);
        drop(other);
        assert_eq!(wanted(), ACCELEROMETER, "still wanted by the first core");
        push_accelerometer(0.0, GRAVITY, 0.0, 0);
        assert!((core.input(1) - 1.0).abs() < 1e-6);
        assert_eq!(core.input(3), 0.0, "gyroscope was refused");
        core.release();
        assert_eq!(wanted(), 0);
        assert_eq!(core.input(1), 0.0);
    }

    #[test]
    fn timestamped_turns_start_and_resume_after_a_gap() {
        for direction in [-1.0_f32, 1.0] {
            let mut gyro = TiltGyro::new();
            let start = 10_000_000_000_u64;
            assert_eq!(gyro.sample(0.0, GRAVITY, start), 0.0);
            assert_eq!(gyro.at, Some(start), "the first reading must keep its timestamp");

            let mut rate = 0.0;
            for step in 1..=100_u64 {
                let angle = direction * step as f32 * 0.01;
                rate = gyro.sample(GRAVITY * angle.sin(), GRAVITY * angle.cos(),
                    start + step * 10_000_000);
            }
            assert!((rate - felt(direction)).abs() < 0.05, "timestamped turn: {rate}");

            // Resume at the same angle after a one-second pause. The gap
            // must give no jump and must not prevent subsequent motion.
            let resumed = start + 2_000_000_000;
            assert_eq!(gyro.sample(GRAVITY * direction.sin(),
                GRAVITY * direction.cos(), resumed), 0.0);
            assert_eq!(gyro.at, Some(resumed), "a gap must keep the new timestamp");
            for step in 1..=100_u64 {
                let angle = direction * (1.0 + step as f32 * 0.01);
                rate = gyro.sample(GRAVITY * angle.sin(), GRAVITY * angle.cos(),
                    resumed + step * 10_000_000);
            }
            assert!((rate - felt(direction)).abs() < 0.05, "turn after gap: {rate}");

            for step in 101..=200_u64 {
                let angle = direction * 2.0;
                rate = gyro.sample(GRAVITY * angle.sin(), GRAVITY * angle.cos(),
                    resumed + step * 10_000_000);
            }
            assert!(rate.abs() < 0.001, "held still: {rate}");
        }
    }

    #[test]
    fn noisy_rest_is_zero_in_every_upright_orientation() {
        for projection in [GRAVITY, 5.0] {
            for angle in [0.0, std::f32::consts::FRAC_PI_2,
                -std::f32::consts::FRAC_PI_2, std::f32::consts::PI] {
                for period_ns in [10_000_000, 20_000_000, 50_000_000] {
                    let mut gyro = TiltGyro::new();
                    // Synthetic stationary accelerometer noise, +/-0.04
                    // m/s²: no physical rotation, but a derivative alone
                    // sends changing gyro values and disturbs calibration.
                    for step in 0..500_u64 {
                        let x = projection * angle.sin()
                            + 0.04 * ((step * 17 % 11) as f32 - 5.0) / 5.0;
                        let y = projection * angle.cos()
                            + 0.04 * ((step * 7 % 13) as f32 - 6.0) / 6.0;
                        let rate = gyro.sample(x, y, step * period_ns);
                        // Startup noise can briefly settle; afterwards the
                        // cartridge must see exactly zero, not just near zero.
                        if step > 25 {
                            assert_eq!(rate, 0.0,
                                "projection={projection} angle={angle} period={period_ns}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn sensor_timestamps_handle_batched_wraparound_and_stale_events() {
        for direction in [-1.0_f32, 1.0] {
            for speed in [0.1, 1.0, 2.0] {
                let mut gyro = TiltGyro::new();
                gyro.sample(0.0, GRAVITY, 0);
                let mut rate = 0.0;
                // Deliver immediately in a tight loop. The sensor timestamps,
                // not callback scheduling, describe the actual 50 Hz motion.
                for step in 1..=200_u64 {
                    let angle = direction * speed * step as f32 * 0.02;
                    let x = GRAVITY * angle.sin();
                    let y = GRAVITY * angle.cos();
                    let at = step * 20_000_000;
                    rate = gyro.sample(x, y, at);
                    assert_eq!(gyro.sample(-x, -y, at), rate, "ignore duplicate");
                    assert_eq!(gyro.sample(-x, -y, at - 1), rate, "ignore old sample");
                }
                assert!((rate - felt(direction * speed)).abs() < 0.02,
                    "speed={speed} direction={direction} got={rate}");
            }
        }
    }

    #[test]
    fn core_motion_resumes_after_flat_calibration_and_returns_to_rest() {
        let _guard = SENSOR_TEST_STATE.lock().unwrap();
        set_available(ACCELEROMETER);
        set_sensitivity(1.0);
        let mut core = Enabled::default();
        assert!(core.action(GYROSCOPE_ENABLE));
        assert_eq!(wanted(), ACCELEROMETER | GYROSCOPE);
        let mut at = 0_u64;
        let mut sample = |x: f32, y: f32, z: f32| {
            at += 20_000_000;
            push_accelerometer(x, y, z, at);
            core.input(GYROSCOPE_Z)
        };
        // Calibrate flat, then lift toward upright without twisting.
        for _ in 0..50 { assert_eq!(sample(0.0, 0.0, GRAVITY), 0.0); }
        for step in 1..=50 {
            let lift = step as f32 / 50.0 * std::f32::consts::FRAC_PI_2;
            assert_eq!(sample(0.0, GRAVITY * lift.sin(), GRAVITY * lift.cos()), 0.0);
        }
        for _ in 0..50 { assert_eq!(sample(0.0, GRAVITY, 0.0), 0.0); }

        let mut angle = 0.0_f32;
        for direction in [-1.0_f32, 1.0] {
            let mut rate = 0.0;
            for _ in 0..100 {
                angle += direction * 0.02;
                rate = sample(GRAVITY * angle.sin(), GRAVITY * angle.cos(), 0.0);
            }
            assert!((rate - felt(direction)).abs() < 0.02, "core turn: {rate}");
            // The sensitivity control still scales the synthetic gyro.
            set_sensitivity(2.0);
            let scaled = sample(GRAVITY * (angle + direction * 0.02).sin(),
                GRAVITY * (angle + direction * 0.02).cos(), 0.0);
            angle += direction * 0.02;
            assert!((scaled - felt(direction) * 2.0).abs() < 0.04, "sensitivity: {scaled}");
            set_sensitivity(4.0);
            for _ in 0..50 {
                rate = sample(GRAVITY * angle.sin(), GRAVITY * angle.cos(), 0.0);
            }
            assert_eq!(rate, 0.0, "rest must be exact even at maximum sensitivity");
            set_sensitivity(1.0);
        }

        set_sensitivity(4.0);
        for direction in [-1.0_f32, 1.0] {
            let mut rate = 0.0;
            for _ in 0..100 {
                angle += direction * 0.06;
                rate = sample(GRAVITY * angle.sin(), GRAVITY * angle.cos(), 0.0);
                assert!(rate.is_finite() && rate.abs() <= TILT_GYRO_LIMIT);
            }
            assert_eq!(rate, direction * TILT_GYRO_LIMIT, "high sensitivity saturates safely");
        }
        set_sensitivity(1.0);
        core.release();
    }

    #[test]
    fn sensitivity_preserves_tilt_real_gyro_shader_gravity_and_light() {
        let _guard = SENSOR_TEST_STATE.lock().unwrap();
        set_available(ACCELEROMETER | GYROSCOPE | ILLUMINANCE);
        assert!(!gyro_from_tilt(), "a real gyroscope must not use the fallback");
        set_tilt_inverted(false);
        set_sensitivity(2.0);
        let mut core = Enabled::default();
        assert!(core.action(ACCELEROMETER_ENABLE));
        assert!(core.action(GYROSCOPE_ENABLE));
        assert!(core.action(ILLUMINANCE_ENABLE));

        push_accelerometer(GRAVITY * 0.25, GRAVITY * 0.5, GRAVITY * 0.75, 0);
        push_gyroscope(0.125, -0.25, 0.5);
        push_illuminance(37.0);
        let close = |actual: f32, expected: f32| {
            assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
        };
        for (id, expected) in [0.5, 1.0, 1.5, 0.25, -0.5, 1.0, 37.0]
            .into_iter().enumerate() {
            close(core.input(id as c_uint), expected);
        }
        for (actual, expected) in gravity().unwrap().into_iter().zip([0.25, 0.5, 0.75]) {
            close(actual, expected);
        }

        // Kirby's MBC7 axis correction remains independent of the gyro.
        set_tilt_inverted(true);
        close(core.input(ACCELEROMETER_X), -0.5);
        close(core.input(ACCELEROMETER_X + 1), -1.0);
        close(core.input(ACCELEROMETER_Z), 1.5);
        close(core.input(GYROSCOPE_Z), 1.0);
        close(gravity().unwrap()[0], 0.25);

        push_gyroscope(0.0, 0.0, 0.01);
        close(core.input(GYROSCOPE_Z), 0.02);

        set_tilt_inverted(false);
        set_sensitivity(1.0);
        core.release();
    }
}
