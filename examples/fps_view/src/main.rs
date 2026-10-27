use bevy::{light::CascadeShadowConfigBuilder, prelude::*};
use bevy_enhanced_input::{prelude::*, preset::cardinal::wasd_keys};
use krit_input::{KritInput, Movement, RotateCamera};
use krit_kamera::{ControllerCamera, KritKamera};
use krit_kore::{
    aim::{LocalAim, ViewAngleOffset, ViewAngles},
    intent::CharacterIntent,
};
use krit_movement::{KritMovement, movement::CharacterController};

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins,
            KritInput,
            KritKamera,
            KritMovement,
            EnhancedInputPlugin,
        ))
        .add_input_context::<Player>()
        .add_systems(Startup, spawn_world)
        .run();
}

fn spawn_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    info!("Spawning Test World");
    commands.spawn_scene(bsn! {
        // Fill Light
        DirectionalLight {
            color: Color::srgb(0.65, 0.75, 0.90),
            illuminance: 800.0,
            shadow_maps_enabled: false
        }
        Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_3))
    });
    commands.spawn_scene(bsn! {
        // Sun
        DirectionalLight {
            shadow_maps_enabled: true
        }
        CascadeShadowConfigBuilder {
            num_cascades: 1,
            minimum_distance: 600.0,
            maximum_distance: 1000.0,
            ..default()
        }
        .build()
        // Initial orientation: morning sun low in the east.
        Transform::from_rotation(Quat::from_euler(
            EulerRot::YXZ,
            std::f32::consts::FRAC_PI_4,  // rotated east
            -std::f32::consts::FRAC_PI_6, // low angle
            0.0,
        ))
    });
    let ground = meshes.add(Cuboid::new(500.0, 2.0, 500.0));
    let rubble = materials.add(solid(Color::srgb_u8(90, 85, 80)));
    commands.spawn_scene(bsn! {
        // Ground
        Mesh3d(ground)
        MeshMaterial3d<StandardMaterial>(rubble)
        Transform::from_xyz(0.0, -2.0, 0.0)
    });
    let boxy = meshes.add(Cuboid::new(10.0, 10.0, 10.0));
    let blue = materials.add(solid(Color::srgb_u8(20, 20, 200)));
    commands.spawn_scene(bsn! {
        // Ground
        Mesh3d(boxy)
        MeshMaterial3d<StandardMaterial>(blue)
        Transform::from_xyz(0.0, 0.0, -30.0)
    });
    commands.spawn_scene(bsn! {
        // Character
        #Player
        Player
        LocalAim
        ViewAngles
        CharacterIntent
        CharacterController
        ViewAngleOffset
        @player_context()
        Transform::from_xyz(0.0, 0.0, 0.0)
        ControllerCamera[
            Camera3d
        ]
    });
}

#[derive(Component, Default, Clone)]
struct Player;

fn player_context() -> impl Scene {
    bsn! {
        Actions<Player> [
            Action::<RotateCamera>::new()
            Bindings [
                Binding::mouse_motion()
            ]
            --
            Action::<Movement>::new()
            Bindings [
                {wasd_keys()}
            ]
        ]
    }
}

fn solid(color: Color) -> StandardMaterial {
    StandardMaterial {
        base_color: color,
        ..default()
    }
}
