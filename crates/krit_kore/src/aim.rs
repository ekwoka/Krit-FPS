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

/// Game injected change to ViewAngle
#[derive(Component, Clone, Default)]
pub struct ViewAngleOffset {
    /// Local Yaw
    pub yaw: u32,
    /// Local Pitch
    pub pitch: i32,
}
