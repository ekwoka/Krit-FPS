use bevy_app::{App, FixedPostUpdate, Plugin};
use bevy_ecs::{
    component::Component,
    schedule::{IntoScheduleConfigs, SystemSet},
};

use krit_kore::sets::KritSimSystems;

/// Plugin that handles the Movement Cycle
pub struct KritMovementPlugin;

impl Plugin for KritMovementPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            FixedPostUpdate,
            (
                MovementSystems::Prepare,
                MovementSystems::Sense,
                MovementSystems::Transition,
                MovementSystems::Step,
                MovementSystems::Apply,
                MovementSystems::Report,
            )
                .chain()
                .in_set(KritSimSystems::Movement),
        );
        app.add_systems(
            FixedPostUpdate,
            (handle_movement).in_set(MovementSystems::Step),
        );
    }
}

/// SystemSet for Movement
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum MovementSystems {
    Prepare,
    Sense,
    Transition,
    Step,
    Apply,
    Report,
}

#[derive(Default, Clone, Debug)]
pub struct StepContext {}

#[derive(Default, Clone, Debug)]
pub struct StepReport {}

pub fn handle_movement() {}

pub fn step(ctx: &StepContext) -> StepReport {
    StepReport {}
}

#[derive(Component, Default, Clone)]
pub struct CharacterController {}

#[derive(Component, Default, Clone)]
pub struct MovementProfile {}

#[derive(Component, Default, Clone)]
pub enum MovementState {
    #[default]
    Ground,
    Air,
    Crouch,
    NoClip,
}
