use bevy_ecs::component::Component;
use bevy_reflect::Reflect;
// Structs representing Aim related data

/// Maximum pitch magnitude in angle units (2^32 per turn).
/// Just under 90° so the aim never points straight up or down.
/// Shared by input, the aim stage and the camera so they clamp identically.
pub const PITCH_LIMIT: i32 =
    ((std::f64::consts::FRAC_PI_2 - 0.01) / std::f64::consts::TAU * 4_294_967_296.0) as i32;

/// Represents Intended View Angle
#[derive(Component, Clone, Default, Reflect, PartialEq, Debug)]
pub struct ViewAngles {
    /// View Yaw
    pub yaw: u32,
    /// View Pitch
    pub pitch: i32,
}

/// Aim accumulations between ticks
#[derive(Component, Clone, Default)]
pub struct LocalAim {
    /// Local Yaw
    pub yaw: f32,
    /// Local Pitch
    pub pitch: f32,
}

impl From<&LocalAim> for ViewAngles {
    fn from(local: &LocalAim) -> ViewAngles {
        ViewAngles {
            pitch: to_signed_units(local.pitch),
            yaw: to_units(local.yaw),
        }
    }
}

impl ViewAngles {
    /// Returns the internal Pitch as Radians
    pub fn pitch_rads(&self) -> f32 {
        signed_to_radians(self.pitch)
    }
    /// Returns the internal Yaw as Radians
    pub fn yaw_rads(&self) -> f32 {
        units_to_radians(self.yaw)
    }
}

/// Game injected change to ViewAngle
#[derive(Component, Clone, Default)]
pub struct ViewAngleOffset {
    /// Local Yaw
    pub yaw: u32,
    /// Local Pitch
    pub pitch: i32,
}

/// One Full 360 Turn
pub const FULL_TURN: f64 = 4_294_967_296.0;

fn to_units(rad: f32) -> u32 {
    ((rad as f64 / std::f64::consts::TAU).rem_euclid(1.0) * FULL_TURN) as u32
}

fn units_to_radians(units: u32) -> f32 {
    (units as f64 / FULL_TURN * std::f64::consts::TAU) as f32
}

fn to_signed_units(rad: f32) -> i32 {
    ((rad as f64 / std::f64::consts::TAU) * FULL_TURN) as i64 as i32
}

fn signed_to_radians(units: i32) -> f32 {
    (units as f64 / FULL_TURN * std::f64::consts::TAU) as f32
}
