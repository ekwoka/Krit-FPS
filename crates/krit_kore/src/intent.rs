use bevy_ecs::component::Component;
use bevy_math::Vec2;
use bevy_reflect::Reflect;

use crate::aim::ViewAngles;

/// Represents the Intent of the Character
/// Can be written by Player input, Bots, Replays, etc
#[derive(Component, Clone, Default, Reflect, PartialEq, Debug)]
pub struct CharacterIntent {
    /// Local Space Movement Intent
    pub move_axis: Vec2,
    /// Absolute view intent
    pub aim: ViewAngles,
    /// Buttons Pressed/Held this frame
    pub buttons: Buttons,
}

/// Bitflag for currently held Buttons
#[derive(Default, Reflect, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Buttons(u32);

bitflags::bitflags! {
    impl Buttons: u32 {
        /// Jump is Pressed
        const JUMP = 0b00000001;
        /// Crouch is Pressed
        const CROUCH = 0b00000010;
        /// Sprint is Pressed
        const SPRINT = 0b00000100;
    }
}

/// Buttons Pressed/Held last tick
#[derive(Component, Clone, Default, Reflect, PartialEq)]
pub struct PrevButtons(pub Buttons);
