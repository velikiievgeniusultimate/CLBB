//! Safe controller kernel. Qt is a projection/transport/settings shell only.
#![allow(non_snake_case)]
#[path = "../causal.rs"]
pub(crate) mod causal;
#[path = "../causal_bridge.rs"]
mod causal_bridge;
#[allow(unsafe_code)]
mod ffi;
mod frame;
pub mod api;
pub mod host;
mod input;
mod lifecycle;
mod charge_lifecycle;
mod movement;
mod native_aim;
mod selection;
pub mod snapshot;
mod tick;
pub mod view;
#[cfg(test)]
mod rejection_tests;
#[cfg(test)]
mod selection_tests;
use crate::{AimSolver, AngularRadialSolver, Point};
use host::{Host, Timer};
use std::collections::{BTreeMap, BTreeSet};
/// Native skill/config IDs are not hero IDs; owned alternate forms may use
/// seven digits. This bound never replaces typed owner/epoch validation.
pub const MAX_SKILL_ID: i32 = 10_000_000;
#[derive(Clone, Default, Debug)]
pub struct Button {
    pub slot: String,
    pub kind: String,
    pub geometrySource: String,
    pub index: i32,
    pub skillId: i32,
    pub key: i32,
    pub castMode: i32,
    pub aimType: u8,
    pub stopMovement: bool,
    pub available: bool,
    pub press: Point,
    pub center: Point,
    pub radius: Point,
}
#[derive(Clone, Copy, Default, Debug)]
pub struct Elapsed(pub Option<i64>);
impl Elapsed {
    pub fn restart(&mut self, now: i64) {
        self.0 = Some(now)
    }
    pub fn invalidate(&mut self) {
        self.0 = None
    }
    pub fn valid(&self) -> bool {
        self.0.is_some()
    }
    pub fn elapsed(&self, now: i64) -> i64 {
        self.0.map(|t| (now - t) / 1_000_000).unwrap_or(-1)
    }
}
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub enum SkillBehavior {
    #[default]
    Range,
    AwaitVariant,
    Tap,
    Direction,
    Selection,
}
#[derive(Clone, Default, Debug)]
pub struct SkillIntent {
    pub pointer: Point,
    pub world: Point,
    pub direction: Point,
    pub worldValid: bool,
    pub directionValid: bool,
    pub worldResolver: u64,
    pub screenDirection: Point,
    pub screenDirectionValid: bool,
}
#[derive(Clone, Default, Debug)]
pub struct QueuedSkill {
    pub slot: String,
    pub key: i32,
    pub held: bool,
    pub requestedNs: i64,
    pub down: SkillIntent,
    pub up: SkillIntent,
}
#[derive(Clone, Default, Debug)]
pub struct AuxiliaryTap {
    pub id: i32,
    pub point: Point,
    pub until: i64,
    pub cooldownSlot: String,
    pub cooldownVariant: i32,
}
#[derive(Clone, Default, Debug)]
pub struct SkillVariant {
    pub slot: String,
    pub base: i32,
    pub active: i32,
    pub cast_locked: Option<bool>,
    pub state_forbidden: Option<bool>,
    pub charge_skill: Option<bool>,
}
#[derive(Clone, Copy)]
pub(crate) struct KaguraThrowTransition {
    released_ns: i64,
    observed_lock: bool,
}
#[derive(Clone, Default, Debug)]
pub struct DirectRange {
    pub hero: i32,
    pub skill: i32,
    pub slot: String,
    pub source: String,
    pub resolution: Point,
    pub range: f64,
    pub native: bool,
}
#[derive(Clone, Default, Debug)]
pub struct DirectStartup {
    pub epoch: String,
    pub hero: i32,
    pub buttonSkill: i32,
    pub activeSkill: i32,
    pub slot: String,
    pub source: String,
    pub resolution: Point,
    pub press: Point,
    pub center: Point,
    pub radius: Point,
    pub range: f64,
    pub plane: f64,
}
#[derive(Clone, Default)]
pub struct ResponseModel {
    pub heroId: i32,
    pub skillId: i32,
    pub slot: String,
    pub source: String,
    pub resolution: Point,
    pub center: Point,
    pub radius: f64,
    pub relative: bool,
    pub used: u64,
    pub solver: AimSolver,
}
#[derive(Default)]
pub struct Controller {
    pub(crate) causal_: causal::CausalInput,
    pub(crate) traceEnabled_: bool,
    pub(crate) enabled_: bool,
    pub(crate) ready_: bool,
    pub(crate) aimReady_: bool,
    pub(crate) frameCameraReady_: bool,
    pub(crate) provisionalStartup_: bool,
    pub(crate) startupArmed_: bool,
    pub(crate) startupMayActivate_: bool,
    pub(crate) cooldownSeen_: bool,
    pub(crate) skillCancelPending_: bool,
    pub(crate) skillCancelAcknowledged_: bool,
    pub(crate) qDragArming_: bool,
    pub(crate) qDragRecompute_: bool,
    pub(crate) qDragRecomputeUsed_: bool,
    pub(crate) directionGeometryFresh_: bool,
    pub(crate) pendingDirectionValid_: bool,
    pub(crate) directionAccepted_: bool,
    pub(crate) directGeometryFresh_: bool,
    pub(crate) logicalMode_: bool,
    pub(crate) logicalSampleFresh_: bool,
    pub(crate) logicalInputFresh_: bool,
    pub(crate) trajectoryValid_: bool,
    pub(crate) acceptedInput_: bool,
    pub(crate) acceptedAdapt_: bool,
    pub(crate) candidateValid_: bool,
    pub(crate) acceptedHeroValid_: bool,
    pub(crate) verifiedTransport_: bool,
    pub(crate) warmAttempted_: bool,
    pub(crate) skillEchoPending_: bool,
    pub(crate) traceDesiredValid_: bool,
    pub(crate) joystickReady_: bool,
    pub(crate) joystickMeasured_: bool,
    pub(crate) cancelReady_: bool,
    pub(crate) skillNudged_: bool,
    pub(crate) skillKeyHeld_: bool,
    pub(crate) skillExpectedAim_: bool,
    pub(crate) skillActivationObserved_: bool,
    pub(crate) skillStateForbiddenAtDown_: bool,
    pub(crate) chargeGesture_: Option<charge_lifecycle::ChargeGesture>,
    pub(crate) pendingRelease_: bool,
    pub(crate) pendingWorldValid_: bool,
    pub(crate) movementReady_: bool,
    pub(crate) movementHeld_: bool,
    pub(crate) movementAuto_: bool,
    pub(crate) movementPressed_: bool,
    pub(crate) movementEchoPending_: bool,
    pub(crate) navigationAvailable_: bool,
    pub(crate) changed_: bool,
    pub(crate) heroId_: i32,
    pub(crate) cancelKey_: i32,
    pub(crate) aimSkillId_: i32,
    pub(crate) skillTouch_: i32,
    pub(crate) skillKey_: i32,
    pub(crate) skillCastMode_: i32,
    pub(crate) observedSkillId_: i32,
    pub(crate) skillBindingVariant_: i32,
    pub(crate) skillCooldownVariant_: i32,
    pub(crate) cooldownTraceKey_: i32,
    pub(crate) cooldownTraceVariant_: i32,
    pub(crate) qDragArmSkill_: i32,
    pub(crate) provisionalSkill_: i32,
    pub(crate) coreSkill_: i32,
    pub(crate) pendingGoalSkill_: i32,
    pub(crate) candidateSkillId_: i32,
    pub(crate) movementTouch_: i32,
    pub(crate) manualTouch_: i32,
    pub(crate) movementClickMode_: i32,
    pub(crate) movementClickPercent_: Option<i32>,
    pub(crate) movementHoldMs_: i32,
    pub(crate) movementWaypoint_: i32,
    pub(crate) now_: i64,
    pub(crate) lastFrameNs_: i64,
    pub(crate) castStartedNs_: i64,
    pub(crate) lastSkillEndNs_: i64,
    pub(crate) frameCoreSampleStartNs_: i64,
    pub(crate) skillCancelSentNs_: i64,
    pub(crate) qDragMismatchSinceNs_: i64,
    pub(crate) qDragArmSentNs_: i64,
    pub(crate) directionGoalTime_: i64,
    pub(crate) directionSampleStart_: i64,
    pub(crate) directionSampleEnd_: i64,
    pub(crate) directionLastEchoNs_: i64,
    pub(crate) skillVariantSampleNs_: i64,
    pub(crate) skillVariantSampleStartNs_: i64,
    pub(crate) pendingGoalTime_: i64,
    pub(crate) logicalSampleNs_: i64,
    pub(crate) lastInputPacketNs_: i64,
    pub(crate) acceptedSampleNs_: i64,
    pub(crate) consumedSampleNs_: i64,
    pub(crate) candidateSinceNs_: i64,
    pub(crate) traceSampleStart_: i64,
    pub(crate) traceSampleEnd_: i64,
    pub(crate) directionGoalSequence_: u64,
    pub(crate) directionMatchedSequence_: u64,
    pub(crate) pendingGoalSequence_: u64,
    pub(crate) touchSequence_: u64,
    pub(crate) verifiedTransportSequence_: u64,
    pub(crate) modelClock_: u64,
    pub(crate) traceMatchedSequence_: u64,
    pub(crate) pointerUpdates_: u64,
    pub(crate) pointerFlushes_: u64,
    pub(crate) pendingWorldResolver_: u64,
    pub(crate) directionGoalRadius_: f64,
    pub(crate) pendingGoalRadius_: f64,
    pub(crate) pendingGoalRange_: f64,
    pub(crate) solverRadius_: f64,
    pub(crate) armingDistance_: f64,
    pub(crate) status_: String,
    pub(crate) activeSlot_: String,
    pub(crate) frameActiveSlot_: String,
    pub(crate) activeConfigSlot_: String,
    pub(crate) inputReason_: String,
    pub(crate) cooldownTraceSlot_: String,
    pub(crate) qDragArmSource_: String,
    pub(crate) directRangeSource_: String,
    pub(crate) frameEpoch_: String,
    pub(crate) pendingGoalSource_: String,
    pub(crate) logicalSource_: String,
    pub(crate) navigationStatus_: String,
    pub(crate) movementStatus_: String,
    pub(crate) androidSize_: Point,
    pub(crate) pointer_: Point,
    pub(crate) heroScreen_: Point,
    pub(crate) aimWorld_: Point,
    pub(crate) frameHero_: Point,
    pub(crate) joystickCenter_: Point,
    pub(crate) joystickRadius_: Point,
    pub(crate) cancelPoint_: Point,
    pub(crate) skillPoint_: Point,
    pub(crate) solverCenter_: Point,
    pub(crate) skillDownPoint_: Point,
    pub(crate) qDragArmPoint_: Point,
    pub(crate) qDragMismatchCell_: Point,
    pub(crate) qDragMismatchOffset_: Point,
    pub(crate) directionCenter_: Point,
    pub(crate) directionRadius_: Point,
    pub(crate) pendingDirection_: Point,
    pub(crate) directionAcceptedPoint_: Point,
    pub(crate) directionObserved_: Point,
    pub(crate) directionGoalPoint_: Point,
    pub(crate) directionGoalCenter_: Point,
    pub(crate) pendingGoalCenter_: Point,
    pub(crate) logicalOrigin_: Point,
    pub(crate) logicalObserved_: Point,
    pub(crate) acceptedPoint_: Point,
    pub(crate) candidateRaw_: Point,
    pub(crate) candidateTarget_: Point,
    pub(crate) lastAcceptedHero_: Point,
    pub(crate) verifiedActual_: Point,
    pub(crate) traceDesired_: Point,
    pub(crate) traceRaw_: Point,
    pub(crate) traceDecoded_: Point,
    pub(crate) pendingPointer_: Point,
    pub(crate) pendingWorld_: Point,
    pub(crate) movementPress_: Point,
    pub(crate) movementCenter_: Point,
    pub(crate) movementRadius_: Point,
    pub(crate) movementTouchPoint_: Point,
    pub(crate) manualPoint_: Point,
    pub(crate) movementTarget_: Point,
    pub(crate) movementRequestedTarget_: Point,
    pub(crate) movementPreviousHero_: Point,
    pub(crate) movementProgressHero_: Point,
    pub(crate) frameCamera_: [f64; 4],
    pub(crate) frameMovementCamera_: [f64; 4],
    pub(crate) directionGoalCamera_: [f64; 4],
    pub(crate) skillStarted_: Elapsed,
    pub(crate) skillMoved_: Elapsed,
    pub(crate) releaseRequested_: Elapsed,
    pub(crate) movementPressTime_: Elapsed,
    pub(crate) movementRouteTime_: Elapsed,
    pub(crate) movementProgressTime_: Elapsed,
    pub(crate) movementReplanTime_: Elapsed,
    pub(crate) buttons_: Vec<Button>,
    pub(crate) queuedSkills_: Vec<QueuedSkill>,
    pub(crate) kaguraThrow_: Option<KaguraThrowTransition>,
    pub(crate) auxiliaryTaps_: Vec<AuxiliaryTap>,
    pub(crate) skillVariants_: Vec<SkillVariant>,
    pub(crate) directRanges_: Vec<DirectRange>,
    pub(crate) directStartups_: Vec<DirectStartup>,
    pub(crate) responseModels_: Vec<ResponseModel>,
    pub(crate) movementPath_: Vec<Point>,
    pub(crate) auxiliaryHeldKeys_: BTreeSet<i32>,
    pub(crate) cooldownIgnoredKeys_: BTreeSet<i32>,
    pub(crate) cooldownCasts_: BTreeMap<String, i64>,
    pub(crate) cooldownState_: snapshot::CooldownState,
    pub(crate) solver_: AimSolver,
    pub(crate) direct_: AngularRadialSolver,
    pub(crate) skillBehavior_: SkillBehavior,
    pub(crate) selection_: selection::SelectionState,
    pub(crate) selectionInput_: snapshot::SelectionInput,
    pub(crate) selectionHeroScreen_: Point,
    pub(crate) selectionHeroSampleNs_: i64,
}
impl Controller {
    pub(crate) fn sync_clock(&mut self, host: &mut impl Host) {
        if let Some(now) = host.clock_ns() {
            self.now_ = now;
        }
    }
    pub fn new() -> Self {
        Self {
            skillTouch_: -1,
            movementTouch_: -1,
            manualTouch_: -1,
            cancelKey_: 84,
            movementHoldMs_: 120,
            pointer_: Point { x: 0.5, y: 0.5 },
            heroScreen_: Point { x: 0.5, y: 0.5 },
            status_: "F7: автоматическое управление выключено".into(),
            navigationStatus_: "Карта проходимости ещё не получена".into(),
            movementStatus_: "Остановлено".into(),
            ..Self::default()
        }
    }
    pub(crate) fn pixels(&self, p: Point) -> Point {
        Point {
            x: p.x * self.androidSize_.x,
            y: p.y * self.androidSize_.y,
        }
    }
    pub(crate) fn normalized(&self, p: Point) -> Point {
        bounded(Point {
            x: p.x / self.androidSize_.x,
            y: p.y / self.androidSize_.y,
        })
    }
    /// A captured virtual joystick can extend past the screen edge. Android
    /// permits an existing touch to continue there; cursor/UI coordinates stay
    /// on the separately bounded normalization path.
    pub(crate) fn skill_normalized(&self, p: Point) -> Point {
        Point::new(p.x / self.androidSize_.x, p.y / self.androidSize_.y)
    }
    pub(crate) fn radius_pixels(&self, p: Point) -> f64 {
        (p.x * self.androidSize_.x).min(p.y * self.androidSize_.y)
    }
    pub(crate) fn solver_observed(&self) -> Point {
        if self.logicalMode_ {
            self.logicalObserved_
        } else {
            self.aimWorld_
        }
    }
    pub(crate) fn ready(&self) -> bool {
        self.enabled_
            && self.ready_
            && self.lastFrameNs_ > 0
            && self.now_ - self.lastFrameNs_ <= 250_000_000
    }
    pub(crate) fn find_button(&self, slot: &str) -> Option<&Button> {
        self.buttons_.iter().find(|b| b.slot == slot)
    }
    pub(crate) fn set_status(&mut self, status: impl Into<String>) {
        let s = status.into();
        if s != self.status_ {
            self.status_ = s;
            self.changed_ = true
        }
    }
    pub(crate) fn set_movement_status(&mut self, status: impl Into<String>) {
        let s = status.into();
        if s != self.movementStatus_ {
            self.movementStatus_ = s;
            self.changed_ = true
        }
    }
    pub(crate) fn trace(&self, host: &mut impl Host, event: &str, reason: &str) {
        if self.traceEnabled_ {
            host.trace(self, event, reason)
        }
    }
}
pub(crate) fn bounded(p: Point) -> Point {
    Point {
        x: p.x.clamp(0., 1.),
        y: p.y.clamp(0., 1.),
    }
}
pub(crate) fn finite(p: Point) -> bool {
    p.x.is_finite() && p.y.is_finite()
}
pub(crate) fn distance(a: Point, b: Point) -> f64 {
    (a - b).length()
}
