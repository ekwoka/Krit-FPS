use bevy::{
    ecs::{lifecycle::HookContext, world::DeferredWorld},
    light::CascadeShadowConfigBuilder,
    prelude::*,
};
use bevy_enhanced_input::prelude::*;
use krit_input::{KritInput, RotateCamera};
use krit_kamera::{ControllerCamera, KritKamera};

fn main() -> () {
    App::new()
        .add_plugins((DefaultPlugins, KritInput, KritKamera, EnhancedInputPlugin))
        .add_input_context::<Player>()
        .add_systems(Startup, spawn_world)
        .run();
    ()
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
        template_value(CascadeShadowConfigBuilder {
            num_cascades: 1,
            minimum_distance: 600.0,
            maximum_distance: 1000.0,
            ..default()
        }
        .build())
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
    commands.spawn_scene(bsn! {
        // Character
        #Player
        Player
        Transform::from_xyz(0.0, 0.0, 0.0)
        ControllerCamera[
            Camera3d
        ]
    });
}

#[derive(Component, Default, Clone)]
#[component(on_add = on_add_player_context)]
struct Player;

fn on_add_player_context(mut world: DeferredWorld, context: HookContext) {
    world
        .commands()
        .entity(context.entity)
        .insert(actions!(Player[
            (
                Action::<RotateCamera>::new(),
                Bindings::spawn(Spawn(Binding::mouse_motion()))
            ),
        ]));
}

fn solid(color: Color) -> StandardMaterial {
    StandardMaterial {
        base_color: color,
        ..default()
    }
}
