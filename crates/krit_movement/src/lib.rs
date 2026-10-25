#![warn(missing_docs)]

//! Provides the CharacterController and other movement related logic

use bevy_app::{App, Plugin};
use bevy_ecs::component::Component;
use bevy_math::{Vec2, Vec3};

use crate::{aim::KritAimPlugin, movement::KritMovementPlugin};

pub mod aim;
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

#[derive(Component, Default, Clone)]
pub struct KinematicState {
    pub velocity: Vec3,
    pub grounded: bool,
    pub ground_normal: Vec2,
    pub last_step_height: f32,
}

#[derive(Component, Default, Clone)]
pub struct Stamina(f32);
