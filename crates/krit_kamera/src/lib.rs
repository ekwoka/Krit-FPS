#![warn(missing_docs)]

//! Handles the Camera behaviors from input to rotation matching

use std::f32::consts::TAU;

use bevy_app::{App, Plugin};
use bevy_ecs::{
    component::Component, entity::Entity, lifecycle::HookContext, observer::On, query::With,
    relationship::Relationship, system::Query, template::FromTemplate, world::DeferredWorld,
};
use bevy_enhanced_input::action::events::Fire;
use bevy_math::{EulerRot, Quat};
use bevy_transform::components::Transform;
use krit_input::RotateCamera;

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
        app.add_observer(rotate_camera);
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
    rotation: On<Fire<RotateCamera>>,
    cameras: Query<&ControllerCamera>,
    mut transforms: Query<&mut Transform, With<ControllerCameraOf>>,
) {
    let Ok(mut camera_transform) = cameras
        .get(rotation.context)
        .and_then(|camera| transforms.get_mut(camera.get()))
    else {
        return;
    };

    let (mut yaw, mut pitch, _) = camera_transform.rotation.to_euler(EulerRot::YXZ);
    let delta = -rotation.value;
    yaw += delta.x.to_radians();
    pitch += delta.y.to_radians();
    pitch = pitch.clamp(-TAU / 4.0 + 0.01, TAU / 4.0 - 0.01);

    camera_transform.rotation = Quat::from_euler(EulerRot::YXZ, yaw, pitch, 0.0);
}
