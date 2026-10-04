#![warn(missing_docs)]

//! Handles all common logic for Inputs in an FPS game

use bevy_app::{App, FixedPostUpdate, Plugin};
use bevy_ecs::{
    component::Component, entity::MapEntities, observer::On, reflect::ReflectComponent,
    system::Query,
};
use bevy_enhanced_input::{action::events::Fire, prelude::InputAction};
use bevy_math::{Vec2, Vec3};
use bevy_reflect::Reflect;

/// Enables KritInput core logic
/// # Adding
/// ```
/// use bevy::prelude::*;
/// use krit_input::KritInput;
///
/// let mut app = App::new();
/// app.add_plugins(KritInput);
/// ```
pub struct KritInput;

impl Plugin for KritInput {
    fn build(&self, app: &mut App) {
        app.add_plugins(KritInputAccumulation);
    }
}

/// Plugin for Accumulating Inputs
/// /// # Adding
/// ```
/// use bevy::prelude::*;
/// use krit_input::KritInputAccumulation;
///
/// let mut app = App::new();
/// app.add_plugins(KritInputAccumulation);
/// ```
pub struct KritInputAccumulation;

impl Plugin for KritInputAccumulation {
    fn build(&self, app: &mut App) {
        app.add_observer(apply_movement)
            .add_observer(apply_jump)
            .add_observer(apply_crouch)
            .add_systems(FixedPostUpdate, clear_accumulated_input);
    }
}

/// Triggers input for moving the character
/// Vec2 action output is in relation to the characters frame of reference on the X and Z planes
#[derive(Debug, InputAction)]
#[action_output(Vec2)]
pub struct Movement;

/// Triggers movements for the character in global space.
/// Internally is normalized to the characters reference space.
#[derive(Debug, InputAction)]
#[action_output(Vec3)]
pub struct GlobalMovement;

/// Triggers the character to Jump
#[derive(Debug, InputAction)]
#[action_output(bool)]
pub struct Jump;

/// Initiates Crouching
#[derive(Debug, InputAction)]
#[action_output(bool)]
pub struct Crouch;

/// Handles rotating the camera (and thus the character)
#[derive(Debug, InputAction)]
#[action_output(Vec2)]
pub struct RotateCamera;

/// Input accumulated since the last fixed update loop. Is cleared after every fixed update loop.
#[derive(Component, Clone, Reflect, PartialEq, Default, Debug, MapEntities)]
#[reflect(Component)]
pub struct AccumulatedInput {
    /// The last non-zero move that was input since the last fixed update loop
    pub last_movement: Option<Vec2>,
    /// Whether any frame since the last fixed update loop input a Jump
    pub jumped: bool,
    /// Whether any frame since the last fixed update loop input a Crouch
    pub crouched: bool,
}

/// Accumulates Movement inputs before controller processes them.
/// The last movement event is used.
pub fn apply_movement(
    movement: On<Fire<Movement>>,
    mut accumulated_inputs: Query<&mut AccumulatedInput>,
) {
    if let Ok(mut accumulated_inputs) = accumulated_inputs.get_mut(movement.context) {
        accumulated_inputs.last_movement = Some(movement.value);
    }
}

/// Accumulates Jump inputs
pub fn apply_jump(jump: On<Fire<Jump>>, mut accumulated_inputs: Query<&mut AccumulatedInput>) {
    if let Ok(mut accumulated_inputs) = accumulated_inputs.get_mut(jump.context) {
        accumulated_inputs.jumped = true;
    }
}

/// Accumulates Crouch inputs
pub fn apply_crouch(
    crouch: On<Fire<Crouch>>,
    mut accumulated_inputs: Query<&mut AccumulatedInput>,
) {
    if let Ok(mut accumulated_inputs) = accumulated_inputs.get_mut(crouch.context) {
        accumulated_inputs.crouched = true;
    }
}

/// Clears Accumulated Inputs after they are processed
pub fn clear_accumulated_input(mut accumulated_inputs: Query<&mut AccumulatedInput>) {
    for mut accumulated_input in &mut accumulated_inputs {
        *accumulated_input = AccumulatedInput::default();
    }
}
