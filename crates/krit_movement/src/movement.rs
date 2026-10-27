use bevy_app::{App, FixedPostUpdate, Plugin};
use bevy_ecs::{
    component::Component,
    schedule::{IntoScheduleConfigs, SystemSet},
    system::Query,
};

use bevy_math::Vec3Swizzles;
use bevy_transform::components::Transform;
use krit_kore::{intent::CharacterIntent, sets::KritSimSystems};
use tracing::info;

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
pub struct StepContext {
    intent: CharacterIntent,
    transform: Transform,
}

#[derive(Default, Clone, Debug)]
pub struct StepReport {
    transform: Transform,
}

pub fn handle_movement(mut ctx: Query<(&CharacterController, &mut Transform, &CharacterIntent)>) {
    for (controller, mut transform, intent) in ctx.iter_mut() {
        info!("stepping Movement");
        let report = step(&StepContext {
            intent: intent.clone(),
            transform: transform.clone(),
        });
        info!("Updating Movement: {:?}", report.transform.translation);
        transform.translation = report.transform.translation.clone();
    }
}

pub fn step(ctx: &StepContext) -> StepReport {
    let mut position = ctx.transform.translation;
    position += ctx.intent.move_axis.extend(0.0).xzy();

    StepReport {
        transform: Transform::from_translation(position),
    }
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
