#![warn(missing_docs)]

//! Provides the CharacterController and other movement related logic

use bevy_app::{App, Plugin};
use bevy_ecs::component::Component;

use crate::{aim::KritAimPlugin, movement::KritMovementPlugin};

/// Handles Character Aim Direction
pub mod aim;
/// Handles Character Motion
pub mod movement;

/// Enables KritMovement core logic
/// # Adding
/// ```
/// use bevy::prelude::*;
/// use krit_input::KritInput;
///
/// let mut app = App::new();
/// app.add_plugins(KritInput);
/// ```
pub struct KritMovement;

impl Plugin for KritMovement {
    fn build(&self, app: &mut App) {
        app.add_plugins((KritAimPlugin, KritMovementPlugin));
    }
}

/// Character Stamina
#[derive(Component, Default, Clone)]
pub struct Stamina(f32);
