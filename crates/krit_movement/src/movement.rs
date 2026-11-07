use std::time::Duration;

use avian3d::{
    character_controller::move_and_slide::{
        self, DepenetrationConfig, MoveAndSlide, MoveAndSlideConfig, MoveAndSlideHitResponse,
    },
    collision::collider::Collider,
    parry::math::Rot3,
    physics_transform::{Position, Rotation},
    spatial_query::{ShapeCastConfig, SpatialQuery, SpatialQueryFilter},
};
use bevy_app::{App, FixedPostUpdate, Plugin};
use bevy_ecs::{
    component::Component,
    entity::{Entity, MapEntities},
    query::With,
    schedule::{IntoScheduleConfigs, SystemSet},
    system::{Query, Res},
};

use bevy_math::{Dir3, Vec3, Vec3Swizzles};
use bevy_time::Time;
use bevy_transform::components::Transform;
use krit_kore::{
    intent::{Buttons, CharacterIntent},
    sets::KritSimSystems,
};
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
            (sense_world).in_set(MovementSystems::Sense),
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

/// Senses the World, updates tracked position, and Depenetrates
pub fn sense_world(
    mut character: Query<
        (
            Entity,
            &mut BodyPose,
            &CharacterHull,
            &mut Surroundings,
            &Collider,
            &Position,
            &Rotation,
        ),
        With<CharacterController>,
    >,
    move_and_slide: MoveAndSlide,
) {
    for (entity, mut pose, hull, mut surroundings, collider, position, rotation) in
        character.iter_mut()
    {
        let filter = SpatialQueryFilter::from_excluded_entities([entity]);
        pose.position = position.0.clone();
        pose.rotation = rotation.0.clone();
        let offset = move_and_slide.depenetrate(
            collider,
            pose.position,
            rotation.0,
            &DepenetrationConfig::default(),
            &filter,
        );
        pose.position += offset;
        let hit = move_and_slide.spatial_query.cast_shape(
            collider,
            pose.position,
            rotation.0,
            Dir3::NEG_Y,
            &ShapeCastConfig::from_max_distance(0.0),
            &filter,
        );
        surroundings.stuck = hit.is_some();
        surroundings.ground = move_and_slide
            .spatial_query
            .cast_shape(
                collider,
                pose.position,
                rotation.0,
                Dir3::NEG_Y,
                &ShapeCastConfig::from_max_distance(0.05),
                &filter,
            )
            .map(|hit| GroundHit {
                entity: hit.entity,
                normal: Dir3::new(hit.normal1).unwrap_or(Dir3::Y),
                distance: hit.distance,
            });
    }
}

pub fn handle_movement(
    mut ctx: Query<
        (
            Entity,
            &MovementProfile,
            &MovementStats,
            &CharacterHull,
            &Collider,
            &MoveCommand,
            &Surroundings,
            &mut BodyPose,
            &mut KinematicState,
            &mut MovementState,
        ),
        With<CharacterController>,
    >,
    move_and_slide: MoveAndSlide,
    time: Res<Time>,
) {
    let dt = time.delta();
    for (
        entity,
        profile,
        stats,
        hull,
        collider,
        command,
        surroundings,
        mut pose,
        mut state,
        mut mode,
    ) in ctx.iter_mut()
    {
        if surroundings.stuck {
            continue;
        }
        info!("stepping Movement");
        let report = step(
            entity,
            &move_and_slide,
            &Tuning {
                profile,
                stats,
                hull,
                collider,
            },
            command,
            surroundings,
            &Motion {
                pose: pose.as_mut(),
                kinematic: state.as_mut(),
                mode: mode.as_mut(),
            },
            &dt,
        );
        info!("Updating Movement: {:?}", report.transform.translation);
        pose.position = report.transform.translation.clone();
    }
}

pub fn step(
    entity: Entity,
    spatial: &MoveAndSlide,
    tuning: &Tuning,
    command: &MoveCommand,
    surroundings: &Surroundings,
    motion: &Motion,
    dt: &Duration,
) -> StepReport {
    let wish = command.wish_dir * command.wish_amount;
    let filter = SpatialQueryFilter::from_excluded_entities([entity]);
    let output = spatial.move_and_slide(
        tuning.collider,
        motion.pose.position,
        motion.pose.rotation,
        wish,
        *dt,
        &MoveAndSlideConfig::default(),
        &filter,
        |_| MoveAndSlideHitResponse::Accept,
    );
    StepReport {
        transform: Transform::from_translation(output.position),
    }
}

