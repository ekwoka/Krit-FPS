use std::time::Duration;

use avian3d::{
    character_controller::move_and_slide::{
        DepenetrationConfig, MoveAndSlide, MoveAndSlideConfig, MoveAndSlideHitResponse,
    },
    collision::collider::Collider,
    parry::math::Rot3,
    physics_transform::{Position, Rotation},
    spatial_query::{ShapeCastConfig, SpatialQueryFilter},
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
            (prepare_movement).in_set(MovementSystems::Prepare),
        );
        app.add_systems(
            FixedPostUpdate,
            (sense_world).in_set(MovementSystems::Sense),
        );
        app.add_systems(
            FixedPostUpdate,
            (handle_movement).in_set(MovementSystems::Step),
        );
        app.add_systems(
            FixedPostUpdate,
            (apply_movement).in_set(MovementSystems::Apply),
        );
    }
}

/// SystemSet for Movement
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum MovementSystems {
    /// Moves data from Kore components to Movement Components
    Prepare,
    /// Senses information about the controller surroundings
    Sense,
    /// Handles Transitions (grounding, jumping, etc)
    Transition,
    /// Steps simulation
    Step,
    /// Applies Movement to the Character
    Apply,
    /// Reports information about the Motion
    Report,
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
    for (entity, mut pose, _hull, mut surroundings, collider, position, rotation) in
        character.iter_mut()
    {
        let filter = SpatialQueryFilter::from_excluded_entities([entity]);
        pose.position = position.0;
        pose.rotation = rotation.0;
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

/// Prepares Character Intent for Motion
pub fn prepare_movement(
    mut characters: Query<(&mut MoveCommand, &CharacterIntent), With<CharacterController>>,
) {
    for (mut movement, intent) in characters.iter_mut() {
        movement.wish_dir = intent
            .move_axis
            .extend(0.0)
            .xzy()
            .normalize_or_zero()
            .rotate_y(intent.aim.yaw_rads());
        movement.wish_amount = intent.move_axis.length().min(1.0);
        movement.held = intent.buttons;
    }
}

/// Handles Stepping motion during normal updates
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
            &mut StepEvents,
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
        mut events,
    ) in ctx.iter_mut()
    {
        if surroundings.stuck {
            continue;
        }
        info!("stepping Movement");
        let mut motion = Motion {
            pose: pose.as_mut(),
            kinematic: state.as_mut(),
            mode: mode.as_mut(),
        };
        step(
            entity,
            &move_and_slide,
            &Tuning {
                profile,
                stats,
                hull,
                collider,
            },
            command,
            &mut motion,
            events.as_mut(),
            surroundings,
            &dt,
        );
        info!("Updating Movement: {:?}", pose.position);
    }
}

/// Steps Motion Simulation
/// Typically called each FixedUpdate
/// but can be called repeatedly to simulate steps for server rollback
pub fn step(
    entity: Entity,
    spatial: &MoveAndSlide,
    tuning: &Tuning,
    command: &MoveCommand,
    motion: &mut Motion,
    _events: &mut StepEvents,
    _surroundings: &Surroundings,
    dt: &Duration,
) {
    let wish = command.wish_dir * 10.0;
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
    motion.pose.position = output.position;
}

/// Applies Movement to the Character
pub fn apply_movement(
    mut characters: Query<(&mut Position, &BodyPose), With<CharacterController>>,
) {
    for (mut position, pose) in characters.iter_mut() {
        position.0 = pose.position;
    }
}

/// Read-only borrows of the configuration. Not a component; built by the Step system.
pub struct Tuning<'a> {
    /// How to handle Movement
    pub profile: &'a MovementProfile,
    /// Stats impacting Movement
    pub stats: &'a MovementStats,
    /// Character Shape Info
    pub hull: &'a CharacterHull,
    /// Collider
    pub collider: &'a Collider,
}

/// Mutable borrows of exactly the rolled-back state, plus the working position.
/// Not a component; built by the Step system. If a field is here, rollback restores it.
pub struct Motion<'a> {
    /// current Body Post
    pub pose: &'a mut BodyPose,
    /// Current Kinematic State
    pub kinematic: &'a mut KinematicState,
    /// Current Movement State
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
    MovementState,
    StepEvents
)]
pub struct CharacterController {}

/// Describes the characteristics of the Character in question for physics purposes
#[derive(Component, Default, Clone)]
pub struct CharacterHull {
    /// Shape to use for the Movement Collider
    pub shape: HullShape,
    /// Width of the Character Shape
    pub radius: f32,
    /// Full height of the Character
    pub standing_height: f32,
    /// How far the Eyes are below the top of the shape
    pub eye_below_top: f32,
    /// Distance to keep from objects
    pub skin_width: f32,
}

/// Describes the shape of the character
#[derive(Default, Clone)]
pub enum HullShape {
    /// A Cylinder
    #[default]
    Cylinder,
    /// A Capsule
    Capsule,
    /// A Box
    Box,
}

/// Describes information about how the character should move,
/// Acceleration, stepping, slopes, etc
/// Should be immutable at runtime
#[derive(Component, Clone)]
pub struct MovementProfile {
    // Gravity and Air
    /// Acceleration due to Gravity (in m/s^2)
    gravity: f32,
    /// Maximum Air Speed (in m/s)
    air_wishspeed_cap: Option<f32>,
    /// Air Control modifer (multipler)
    air_control: Option<f32>,
    // Ground
    /// Friction on ground surface
    friction: f32,
    stop_speed: f32,
    accel_model: AccelModel,
    decel: Option<f32>,

