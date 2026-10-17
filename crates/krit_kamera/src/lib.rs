#![warn(missing_docs)]

//! Handles the Camera behaviors from input to rotation matching

use bevy_app::{App, FixedPostUpdate, Plugin, Update};
use bevy_ecs::{
    component::Component, entity::Entity, lifecycle::HookContext, query::With,
    relationship::Relationship, system::Query, template::FromTemplate, world::DeferredWorld,
};
use bevy_math::{EulerRot, Quat};
use bevy_transform::components::Transform;
use krit_kore::{
    aim::{LocalAim, PITCH_LIMIT, ViewAngleOffset, ViewAngles},
    intent::CharacterIntent,
};

/// Enables KritKamera core logic
/// # Adding
/// ```
/// use bevy::prelude::*;
/// use krit_kamera::KritKamera;
///
/// let mut app = App::new();
/// app.add_plugins(KritKamera);
/// ```
pub struct KritKamera;

impl Plugin for KritKamera {
    fn build(&self, app: &mut App) {
        app.add_systems(FixedPostUpdate, update_view_angles)
            .add_systems(Update, rotate_camera);
    }
}

/// Relationship Marker that identifies the entity as a camera associated with the target controller
#[derive(Component, Clone, Copy, Debug, FromTemplate)]
#[relationship(relationship_target = ControllerCamera)]
#[require(Transform)]
#[component(on_add = Self::on_add)]
pub struct ControllerCameraOf {
    /// The Controller this camera is attached to
    #[relationship]
    pub character_controller: Entity,
}

impl ControllerCameraOf {
    /// Creates a CameraOf the provided entity
    pub fn new(character_controller: Entity) -> Self {
        Self {
            character_controller,
        }
    }

    /// Syncs the relevant transforms of the Camera and Controller on add, to prevent a frame flash
    fn on_add(mut world: DeferredWorld, ctx: HookContext) {
        let Some(camera_target) = world.get::<Self>(ctx.entity).copied() else {
            return;
        };
        let Some(target_transform) = world.get::<Transform>(camera_target.get()).copied() else {
            return;
        };
        let Some(mut camera_transform) = world.get_mut::<Transform>(ctx.entity) else {
            return;
        };
        *camera_transform = target_transform;
    }
}

/// Indentifies the Camera associated with this entities Controller
#[derive(Component, Clone, Copy, Debug, FromTemplate)]
#[relationship_target(relationship = ControllerCameraOf)]
pub struct ControllerCamera(Entity);

impl ControllerCamera {
    /// Gets the entity representing this controllers camera
    pub fn get(&self) -> Entity {
        self.0
    }
}

/// Handles rotating the camera in response to BEI actions firing
pub fn rotate_camera(
    cameras: Query<(&ControllerCamera, &LocalAim, &CharacterIntent, &ViewAngles)>,
    mut transforms: Query<&mut Transform, With<ControllerCameraOf>>,
) {
    for (camera, local, intent, angles) in cameras.iter() {
        if let Ok(mut camera_transform) = transforms.get_mut(camera.get()) {
            let mut pitch = signed_to_radians(angles.pitch)
                + (local.pitch - signed_to_radians(intent.aim.pitch));
            let yaw = to_radians(angles.yaw) + (local.yaw - to_radians(intent.aim.yaw));

            let limit = signed_to_radians(PITCH_LIMIT);
            pitch = pitch.clamp(-limit, limit);

            camera_transform.rotation = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0);
        }
    }
}

/// Updates ViewAngles from CharacterIntent and ViewAngleOffset
pub fn update_view_angles(mut views: Query<(&mut ViewAngles, &CharacterIntent, &ViewAngleOffset)>) {
    for (mut angles, intent, offset) in views.iter_mut() {
        angles.pitch = intent
            .aim
            .pitch
            .saturating_add(offset.pitch)
            .clamp(-PITCH_LIMIT, PITCH_LIMIT);
        angles.yaw = intent.aim.yaw.wrapping_add(offset.yaw);
    }
}

/// One Full 360 Turn
pub const FULL_TURN: f64 = 4_294_967_296.0;

fn to_radians(units: u32) -> f32 {
    (units as f64 / FULL_TURN * std::f64::consts::TAU) as f32
}

fn signed_to_radians(units: i32) -> f32 {
    (units as f64 / FULL_TURN * std::f64::consts::TAU) as f32
}
