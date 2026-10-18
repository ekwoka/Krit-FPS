#![warn(missing_docs)]

//! Provides base integration for Avian3d

use avian3d::schedule::PhysicsSystems;
use bevy_app::{App, FixedPostUpdate, Plugin};
use bevy_ecs::schedule::IntoScheduleConfigs;
use krit_kore::sets::KritSimPhase;

/// Main Plugin for connecting Krit Systems to Avians Systems
pub struct KritAvian;

impl Plugin for KritAvian {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            FixedPostUpdate,
            KritSimPhase::PrePhysics.before(PhysicsSystems::First),
        );
        app.configure_sets(
            FixedPostUpdate,
            KritSimPhase::PostPhysics.after(PhysicsSystems::Writeback),
        );
    }
}
