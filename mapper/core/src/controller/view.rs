use super::{Button, Controller, snapshot::Text};
use crate::{AimSolver, AngularRadialSolver, Point};
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Binding {
    pub key: i32,
    pub cast_mode: i32,
    pub stop_movement: u8,
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct ButtonView {
    pub slot: Text,
    pub kind: Text,
    pub geometry_source: Text,
    pub index: i32,
    pub skill_id: i32,
    pub key: i32,
    pub cast_mode: i32,
    pub stop_movement: u8,
    pub available: u8,
    pub press: Point,
    pub center: Point,
    pub radius: Point,
}
impl From<&Button> for ButtonView {
    fn from(b: &Button) -> Self {
        Self {
            slot: text(&b.slot),
            kind: text(&b.kind),
            geometry_source: text(&b.geometrySource),
            index: b.index,
            skill_id: b.skillId,
            key: b.key,
            cast_mode: b.castMode,
            stop_movement: b.stopMovement as u8,
            available: b.available as u8,
            press: b.press,
            center: b.center,
            radius: b.radius,
        }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct VariantView {
    pub slot: Text,
    pub base: i32,
    pub active: i32,
    pub state_forbidden: super::snapshot::Flag,
    pub charge_skill: super::snapshot::Flag,
}
#[repr(C)]
#[derive(Default)]
pub struct View {
    pub traceEnabled_: u8,
    pub enabled_: u8,
    pub ready_: u8,
    pub aimReady_: u8,
    pub frameCameraReady_: u8,
    pub provisionalStartup_: u8,
    pub startupArmed_: u8,
    pub startupMayActivate_: u8,
    pub cooldownSeen_: u8,
    pub skillCancelPending_: u8,
    pub skillCancelAcknowledged_: u8,
    pub qDragArming_: u8,
    pub qDragRecompute_: u8,
    pub qDragRecomputeUsed_: u8,
    pub directionGeometryFresh_: u8,
    pub pendingDirectionValid_: u8,
    pub directionAccepted_: u8,
    pub directGeometryFresh_: u8,
    pub logicalMode_: u8,
    pub logicalSampleFresh_: u8,
    pub logicalInputFresh_: u8,
    pub trajectoryValid_: u8,
    pub acceptedInput_: u8,
    pub acceptedAdapt_: u8,
    pub candidateValid_: u8,
    pub acceptedHeroValid_: u8,
    pub verifiedTransport_: u8,
    pub warmAttempted_: u8,
    pub skillEchoPending_: u8,
    pub traceDesiredValid_: u8,
    pub joystickReady_: u8,
    pub joystickMeasured_: u8,
    pub cancelReady_: u8,
    pub skillNudged_: u8,
    pub skillKeyHeld_: u8,
    pub skillExpectedAim_: u8,
    pub skillActivationObserved_: u8,
    pub skillStateForbiddenAtDown_: u8,
    // Derived read-only lifecycle evidence; not a request to mutate the proof.
    pub chargeStage_: u8,
    pub chargeBase_: i32,
    pub chargeFuture_: i32,
    pub chargeEndClock_: super::snapshot::Number,
    pub chargeEndSampleNs_: i64,
    pub chargeLastSampleNs_: i64,
    pub pendingRelease_: u8,
    pub pendingWorldValid_: u8,
    pub movementReady_: u8,
    pub movementHeld_: u8,
    pub movementAuto_: u8,
    pub movementPressed_: u8,
    pub movementEchoPending_: u8,
    pub navigationAvailable_: u8,
    pub changed_: u8,
    pub heroId_: i32,
    pub cancelKey_: i32,
    pub aimSkillId_: i32,
    pub skillTouch_: i32,
    pub skillKey_: i32,
    pub skillCastMode_: i32,
    pub observedSkillId_: i32,
    pub skillBindingVariant_: i32,
    pub skillCooldownVariant_: i32,
    pub cooldownTraceKey_: i32,
    pub cooldownTraceVariant_: i32,
    pub qDragArmSkill_: i32,
    pub provisionalSkill_: i32,
    pub coreSkill_: i32,
    pub pendingGoalSkill_: i32,
    pub candidateSkillId_: i32,
    pub movementTouch_: i32,
    pub manualTouch_: i32,
    pub movementClickMode_: i32,
    pub movementHoldMs_: i32,
    pub movementWaypoint_: i32,
    pub now_: i64,
    pub lastFrameNs_: i64,
    pub castStartedNs_: i64,
    pub lastSkillEndNs_: i64,
    pub frameCoreSampleStartNs_: i64,
    pub skillCancelSentNs_: i64,
    pub qDragMismatchSinceNs_: i64,
    pub qDragArmSentNs_: i64,
    pub directionGoalTime_: i64,
    pub directionSampleStart_: i64,
    pub directionSampleEnd_: i64,
    pub directionLastEchoNs_: i64,
    pub skillVariantSampleNs_: i64,
    pub skillVariantSampleStartNs_: i64,
    pub pendingGoalTime_: i64,
    pub logicalSampleNs_: i64,
    pub lastInputPacketNs_: i64,
    pub acceptedSampleNs_: i64,
    pub consumedSampleNs_: i64,
    pub candidateSinceNs_: i64,
    pub traceSampleStart_: i64,
    pub traceSampleEnd_: i64,
    pub directionGoalSequence_: u64,
    pub directionMatchedSequence_: u64,
    pub pendingGoalSequence_: u64,
    pub touchSequence_: u64,
    pub verifiedTransportSequence_: u64,
    pub modelClock_: u64,
    pub traceMatchedSequence_: u64,
    pub pointerUpdates_: u64,
    pub pointerFlushes_: u64,
    pub pendingWorldResolver_: u64,
    pub directionGoalRadius_: f64,
    pub pendingGoalRadius_: f64,
    pub pendingGoalRange_: f64,
    pub solverRadius_: f64,
    pub armingDistance_: f64,
    pub status_: Text,
    pub activeSlot_: Text,
    pub frameActiveSlot_: Text,
    pub activeConfigSlot_: Text,
    pub inputReason_: Text,
    pub cooldownTraceSlot_: Text,
    pub qDragArmSource_: Text,
    pub directRangeSource_: Text,
    pub frameEpoch_: Text,
    pub pendingGoalSource_: Text,
    pub logicalSource_: Text,
    pub navigationStatus_: Text,
    pub movementStatus_: Text,
    pub androidSize_: Point,
    pub pointer_: Point,
    pub heroScreen_: Point,
    pub aimWorld_: Point,
    pub frameHero_: Point,
    pub joystickCenter_: Point,
    pub joystickRadius_: Point,
    pub cancelPoint_: Point,
    pub skillPoint_: Point,
    pub solverCenter_: Point,
    pub skillDownPoint_: Point,
    pub qDragArmPoint_: Point,
    pub qDragMismatchCell_: Point,
    pub qDragMismatchOffset_: Point,
    pub directionCenter_: Point,
    pub directionRadius_: Point,
    pub pendingDirection_: Point,
    pub directionAcceptedPoint_: Point,
    pub directionObserved_: Point,
    pub directionGoalPoint_: Point,
    pub directionGoalCenter_: Point,
    pub pendingGoalCenter_: Point,
    pub logicalOrigin_: Point,
    pub logicalObserved_: Point,
    pub acceptedPoint_: Point,
    pub candidateRaw_: Point,
    pub candidateTarget_: Point,
    pub lastAcceptedHero_: Point,
    pub verifiedActual_: Point,
    pub traceDesired_: Point,
    pub traceRaw_: Point,
    pub traceDecoded_: Point,
    pub pendingPointer_: Point,
    pub pendingWorld_: Point,
    pub movementPress_: Point,
    pub movementCenter_: Point,
    pub movementRadius_: Point,
    pub movementTouchPoint_: Point,
    pub manualPoint_: Point,
    pub movementTarget_: Point,
    pub movementRequestedTarget_: Point,
    pub movementPreviousHero_: Point,
    pub movementProgressHero_: Point,
    pub frameCamera_: [f64; 4],
    pub frameMovementCamera_: [f64; 4],
    pub directionGoalCamera_: [f64; 4],
    pub buttons: [ButtonView; 32],
    pub button_count: u32,
    pub variants: [VariantView; 32],
    pub variant_count: u32,
    pub solver: AimSolver,
    pub direct: AngularRadialSolver,
    pub history_size: u32,
    pub queue_depth: u32,
    pub queue_front_slot: Text,
    pub queue_front_key: i32,
    pub queue_front_held: u8,
    pub movement_path_count: u32,
    pub movement_waypoint: Point,
}
pub(crate) fn text(s: &str) -> Text {
    let mut t = Text::default();
    if s.len() > 256 {
        t.present = 2;
        return t;
    }
    t.present = 1;
    t.size = s.len() as u32;
    t.bytes[..s.len()].copy_from_slice(s.as_bytes());
    t
}
impl View {
    pub fn of(s: &Controller) -> Self {
        let mut buttons = [ButtonView::default(); 32];
        for (i, b) in s.buttons_.iter().take(32).enumerate() {
            buttons[i] = b.into()
        }
        let mut variants = [VariantView::default(); 32];
        for (i, v) in s.skillVariants_.iter().take(32).enumerate() {
            variants[i] = VariantView {
                slot: text(&v.slot),
                base: v.base,
                active: v.active,
                state_forbidden: super::snapshot::Flag {
                    value: v.state_forbidden.map_or(-1, |value| value as i32),
                },
                charge_skill: super::snapshot::Flag {
                    value: v.charge_skill.map_or(-1, |value| value as i32),
                },
            }
        }
        Self {
            traceEnabled_: s.traceEnabled_ as u8,
            enabled_: s.enabled_ as u8,
            ready_: s.ready_ as u8,
            aimReady_: s.aimReady_ as u8,
            frameCameraReady_: s.frameCameraReady_ as u8,
            provisionalStartup_: s.provisionalStartup_ as u8,
            startupArmed_: s.startupArmed_ as u8,
            startupMayActivate_: s.startupMayActivate_ as u8,
            cooldownSeen_: s.cooldownSeen_ as u8,
            skillCancelPending_: s.skillCancelPending_ as u8,
            skillCancelAcknowledged_: s.skillCancelAcknowledged_ as u8,
            qDragArming_: s.qDragArming_ as u8,
            qDragRecompute_: s.qDragRecompute_ as u8,
            qDragRecomputeUsed_: s.qDragRecomputeUsed_ as u8,
            directionGeometryFresh_: s.directionGeometryFresh_ as u8,
            pendingDirectionValid_: s.pendingDirectionValid_ as u8,
            directionAccepted_: s.directionAccepted_ as u8,
            directGeometryFresh_: s.directGeometryFresh_ as u8,
            logicalMode_: s.logicalMode_ as u8,
            logicalSampleFresh_: s.logicalSampleFresh_ as u8,
            logicalInputFresh_: s.logicalInputFresh_ as u8,
            trajectoryValid_: s.trajectoryValid_ as u8,
            acceptedInput_: s.acceptedInput_ as u8,
            acceptedAdapt_: s.acceptedAdapt_ as u8,
            candidateValid_: s.candidateValid_ as u8,
            acceptedHeroValid_: s.acceptedHeroValid_ as u8,
            verifiedTransport_: s.verifiedTransport_ as u8,
            warmAttempted_: s.warmAttempted_ as u8,
            skillEchoPending_: s.skillEchoPending_ as u8,
            traceDesiredValid_: s.traceDesiredValid_ as u8,
            joystickReady_: s.joystickReady_ as u8,
            joystickMeasured_: s.joystickMeasured_ as u8,
            cancelReady_: s.cancelReady_ as u8,
            skillNudged_: s.skillNudged_ as u8,
            skillKeyHeld_: s.skillKeyHeld_ as u8,
            skillExpectedAim_: s.skillExpectedAim_ as u8,
            skillActivationObserved_: s.skillActivationObserved_ as u8,
            skillStateForbiddenAtDown_: s.skillStateForbiddenAtDown_ as u8,
            chargeStage_: s.chargeGesture_.as_ref().map_or(0, |v| v.stage),
            chargeBase_: s.chargeGesture_.as_ref().map_or(0, |v| v.base),
            chargeFuture_: s.chargeGesture_.as_ref().map_or(0, |v| v.future),
            chargeEndClock_: s.chargeGesture_.as_ref().and_then(|v| v.end_clock)
                .map_or_else(super::snapshot::Number::default, |v| super::snapshot::Number { kind: 1, value: v as f64 }),
            chargeEndSampleNs_: s.chargeGesture_.as_ref().map_or(0, |v| v.end_sample_ns),
            chargeLastSampleNs_: s.chargeGesture_.as_ref().map_or(0, |v| v.last_sample_ns),
            pendingRelease_: s.pendingRelease_ as u8,
            pendingWorldValid_: s.pendingWorldValid_ as u8,
            movementReady_: s.movementReady_ as u8,
            movementHeld_: s.movementHeld_ as u8,
            movementAuto_: s.movementAuto_ as u8,
            movementPressed_: s.movementPressed_ as u8,
            movementEchoPending_: s.movementEchoPending_ as u8,
            navigationAvailable_: s.navigationAvailable_ as u8,
            changed_: s.changed_ as u8,
            heroId_: s.heroId_,
            cancelKey_: s.cancelKey_,
            aimSkillId_: s.aimSkillId_,
            skillTouch_: s.skillTouch_,
            skillKey_: s.skillKey_,
            skillCastMode_: s.skillCastMode_,
            observedSkillId_: s.observedSkillId_,
            skillBindingVariant_: s.skillBindingVariant_,
            skillCooldownVariant_: s.skillCooldownVariant_,
            cooldownTraceKey_: s.cooldownTraceKey_,
            cooldownTraceVariant_: s.cooldownTraceVariant_,
            qDragArmSkill_: s.qDragArmSkill_,
            provisionalSkill_: s.provisionalSkill_,
            coreSkill_: s.coreSkill_,
            pendingGoalSkill_: s.pendingGoalSkill_,
            candidateSkillId_: s.candidateSkillId_,
            movementTouch_: s.movementTouch_,
            manualTouch_: s.manualTouch_,
            movementClickMode_: s.movementClickMode_,
            movementHoldMs_: s.movementHoldMs_,
            movementWaypoint_: s.movementWaypoint_,
            now_: s.now_,
            lastFrameNs_: s.lastFrameNs_,
            castStartedNs_: s.castStartedNs_,
            lastSkillEndNs_: s.lastSkillEndNs_,
            frameCoreSampleStartNs_: s.frameCoreSampleStartNs_,
            skillCancelSentNs_: s.skillCancelSentNs_,
            qDragMismatchSinceNs_: s.qDragMismatchSinceNs_,
            qDragArmSentNs_: s.qDragArmSentNs_,
            directionGoalTime_: s.directionGoalTime_,
            directionSampleStart_: s.directionSampleStart_,
            directionSampleEnd_: s.directionSampleEnd_,
            directionLastEchoNs_: s.directionLastEchoNs_,
            skillVariantSampleNs_: s.skillVariantSampleNs_,
            skillVariantSampleStartNs_: s.skillVariantSampleStartNs_,
            pendingGoalTime_: s.pendingGoalTime_,
            logicalSampleNs_: s.logicalSampleNs_,
            lastInputPacketNs_: s.lastInputPacketNs_,
            acceptedSampleNs_: s.acceptedSampleNs_,
            consumedSampleNs_: s.consumedSampleNs_,
            candidateSinceNs_: s.candidateSinceNs_,
            traceSampleStart_: s.traceSampleStart_,
            traceSampleEnd_: s.traceSampleEnd_,
            directionGoalSequence_: s.directionGoalSequence_,
            directionMatchedSequence_: s.directionMatchedSequence_,
            pendingGoalSequence_: s.pendingGoalSequence_,
            touchSequence_: s.touchSequence_,
            verifiedTransportSequence_: s.verifiedTransportSequence_,
            modelClock_: s.modelClock_,
            traceMatchedSequence_: s.traceMatchedSequence_,
            pointerUpdates_: s.pointerUpdates_,
            pointerFlushes_: s.pointerFlushes_,
            pendingWorldResolver_: s.pendingWorldResolver_,
            directionGoalRadius_: s.directionGoalRadius_,
            pendingGoalRadius_: s.pendingGoalRadius_,
            pendingGoalRange_: s.pendingGoalRange_,
            solverRadius_: s.solverRadius_,
            armingDistance_: s.armingDistance_,
            status_: text(&s.status_),
            activeSlot_: text(&s.activeSlot_),
            frameActiveSlot_: text(&s.frameActiveSlot_),
            activeConfigSlot_: text(&s.activeConfigSlot_),
            inputReason_: text(&s.inputReason_),
            cooldownTraceSlot_: text(&s.cooldownTraceSlot_),
            qDragArmSource_: text(&s.qDragArmSource_),
            directRangeSource_: text(&s.directRangeSource_),
            frameEpoch_: text(&s.frameEpoch_),
            pendingGoalSource_: text(&s.pendingGoalSource_),
            logicalSource_: text(&s.logicalSource_),
            navigationStatus_: text(&s.navigationStatus_),
            movementStatus_: text(&s.movementStatus_),
            androidSize_: s.androidSize_,
            pointer_: s.pointer_,
            heroScreen_: s.heroScreen_,
            aimWorld_: s.aimWorld_,
            frameHero_: s.frameHero_,
            joystickCenter_: s.joystickCenter_,
            joystickRadius_: s.joystickRadius_,
            cancelPoint_: s.cancelPoint_,
            skillPoint_: s.skillPoint_,
            solverCenter_: s.solverCenter_,
            skillDownPoint_: s.skillDownPoint_,
            qDragArmPoint_: s.qDragArmPoint_,
            qDragMismatchCell_: s.qDragMismatchCell_,
            qDragMismatchOffset_: s.qDragMismatchOffset_,
            directionCenter_: s.directionCenter_,
            directionRadius_: s.directionRadius_,
            pendingDirection_: s.pendingDirection_,
            directionAcceptedPoint_: s.directionAcceptedPoint_,
            directionObserved_: s.directionObserved_,
            directionGoalPoint_: s.directionGoalPoint_,
            directionGoalCenter_: s.directionGoalCenter_,
            pendingGoalCenter_: s.pendingGoalCenter_,
            logicalOrigin_: s.logicalOrigin_,
            logicalObserved_: s.logicalObserved_,
            acceptedPoint_: s.acceptedPoint_,
            candidateRaw_: s.candidateRaw_,
            candidateTarget_: s.candidateTarget_,
            lastAcceptedHero_: s.lastAcceptedHero_,
            verifiedActual_: s.verifiedActual_,
            traceDesired_: s.traceDesired_,
            traceRaw_: s.traceRaw_,
            traceDecoded_: s.traceDecoded_,
            pendingPointer_: s.pendingPointer_,
            pendingWorld_: s.pendingWorld_,
            movementPress_: s.movementPress_,
            movementCenter_: s.movementCenter_,
            movementRadius_: s.movementRadius_,
            movementTouchPoint_: s.movementTouchPoint_,
            manualPoint_: s.manualPoint_,
            movementTarget_: s.movementTarget_,
            movementRequestedTarget_: s.movementRequestedTarget_,
            movementPreviousHero_: s.movementPreviousHero_,
            movementProgressHero_: s.movementProgressHero_,
            frameCamera_: s.frameCamera_,
            frameMovementCamera_: s.frameMovementCamera_,
            directionGoalCamera_: s.directionGoalCamera_,
            buttons,
            button_count: s.buttons_.len() as u32,
            variants,
            variant_count: s.skillVariants_.len() as u32,
            solver: s.solver_,
            direct: s.direct_,
            history_size: s.causal_.history_len() as u32,
            queue_depth: s.queuedSkills_.len() as u32,
            queue_front_slot: text(
                s.queuedSkills_
                    .first()
                    .map(|q| q.slot.as_str())
                    .unwrap_or(""),
            ),
            queue_front_key: s.queuedSkills_.first().map(|q| q.key).unwrap_or(0),
            queue_front_held: s.queuedSkills_.first().is_some_and(|q| q.held) as u8,
            movement_path_count: s.movementPath_.len() as u32,
            movement_waypoint: s
                .movementPath_
                .get(s.movementWaypoint_ as usize)
                .copied()
                .unwrap_or_default(),
        }
    }
}

impl View {
    pub fn refresh_scalars(&mut self, s: &Controller) {
        self.traceEnabled_ = s.traceEnabled_ as u8;
        self.enabled_ = s.enabled_ as u8;
        self.ready_ = s.ready_ as u8;
        self.aimReady_ = s.aimReady_ as u8;
        self.frameCameraReady_ = s.frameCameraReady_ as u8;
        self.provisionalStartup_ = s.provisionalStartup_ as u8;
        self.startupArmed_ = s.startupArmed_ as u8;
        self.startupMayActivate_ = s.startupMayActivate_ as u8;
        self.cooldownSeen_ = s.cooldownSeen_ as u8;
        self.skillCancelPending_ = s.skillCancelPending_ as u8;
        self.skillCancelAcknowledged_ = s.skillCancelAcknowledged_ as u8;
        self.qDragArming_ = s.qDragArming_ as u8;
        self.qDragRecompute_ = s.qDragRecompute_ as u8;
        self.qDragRecomputeUsed_ = s.qDragRecomputeUsed_ as u8;
        self.directionGeometryFresh_ = s.directionGeometryFresh_ as u8;
        self.pendingDirectionValid_ = s.pendingDirectionValid_ as u8;
        self.directionAccepted_ = s.directionAccepted_ as u8;
        self.directGeometryFresh_ = s.directGeometryFresh_ as u8;
        self.logicalMode_ = s.logicalMode_ as u8;
        self.logicalSampleFresh_ = s.logicalSampleFresh_ as u8;
        self.logicalInputFresh_ = s.logicalInputFresh_ as u8;
        self.trajectoryValid_ = s.trajectoryValid_ as u8;
        self.acceptedInput_ = s.acceptedInput_ as u8;
        self.acceptedAdapt_ = s.acceptedAdapt_ as u8;
        self.candidateValid_ = s.candidateValid_ as u8;
        self.acceptedHeroValid_ = s.acceptedHeroValid_ as u8;
        self.verifiedTransport_ = s.verifiedTransport_ as u8;
        self.warmAttempted_ = s.warmAttempted_ as u8;
        self.skillEchoPending_ = s.skillEchoPending_ as u8;
        self.traceDesiredValid_ = s.traceDesiredValid_ as u8;
        self.joystickReady_ = s.joystickReady_ as u8;
        self.joystickMeasured_ = s.joystickMeasured_ as u8;
        self.cancelReady_ = s.cancelReady_ as u8;
        self.skillNudged_ = s.skillNudged_ as u8;
        self.skillKeyHeld_ = s.skillKeyHeld_ as u8;
        self.skillExpectedAim_ = s.skillExpectedAim_ as u8;
        self.skillActivationObserved_ = s.skillActivationObserved_ as u8;
        self.skillStateForbiddenAtDown_ = s.skillStateForbiddenAtDown_ as u8;
        self.pendingRelease_ = s.pendingRelease_ as u8;
        self.pendingWorldValid_ = s.pendingWorldValid_ as u8;
        self.movementReady_ = s.movementReady_ as u8;
        self.movementHeld_ = s.movementHeld_ as u8;
        self.movementAuto_ = s.movementAuto_ as u8;
        self.movementPressed_ = s.movementPressed_ as u8;
        self.movementEchoPending_ = s.movementEchoPending_ as u8;
        self.navigationAvailable_ = s.navigationAvailable_ as u8;
        self.changed_ = s.changed_ as u8;
        self.heroId_ = s.heroId_;
        self.cancelKey_ = s.cancelKey_;
        self.aimSkillId_ = s.aimSkillId_;
        self.skillTouch_ = s.skillTouch_;
        self.skillKey_ = s.skillKey_;
        self.skillCastMode_ = s.skillCastMode_;
        self.observedSkillId_ = s.observedSkillId_;
        self.skillBindingVariant_ = s.skillBindingVariant_;
        self.skillCooldownVariant_ = s.skillCooldownVariant_;
        self.cooldownTraceKey_ = s.cooldownTraceKey_;
        self.cooldownTraceVariant_ = s.cooldownTraceVariant_;
        self.qDragArmSkill_ = s.qDragArmSkill_;
        self.provisionalSkill_ = s.provisionalSkill_;
        self.coreSkill_ = s.coreSkill_;
        self.pendingGoalSkill_ = s.pendingGoalSkill_;
        self.candidateSkillId_ = s.candidateSkillId_;
        self.movementTouch_ = s.movementTouch_;
        self.manualTouch_ = s.manualTouch_;
        self.movementClickMode_ = s.movementClickMode_;
        self.movementHoldMs_ = s.movementHoldMs_;
        self.movementWaypoint_ = s.movementWaypoint_;
        self.now_ = s.now_;
        self.lastFrameNs_ = s.lastFrameNs_;
        self.castStartedNs_ = s.castStartedNs_;
        self.lastSkillEndNs_ = s.lastSkillEndNs_;
        self.frameCoreSampleStartNs_ = s.frameCoreSampleStartNs_;
        self.skillCancelSentNs_ = s.skillCancelSentNs_;
        self.qDragMismatchSinceNs_ = s.qDragMismatchSinceNs_;
        self.qDragArmSentNs_ = s.qDragArmSentNs_;
        self.directionGoalTime_ = s.directionGoalTime_;
        self.directionSampleStart_ = s.directionSampleStart_;
        self.directionSampleEnd_ = s.directionSampleEnd_;
        self.directionLastEchoNs_ = s.directionLastEchoNs_;
        self.skillVariantSampleNs_ = s.skillVariantSampleNs_;
        self.skillVariantSampleStartNs_ = s.skillVariantSampleStartNs_;
        self.pendingGoalTime_ = s.pendingGoalTime_;
        self.logicalSampleNs_ = s.logicalSampleNs_;
        self.lastInputPacketNs_ = s.lastInputPacketNs_;
        self.acceptedSampleNs_ = s.acceptedSampleNs_;
        self.consumedSampleNs_ = s.consumedSampleNs_;
        self.candidateSinceNs_ = s.candidateSinceNs_;
        self.traceSampleStart_ = s.traceSampleStart_;
        self.traceSampleEnd_ = s.traceSampleEnd_;
        self.directionGoalSequence_ = s.directionGoalSequence_;
        self.directionMatchedSequence_ = s.directionMatchedSequence_;
        self.pendingGoalSequence_ = s.pendingGoalSequence_;
        self.touchSequence_ = s.touchSequence_;
        self.verifiedTransportSequence_ = s.verifiedTransportSequence_;
        self.modelClock_ = s.modelClock_;
        self.traceMatchedSequence_ = s.traceMatchedSequence_;
        self.pointerUpdates_ = s.pointerUpdates_;
        self.pointerFlushes_ = s.pointerFlushes_;
        self.pendingWorldResolver_ = s.pendingWorldResolver_;
        self.directionGoalRadius_ = s.directionGoalRadius_;
        self.pendingGoalRadius_ = s.pendingGoalRadius_;
        self.pendingGoalRange_ = s.pendingGoalRange_;
        self.solverRadius_ = s.solverRadius_;
        self.armingDistance_ = s.armingDistance_;
        self.status_ = text(&s.status_);
        self.activeSlot_ = text(&s.activeSlot_);
        self.frameActiveSlot_ = text(&s.frameActiveSlot_);
        self.activeConfigSlot_ = text(&s.activeConfigSlot_);
        self.inputReason_ = text(&s.inputReason_);
        self.cooldownTraceSlot_ = text(&s.cooldownTraceSlot_);
        self.qDragArmSource_ = text(&s.qDragArmSource_);
        self.directRangeSource_ = text(&s.directRangeSource_);
        self.frameEpoch_ = text(&s.frameEpoch_);
        self.pendingGoalSource_ = text(&s.pendingGoalSource_);
        self.logicalSource_ = text(&s.logicalSource_);
        self.navigationStatus_ = text(&s.navigationStatus_);
        self.movementStatus_ = text(&s.movementStatus_);
        self.androidSize_ = s.androidSize_;
        self.pointer_ = s.pointer_;
        self.heroScreen_ = s.heroScreen_;
        self.aimWorld_ = s.aimWorld_;
        self.frameHero_ = s.frameHero_;
        self.joystickCenter_ = s.joystickCenter_;
        self.joystickRadius_ = s.joystickRadius_;
        self.cancelPoint_ = s.cancelPoint_;
        self.skillPoint_ = s.skillPoint_;
        self.solverCenter_ = s.solverCenter_;
        self.skillDownPoint_ = s.skillDownPoint_;
        self.qDragArmPoint_ = s.qDragArmPoint_;
        self.qDragMismatchCell_ = s.qDragMismatchCell_;
        self.qDragMismatchOffset_ = s.qDragMismatchOffset_;
        self.directionCenter_ = s.directionCenter_;
        self.directionRadius_ = s.directionRadius_;
        self.pendingDirection_ = s.pendingDirection_;
        self.directionAcceptedPoint_ = s.directionAcceptedPoint_;
        self.directionObserved_ = s.directionObserved_;
        self.directionGoalPoint_ = s.directionGoalPoint_;
        self.directionGoalCenter_ = s.directionGoalCenter_;
        self.pendingGoalCenter_ = s.pendingGoalCenter_;
        self.logicalOrigin_ = s.logicalOrigin_;
        self.logicalObserved_ = s.logicalObserved_;
        self.acceptedPoint_ = s.acceptedPoint_;
        self.candidateRaw_ = s.candidateRaw_;
        self.candidateTarget_ = s.candidateTarget_;
        self.lastAcceptedHero_ = s.lastAcceptedHero_;
        self.verifiedActual_ = s.verifiedActual_;
        self.traceDesired_ = s.traceDesired_;
        self.traceRaw_ = s.traceRaw_;
        self.traceDecoded_ = s.traceDecoded_;
        self.pendingPointer_ = s.pendingPointer_;
        self.pendingWorld_ = s.pendingWorld_;
        self.movementPress_ = s.movementPress_;
        self.movementCenter_ = s.movementCenter_;
        self.movementRadius_ = s.movementRadius_;
        self.movementTouchPoint_ = s.movementTouchPoint_;
        self.manualPoint_ = s.manualPoint_;
        self.movementTarget_ = s.movementTarget_;
        self.movementRequestedTarget_ = s.movementRequestedTarget_;
        self.movementPreviousHero_ = s.movementPreviousHero_;
        self.movementProgressHero_ = s.movementProgressHero_;
        self.frameCamera_ = s.frameCamera_;
        self.frameMovementCamera_ = s.frameMovementCamera_;
        self.directionGoalCamera_ = s.directionGoalCamera_;
        self.solver = s.solver_;
        self.direct = s.direct_;
        self.history_size = s.causal_.history_len() as u32;
        self.queue_depth = s.queuedSkills_.len() as u32;
        self.queue_front_slot = text(
            s.queuedSkills_
                .first()
                .map(|q| q.slot.as_str())
                .unwrap_or(""),
        );
        self.queue_front_key = s.queuedSkills_.first().map(|q| q.key).unwrap_or(0);
        self.queue_front_held = s.queuedSkills_.first().is_some_and(|q| q.held) as u8;
        self.movement_path_count = s.movementPath_.len() as u32;
        self.movement_waypoint = s
            .movementPath_
            .get(s.movementWaypoint_ as usize)
            .copied()
            .unwrap_or_default();
    }
}