/// Read-only borrows of the configuration. Not a component; built by the Step system.
pub struct Tuning<'a> {
    pub profile: &'a MovementProfile,
    pub stats: &'a MovementStats,
    pub hull: &'a CharacterHull,
    pub collider: &'a Collider,
}

/// Mutable borrows of exactly the rolled-back state, plus the working position.
/// Not a component; built by the Step system. If a field is here, rollback restores it.
pub struct Motion<'a> {
    pub pose: &'a mut BodyPose,
    pub kinematic: &'a mut KinematicState,
    pub mode: &'a mut MovementState,
}

/// The main component that defines a Character for movement handling
#[derive(Component, Default, Clone)]
#[require(
    MovementProfile,
    MovementStats,
    CharacterHull,
    MoveCommand,
    Surroundings,
    BodyPose,
    KinematicState,
    MovementState
)]
pub struct CharacterController {}

/// Describes the characteristics of the Character in question for physics purposes
#[derive(Component, Default, Clone)]
pub struct CharacterHull {
    pub shape: HullShape,
    pub radius: f32,
    pub standing_height: f32,
    pub eye_below_top: f32,
    pub skin_width: f32,
}

/// Describes the shape of the character
#[derive(Default, Clone)]
pub enum HullShape {
    #[default]
    Cylinder,
    Capsule,
    Box,
}

/// Describes information about how the character should move,
/// Acceleration, stepping, slopes, etc
/// Should be immutable at runtime
#[derive(Component, Default, Clone)]
pub struct MovementProfile {}

/// Stats related to how the character moves
/// Can be adjusted at runtime by equipment, buffs, debuffs
#[derive(Component, Default, Clone)]
pub struct MovementStats {}

/// Represents the current state of the movement
#[derive(Component, Default, Clone)]
pub enum MovementState {
    Ground,
    #[default]
    Air,
}

/// Internal state of the movement engine
#[derive(Component, Clone, Default, PartialEq, MapEntities)]
pub struct KinematicState {
    pub velocity: Vec3,
    #[entities]
    pub ground: Option<Entity>,
}

/// How the character wishes to move this tick, in world space
#[derive(Component, Clone, Default)]
pub struct MoveCommand {
    /// Horizontal unit direction the character wants to move in, in world space. Zero for no input.
    pub wish_dir: Vec3,
    /// How hard it wants to move, 0..=1. A half-tilted stick gives 0.5; keyboards give 0 or 1.
    pub wish_amount: f32,
    /// Full view direction including pitch. Only swim, ladder and noclip use it (Part 4).
    pub look_dir: Vec3,
    /// Movement buttons held this tick, after `ActionGates` and forced/disabled masks.
    pub held: Buttons,
    /// Movement buttons that went down this tick (from core's `ButtonEdges`), after gates.
    pub pressed: Buttons,
    /// A game-supplied velocity that replaces wish acceleration this tick (§11, tier 3).
    pub velocity_override: Option<VelocityOverride>,
}

#[derive(Clone)]
pub struct VelocityOverride {
    velocity: Vec3,
    mode: VelocityOverrideMode,
}

#[derive(Clone)]
pub enum VelocityOverrideMode {
    Replace,
    Add,
}

/// Copy of the position
#[derive(Component, Clone, Default)]
pub struct BodyPose {
    pub position: Vec3,
    pub rotation: Rot3,
}

#[derive(Component, Clone, Default)]
pub struct Surroundings {
    /// Walkable ground under the feet, if any. `None` means airborne.
    pub ground: Option<GroundHit>,
    /// True if depenetration could not free the hull. The step will not move the character.
    pub stuck: bool,
}

#[derive(Clone)]
pub struct GroundHit {
    pub entity: Entity,
    pub normal: Dir3,
    pub distance: f32, // gap between the feet and the ground, ≤ ground_probe_distance
}

#[derive(Component, Clone, Default)]
pub struct StepEvents {
    /// Mode and stance at the start of the tick, recorded by Prepare. Report compares them with
    /// the final values to emit MovementStateChanged / StanceChanged exactly once.
    pub start_mode: MovementState,
    /// Vertical speed at touchdown, if the character landed this tick.
    pub landed: Option<f32>,
    /// Vertical distance moved by a step-up (positive) or ground snap (negative), for camera smoothing.
    pub step_height: f32,
    pub jumped: bool,   // Part 2
    pub footstep: bool, // §9.3
}
