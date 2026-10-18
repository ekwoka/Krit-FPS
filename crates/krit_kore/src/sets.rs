use bevy_ecs::schedule::SystemSet;

/// Core Simulation sets to bookend the physics simulation
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum KritSimPhase {
    /// Runs Before Physics
    /// Intent -> Movement
    PrePhysics,
    /// After Physics
    /// Cleans intermediary values
    PostPhysics,
}

/// Various Sets within the Simulation
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum KritSimSystems {
    // ---- PrePhysics ----
    /// Copy/validate intents for this tick; derive button edges from `PrevButtons`;
    /// apply forced/disabled buttons; reset and apply gates (freeze time, dead, menus)
    Intent,
    /// Spawn requests from last tick's Rules are realised here, at the start of the tick
    Spawn,
    /// ViewAngles from the intent's absolute aim + `ViewAngleOffset`; recoil recovery
    Aim,
    /// Moving platforms, doors and other kinematic movers (before characters ride them)
    Movers,
    /// Character movement (KCC)
    Movement,

    // ---- PostPhysics ----
    /// Equipment changes, intent routing to the active item
    Equipment,
    /// Weapon state machines -> ShotRequest
    Weapons,
    /// ShotRequest + projectiles -> HitResult -> DamageRequest
    Ballistics,
    /// DamageRequest -> DamageApplied / Died
    Damage,
    /// Interaction progress
    Interaction,
    /// Match rules: scoring, phase transitions, spawn requests
    Rules,
    /// Despawning of characters and items, cleanup
    Despawn,
}

/// Presentation (Update / PostUpdate)
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum KritViewSystems {
    /// Syncs any data after the simulation
    SyncFromSim,
    /// Calculates Camera Effects (shake, etc)
    CameraEffects,
    /// Handles the Players View Model
    ViewModel,
    /// Provides Feedback to the player
    Feedback,
}