    // Collision
    step_height: f32,
    step_rule: StepRule,
    step_in_air: bool,
    min_step_ledge_space: Option<f32>,
    max_walk_slop_cos: f32,
    ground_probe_distance: f32,
    ground_snap_distance: f32,
    unground_speed: f32,
    overbounce: f32,
    max_depenetration_iterations: usize,
}

impl Default for MovementProfile {
    fn default() -> Self {
        Self::QUAKE
    }
}

impl MovementProfile {
    const COMMON: Self = Self {
        gravity: 20.0,
        air_wishspeed_cap: None,
        air_control: None,
        accel_model: AccelModel::Arcade,
        friction: 0.0,
        stop_speed: 2.54,
        decel: None,
        step_height: 0.4,
        step_rule: StepRule::FartherHorizontal,
        step_in_air: false,
        min_step_ledge_space: Some(0.1),
        max_walk_slop_cos: 0.7,
        ground_probe_distance: 0.05,
        ground_snap_distance: 0.4,
        unground_speed: 3.56,
        overbounce: 1.0,
        max_depenetration_iterations: 4,
    };
    const QUAKE: Self = Self {
        gravity: 20.32,
        air_wishspeed_cap: Some(0.76),
        accel_model: AccelModel::QuakeProjection,
        friction: 4.0,
        step_height: 0.46,
        step_rule: StepRule::WhenBlocked,
        min_step_ledge_space: None,
        ground_snap_distance: 0.46,
        overbounce: 1.001,
        ..Self::COMMON
    };
    const SOURCE: Self = Self {
        gravity: 20.32,
        air_wishspeed_cap: Some(0.76),
        accel_model: AccelModel::QuakeProjection,
        friction: 4.0,
        step_height: 0.46,
        min_step_ledge_space: None,
        ground_snap_distance: 0.46,
        ..Self::COMMON
    };
    const ARCADE: Self = Self {
        air_control: Some(0.3),
        stop_speed: 0.0,
        decel: Some(50.0),
        step_in_air: true,
        ..Self::COMMON
    };
    const TACTICAL: Self = Self {
        accel_model: AccelModel::Proper,
        friction: 6.0,
        decel: Some(50.0),
        step_in_air: true,
        ..Self::COMMON
    };
    const IMMERSIVE: Self = Self {
        gravity: 18.0,
        air_control: Some(0.1),
        stop_speed: 0.0,
        decel: Some(25.0),
        ..Self::COMMON
    };
}

#[derive(Default, Clone)]
enum AccelModel {
    QuakeProjection,
    #[default]
    Arcade,
    Proper,
}

#[derive(Default, Clone)]
enum StepRule {
    WhenBlocked,
    #[default]
    FartherHorizontal,
}

/// Stats related to how the character moves
/// Can be adjusted at runtime by equipment, buffs, debuffs
#[derive(Component, Default, Clone)]
pub struct MovementStats {}

/// Represents the current state of the movement
#[derive(Component, Default, Clone)]
pub enum MovementState {
    /// Touching the Ground
    Ground,
    /// In the Air
    #[default]
    Air,
}

/// Internal state of the movement engine
#[derive(Component, Clone, Default, PartialEq, MapEntities)]
pub struct KinematicState {
    /// Current Controller Velocity
    pub velocity: Vec3,
    /// Entity the Controller is grounded to
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

/// Overrides on the Characters Velocity and Motion
#[derive(Clone)]
pub struct VelocityOverride {
    /// Velocity to Override By
    pub velocity: Vec3,
    /// How to Override
    pub mode: VelocityOverrideMode,
}

/// How a Velocity Override should be applied
#[derive(Clone)]
pub enum VelocityOverrideMode {
    /// Replaces the Velocity
    Replace,
    /// Adds on top of the existing Velocity
    Add,
}

/// Copy of the Position/Rotation
#[derive(Component, Clone, Default)]
pub struct BodyPose {
    /// Characters Global Position
    pub position: Vec3,
    /// Characters Global Rotation
    pub rotation: Rot3,
}

/// Information about the Character Surroundings
#[derive(Component, Clone, Default)]
pub struct Surroundings {
    /// Walkable ground under the feet, if any. `None` means airborne.
    pub ground: Option<GroundHit>,
    /// True if depenetration could not free the hull. The step will not move the character.
    pub stuck: bool,
}

/// A hit with the ground when probing for ground contact
/// Can be anywhere from 0 -> Probe Distance below character
#[derive(Clone)]
pub struct GroundHit {
    /// Relevant Entity
    pub entity: Entity,
    /// Surface Normal at Hit
    pub normal: Dir3,
    /// Real Distance to the ground
    pub distance: f32,
}

/// Event from the result of stepping the character motion simulation
#[derive(Component, Clone, Default)]
pub struct StepEvents {
    /// Mode and stance at the start of the tick, recorded by Prepare. Report compares them with
    /// the final values to emit MovementStateChanged / StanceChanged exactly once.
    pub start_mode: MovementState,
    /// Vertical speed at touchdown, if the character landed this tick.
    pub landed: Option<f32>,
    /// Vertical distance moved by a step-up (positive) or ground snap (negative), for camera smoothing.
    pub step_height: f32,
    /// If a Jump Started this Step
    pub jumped: bool,
    /// If there was a Footstep this Step
    pub footstep: bool,
}
