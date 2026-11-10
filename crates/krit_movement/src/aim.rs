use bevy_app::{App, FixedPostUpdate, Plugin};
use bevy_ecs::schedule::{IntoScheduleConfigs, SystemSet};

use krit_kore::sets::KritSimSystems;

/// Plugin that handles how Aiming applies
pub struct KritAimPlugin;

impl Plugin for KritAimPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            FixedPostUpdate,
            (AimSystems::Ride, AimSystems::Compose, AimSystems::Recoil)
                .chain()
                .in_set(KritSimSystems::Aim),
        );
    }
}

/// SystemSet for Aiming
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum AimSystems {
    /// Updates Character Aim based on ground
    Ride,
    /// Composes Aim
    Compose,
    /// Handles Recoil
    Recoil,
}
