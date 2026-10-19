# 5.12 `krit_kamera`: the first-person view (presentation)

> Part of the [crate architecture plan](../architecture.md) (§5). Section references (§) point to
> that document unless stated otherwise.

**Owns:**
- `ControllerCameraOf` ↔ `ControllerCamera` (already exists);
- a `CameraModeStack` that blends between modes (`FirstPerson`, `ThirdPerson { offset }`,
  `Spectate { target }`, `DeathCam`, `Free`, `Attached`) by weight, as Lyra's camera stack does;
- `FovController` (base FOV, ADS FOV × `AdsState`, sprint FOV);
- additive effect layers:
  - `HeadBob`, with an accessibility toggle;
  - `LandingDip`;
  - `StepSmoothing`, from `KinematicState.last_step_height` (Q3 `CG_StepOffset`);
  - `CameraShake { trauma }`;
  - `ViewKick`, the *visual* recoil spring, separate from `AimRecoil`;
  - `LeanRoll`;
  - crouch eye-height smoothing;
- `CameraSettings`, which includes motion-reduction options.

**Displayed aim:** the camera shows `ViewAngles + AimRecoil + (LocalAim − intent.aim)`, that is,
the simulated aim plus the mouse movement that has not reached a tick yet (the pending term, which
also absorbs quantization rounding). Visual-only layers (`ViewKick`, shake, lean) are added on top
in `CameraEffects`. Mouse look therefore updates every frame, while shots still use the tick's
`ViewAngles`. Details:
- For the locally controlled Character, `ViewAngles` and `AimRecoil` are read from the latest tick,
  **not interpolated**. The pending term is measured from that tick's `intent.aim`, so interpolating
  the base would make the view lag behind the mouse.
- The pending difference uses the shortest signed angle, because `LocalAim.yaw` wraps.
- On a remote, bot or headless view there is no `LocalAim`. The pending term is zero, and
  `ViewAngles` may be interpolated like the `Transform`.
- The camera never writes back into `LocalAim`, `ViewAngles` or `AimRecoil` (§6.2).

**Consumes:** `ViewAngles`, `AimRecoil`, `LocalAim`, `CharacterIntent.aim`, `EyeHeight` and the character `Transform`
(interpolated); `MovementState`, `KinematicState`, `Landed`, `WeaponFired`, `AdsState`,
`LastDamageTaken` and `LifeState`.

**Produces:** only the camera `Transform` and `Projection` FOV. Nothing in simulation reads them.

**Customization:** each effect is its own plugin, and camera modes are pluggable.
