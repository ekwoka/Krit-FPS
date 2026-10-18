#![warn(missing_docs)]

//! Provides Core Structs and System Sets to allow other crates to function with minimal interdependence

use bevy_app::{App, FixedPostUpdate, Plugin};

/// Structs representing Aim related data
pub mod aim;

/// Structs representing Character Intents
pub mod intent;

/// Contains System Sets
pub mod sets;

use bevy_ecs::schedule::IntoScheduleConfigs;
use sets::{KritSimPhase, KritSimSystems};

/// Main Plugin for settings up systems
pub struct KritKore;

impl Plugin for KritKore {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            FixedPostUpdate,
            (
                KritSimSystems::Intent,
                KritSimSystems::Spawn,
                KritSimSystems::Aim,
                KritSimSystems::Movers,
                KritSimSystems::Movement,
            )
                .chain()
                .in_set(KritSimPhase::PrePhysics),
        );
        app.configure_sets(
            FixedPostUpdate,
            (
                KritSimSystems::Equipment,
                KritSimSystems::Weapons,
                KritSimSystems::Ballistics,
                KritSimSystems::Damage,
                KritSimSystems::Interaction,
                KritSimSystems::Rules,
                KritSimSystems::Despawn,
            )
                .chain()
                .in_set(KritSimPhase::PostPhysics),
        );
    }
}
