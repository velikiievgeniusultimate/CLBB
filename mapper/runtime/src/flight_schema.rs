// SPDX-License-Identifier: GPL-3.0-or-later
//! Complete typed input snapshots for recorder/replay; kept out of controller hot logic.
use clbb_moba_core::controller::snapshot as s;
use serde_json::{Value, json};
pub trait Encode {
    fn encode(&self) -> Value;
}
impl Encode for s::Text {
    fn encode(&self) -> Value {
        json!({"value":self.as_str(),"present":self.present,"size":self.size})
    }
}
impl Encode for f64 {
    fn encode(&self) -> Value {
        json!(self)
    }
}
impl Encode for i32 {
    fn encode(&self) -> Value {
        json!(self)
    }
}
impl Encode for u32 {
    fn encode(&self) -> Value {
        json!(self)
    }
}
impl<T: Encode, const N: usize> Encode for [T; N] {
    fn encode(&self) -> Value {
        Value::Array(self.iter().map(Encode::encode).collect())
    }
}
impl Encode for s::Number {
    fn encode(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("value".into(), self.value.encode());
        out.insert("kind".into(), self.kind.encode());
        Value::Object(out)
    }
}
impl Encode for s::Flag {
    fn encode(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("value".into(), self.value.encode());
        Value::Object(out)
    }
}
impl Encode for s::Vector2 {
    fn encode(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("values".into(), self.values.encode());
        out.insert("count".into(), self.count.encode());
        Value::Object(out)
    }
}
impl Encode for s::Vector3 {
    fn encode(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("values".into(), self.values.encode());
        out.insert("count".into(), self.count.encode());
        Value::Object(out)
    }
}
impl Encode for s::Vector16 {
    fn encode(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("values".into(), self.values.encode());
        out.insert("count".into(), self.count.encode());
        Value::Object(out)
    }
}
impl Encode for s::Button {
    fn encode(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("slot".into(), self.slot.encode());
        out.insert("kind".into(), self.kind.encode());
        out.insert("geometry_source".into(), self.geometry_source.encode());
        out.insert("index".into(), self.index.encode());
        out.insert("skill_id".into(), self.skill_id.encode());
        out.insert("active".into(), self.active.encode());
        out.insert("press_uv".into(), self.press_uv.encode());
        out.insert("center_uv".into(), self.center_uv.encode());
        out.insert("radius_uv".into(), self.radius_uv.encode());
        Value::Object(out)
    }
}
impl Encode for s::Geometry {
    fn encode(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("ready".into(), self.ready.encode());
        out.insert("source".into(), self.source.encode());
        out.insert("slot".into(), self.slot.encode());
        out.insert("skill_id".into(), self.skill_id.encode());
        out.insert("sample_start_ns".into(), self.sample_start_ns.encode());
        out.insert("sample_end_ns".into(), self.sample_end_ns.encode());
        out.insert("center_uv".into(), self.center_uv.encode());
        out.insert("radius_uv".into(), self.radius_uv.encode());
        Value::Object(out)
    }
}
impl Encode for s::Variant {
    fn encode(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("slot".into(), self.slot.encode());
        out.insert("base_skill_id".into(), self.base_skill_id.encode());
        out.insert("active_skill_id".into(), self.active_skill_id.encode());
        out.insert("cast_locked".into(), self.cast_locked.encode());
        out.insert("state_forbidden".into(), self.state_forbidden.encode());
        out.insert("charge_skill".into(), self.charge_skill.encode());
        Value::Object(out)
    }
}
impl Encode for s::VariantState {
    fn encode(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("ready".into(), self.ready.encode());
        out.insert("source".into(), self.source.encode());
        out.insert("token".into(), self.token.encode());
        out.insert("hero_id".into(), self.hero_id.encode());
        out.insert("sample_start_ns".into(), self.sample_start_ns.encode());
        out.insert("sample_end_ns".into(), self.sample_end_ns.encode());
        out.insert(
            "buttons".into(),
            Value::Array(
                self.buttons
                    .iter()
                    .take((self.count as usize).min(32))
                    .map(Encode::encode)
                    .collect(),
            ),
        );
        out.insert("count".into(), self.count.encode());
        Value::Object(out)
    }
}
impl Encode for s::CooldownRow {
    fn encode(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("slot".into(), self.slot.encode());
        out.insert("base_skill_id".into(), self.base_skill_id.encode());
        out.insert("active_skill_id".into(), self.active_skill_id.encode());
        out.insert("ready".into(), self.ready.encode());
        out.insert("on_cooldown".into(), self.on_cooldown.encode());
        Value::Object(out)
    }
}
impl Encode for s::CooldownState {
    fn encode(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("present".into(), self.present.encode());
        out.insert("ready".into(), self.ready.encode());
        out.insert("coherent".into(), self.coherent.encode());
        out.insert("source".into(), self.source.encode());
        out.insert("token".into(), self.token.encode());
        out.insert("hero_id".into(), self.hero_id.encode());
        out.insert("sample_start_ns".into(), self.sample_start_ns.encode());
        out.insert("sample_end_ns".into(), self.sample_end_ns.encode());
        out.insert(
            "buttons".into(),
            Value::Array(
                self.buttons
                    .iter()
                    .take((self.count as usize).min(32))
                    .map(Encode::encode)
                    .collect(),
            ),
        );
        out.insert("count".into(), self.count.encode());
        Value::Object(out)
    }
}
impl Encode for s::MovementGeometry {
    fn encode(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("ready".into(), self.ready.encode());
        out.insert("press_uv".into(), self.press_uv.encode());
        out.insert("center_uv".into(), self.center_uv.encode());
        out.insert("radius_uv".into(), self.radius_uv.encode());
        Value::Object(out)
    }
}
impl Encode for s::Ui {
    fn encode(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("ready".into(), self.ready.encode());
        out.insert("hero_id".into(), self.hero_id.encode());
        out.insert("time_ns".into(), self.time_ns.encode());
        out.insert("resolution_px".into(), self.resolution_px.encode());
        out.insert(
            "buttons".into(),
            Value::Array(
                self.buttons
                    .iter()
                    .take((self.button_count as usize).min(32))
                    .map(Encode::encode)
                    .collect(),
            ),
        );
        out.insert("button_count".into(), self.button_count.encode());
        out.insert(
            "capability_controls".into(),
            Value::Array(
                self.capability_controls
                    .iter()
                    .take((self.capability_count as usize).min(32))
                    .map(Encode::encode)
                    .collect(),
            ),
        );
        out.insert("capability_count".into(), self.capability_count.encode());
        out.insert("active_slot".into(), self.active_slot.encode());
        out.insert("joystick".into(), self.joystick.encode());
        out.insert("cancel_uv".into(), self.cancel_uv.encode());
        out.insert("movement".into(), self.movement.encode());
        out.insert("cooldown_state".into(), self.cooldown_state.encode());
        Value::Object(out)
    }
}
impl Encode for s::Motion {
    fn encode(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("source".into(), self.source.encode());
        out.insert("available".into(), self.available.encode());
        out.insert("bracket_stable".into(), self.bracket_stable.encode());
        out.insert(
            "drag_touch_raw_xyz".into(),
            self.drag_touch_raw_xyz.encode(),
        );
        out.insert("sample_start_ns".into(), self.sample_start_ns.encode());
        out.insert("sample_end_ns".into(), self.sample_end_ns.encode());
        Value::Object(out)
    }
}
impl Encode for s::Logical {
    fn encode(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("ready".into(), self.ready.encode());
        out.insert("input_ready".into(), self.input_ready.encode());
        out.insert("coherent".into(), self.coherent.encode());
        out.insert("source".into(), self.source.encode());
        out.insert("skill_id".into(), self.skill_id.encode());
        out.insert("sample_start_ns".into(), self.sample_start_ns.encode());
        out.insert("sample_end_ns".into(), self.sample_end_ns.encode());
        out.insert("origin_render_xyz".into(), self.origin_render_xyz.encode());
        out.insert("raw_touch_xyz".into(), self.raw_touch_xyz.encode());
        out.insert("offset_xz".into(), self.offset_xz.encode());
        out.insert("target_render_xyz".into(), self.target_render_xyz.encode());
        out.insert("joystick".into(), self.joystick.encode());
        out.insert("drag_ratio".into(), self.drag_ratio.encode());
        out.insert("world_range".into(), self.world_range.encode());
        out.insert(
            "world_range_source".into(),
            self.world_range_source.encode(),
        );
        out.insert(
            "world_range_source_present".into(),
            self.world_range_source_present.encode(),
        );
        out.insert("visual_plane_y".into(), self.visual_plane_y.encode());
        out.insert("valid_drag".into(), self.valid_drag.encode());
        out.insert("first_drag".into(), self.first_drag.encode());
        Value::Object(out)
    }
}
impl Encode for s::Direction {
    fn encode(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("covered_binding_ready".into(), self.covered_binding_ready.encode());
        out.insert("covered_base_skill_id".into(), self.covered_base_skill_id.encode());
        out.insert("covered_actual_skill_id".into(), self.covered_actual_skill_id.encode());
        out.insert("covered_source".into(), self.covered_source.encode());
        out.insert("ready".into(), self.ready.encode());
        out.insert("coherent".into(), self.coherent.encode());
        out.insert("source".into(), self.source.encode());
        out.insert("token".into(), self.token.encode());
        out.insert("hero_id".into(), self.hero_id.encode());
        out.insert("slot".into(), self.slot.encode());
        out.insert("skill_id".into(), self.skill_id.encode());
        out.insert("sample_start_ns".into(), self.sample_start_ns.encode());
        out.insert("sample_end_ns".into(), self.sample_end_ns.encode());
        out.insert("raw_touch_xyz".into(), self.raw_touch_xyz.encode());
        out.insert("direction_xz".into(), self.direction_xz.encode());
        Value::Object(out)
    }
}
impl Encode for s::CancelState {
    fn encode(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("ready".into(), self.ready.encode());
        out.insert("coherent".into(), self.coherent.encode());
        out.insert("source".into(), self.source.encode());
        out.insert("token".into(), self.token.encode());
        out.insert("hero_id".into(), self.hero_id.encode());
        out.insert("slot".into(), self.slot.encode());
        out.insert("base_skill_id".into(), self.base_skill_id.encode());
        out.insert("cancelled".into(), self.cancelled.encode());
        out.insert("sample_start_ns".into(), self.sample_start_ns.encode());
        out.insert("sample_end_ns".into(), self.sample_end_ns.encode());
        Value::Object(out)
    }
}
impl Encode for s::Frame {
    fn encode(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("ready".into(), self.ready.encode());
        out.insert("hero_id".into(), self.hero_id.encode());
        out.insert("time_ns".into(), self.time_ns.encode());
        out.insert("game_clock_ms".into(), self.game_clock_ms.encode());
        out.insert(
            "sample_duration_ms".into(),
            self.sample_duration_ms.encode(),
        );
        out.insert("skill_id".into(), self.skill_id.encode());
        out.insert("token".into(), self.token.encode());
        out.insert("token_nonempty".into(), self.token_nonempty.encode());
        out.insert("token_string".into(), self.token_string.encode());
        out.insert("view_matrix".into(), self.view_matrix.encode());
        out.insert("hero_render_xyz".into(), self.hero_render_xyz.encode());
        out.insert("hero_screen_uv".into(), self.hero_screen_uv.encode());
        out.insert("ui".into(), self.ui.encode());
        out.insert("skill_state".into(), self.skill_state.encode());
        out.insert("active_joystick".into(), self.active_joystick.encode());
        out.insert("direction_input".into(), self.direction_input.encode());
        out.insert("cancel_state".into(), self.cancel_state.encode());
        out.insert("logical".into(), self.logical.encode());
        out.insert("motion".into(), self.motion.encode());
        Value::Object(out)
    }
}

impl Encode for u8 {
    fn encode(&self) -> Value {
        json!(self)
    }
}
impl Encode for u64 {
    fn encode(&self) -> Value {
        json!(self)
    }
}
impl Encode for i64 {
    fn encode(&self) -> Value {
        json!(self)
    }
}
impl Encode for clbb_moba_core::Point {
    fn encode(&self) -> Value {
        json!([self.x, self.y])
    }
}
impl Encode for clbb_moba_core::controller::view::Binding {
    fn encode(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("key".into(), self.key.encode());
        out.insert("cast_mode".into(), self.cast_mode.encode());
        out.insert("stop_movement".into(), self.stop_movement.encode());
        Value::Object(out)
    }
}
impl Encode for clbb_moba_core::controller::view::ButtonView {
    fn encode(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("slot".into(), self.slot.encode());
        out.insert("kind".into(), self.kind.encode());
        out.insert("geometry_source".into(), self.geometry_source.encode());
        out.insert("index".into(), self.index.encode());
        out.insert("skill_id".into(), self.skill_id.encode());
        out.insert("key".into(), self.key.encode());
        out.insert("cast_mode".into(), self.cast_mode.encode());
        out.insert("stop_movement".into(), self.stop_movement.encode());
        out.insert("available".into(), self.available.encode());
        out.insert("press".into(), self.press.encode());
        out.insert("center".into(), self.center.encode());
        out.insert("radius".into(), self.radius.encode());
        Value::Object(out)
    }
}
impl Encode for clbb_moba_core::controller::view::VariantView {
    fn encode(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("slot".into(), self.slot.encode());
        out.insert("base".into(), self.base.encode());
        out.insert("active".into(), self.active.encode());
        out.insert("state_forbidden".into(), self.state_forbidden.encode());
        out.insert("charge_skill".into(), self.charge_skill.encode());
        Value::Object(out)
    }
}
impl Encode for clbb_moba_core::controller::view::View {
    fn encode(&self) -> Value {
        let mut out = serde_json::Map::new();
        out.insert("traceEnabled_".into(), self.traceEnabled_.encode());
        out.insert("enabled_".into(), self.enabled_.encode());
        out.insert("ready_".into(), self.ready_.encode());
        out.insert("aimReady_".into(), self.aimReady_.encode());
        out.insert("frameCameraReady_".into(), self.frameCameraReady_.encode());
        out.insert(
            "provisionalStartup_".into(),
            self.provisionalStartup_.encode(),
        );
        out.insert("startupArmed_".into(), self.startupArmed_.encode());
        out.insert(
            "startupMayActivate_".into(),
            self.startupMayActivate_.encode(),
        );
        out.insert("cooldownSeen_".into(), self.cooldownSeen_.encode());
        out.insert(
            "skillCancelPending_".into(),
            self.skillCancelPending_.encode(),
        );
        out.insert(
            "skillCancelAcknowledged_".into(),
            self.skillCancelAcknowledged_.encode(),
        );
        out.insert("qDragArming_".into(), self.qDragArming_.encode());
        out.insert("qDragRecompute_".into(), self.qDragRecompute_.encode());
        out.insert(
            "qDragRecomputeUsed_".into(),
            self.qDragRecomputeUsed_.encode(),
        );
        out.insert(
            "directionGeometryFresh_".into(),
            self.directionGeometryFresh_.encode(),
        );
        out.insert(
            "pendingDirectionValid_".into(),
            self.pendingDirectionValid_.encode(),
        );
        out.insert(
            "directionAccepted_".into(),
            self.directionAccepted_.encode(),
        );
        out.insert(
            "directGeometryFresh_".into(),
            self.directGeometryFresh_.encode(),
        );
        out.insert("logicalMode_".into(), self.logicalMode_.encode());
        out.insert(
            "logicalSampleFresh_".into(),
            self.logicalSampleFresh_.encode(),
        );
        out.insert(
            "logicalInputFresh_".into(),
            self.logicalInputFresh_.encode(),
        );
        out.insert("trajectoryValid_".into(), self.trajectoryValid_.encode());
        out.insert("acceptedInput_".into(), self.acceptedInput_.encode());
        out.insert("acceptedAdapt_".into(), self.acceptedAdapt_.encode());
        out.insert("candidateValid_".into(), self.candidateValid_.encode());
        out.insert(
            "acceptedHeroValid_".into(),
            self.acceptedHeroValid_.encode(),
        );
        out.insert(
            "verifiedTransport_".into(),
            self.verifiedTransport_.encode(),
        );
        out.insert("warmAttempted_".into(), self.warmAttempted_.encode());
        out.insert("skillEchoPending_".into(), self.skillEchoPending_.encode());
        out.insert(
            "traceDesiredValid_".into(),
            self.traceDesiredValid_.encode(),
        );
        out.insert("joystickReady_".into(), self.joystickReady_.encode());
        out.insert("joystickMeasured_".into(), self.joystickMeasured_.encode());
        out.insert("cancelReady_".into(), self.cancelReady_.encode());
        out.insert("skillNudged_".into(), self.skillNudged_.encode());
        out.insert("skillKeyHeld_".into(), self.skillKeyHeld_.encode());
        out.insert("skillExpectedAim_".into(), self.skillExpectedAim_.encode());
        out.insert("skillActivationObserved_".into(), self.skillActivationObserved_.encode());
        out.insert("skillStateForbiddenAtDown_".into(), self.skillStateForbiddenAtDown_.encode());
        out.insert("chargeStage_".into(), self.chargeStage_.encode());
        out.insert("chargeBase_".into(), self.chargeBase_.encode());
        out.insert("chargeFuture_".into(), self.chargeFuture_.encode());
        out.insert("chargeEndClock_".into(), self.chargeEndClock_.encode());
        out.insert("chargeEndSampleNs_".into(), self.chargeEndSampleNs_.encode());
        out.insert("chargeLastSampleNs_".into(), self.chargeLastSampleNs_.encode());
        out.insert("pendingRelease_".into(), self.pendingRelease_.encode());
        out.insert(
            "pendingWorldValid_".into(),
            self.pendingWorldValid_.encode(),
        );
        out.insert("movementReady_".into(), self.movementReady_.encode());
        out.insert("movementHeld_".into(), self.movementHeld_.encode());
        out.insert("movementAuto_".into(), self.movementAuto_.encode());
        out.insert("movementPressed_".into(), self.movementPressed_.encode());
        out.insert(
            "movementEchoPending_".into(),
            self.movementEchoPending_.encode(),
        );
        out.insert(
            "navigationAvailable_".into(),
            self.navigationAvailable_.encode(),
        );
        out.insert("changed_".into(), self.changed_.encode());
        out.insert("heroId_".into(), self.heroId_.encode());
        out.insert("cancelKey_".into(), self.cancelKey_.encode());
        out.insert("aimSkillId_".into(), self.aimSkillId_.encode());
        out.insert("skillTouch_".into(), self.skillTouch_.encode());
        out.insert("skillKey_".into(), self.skillKey_.encode());
        out.insert("skillCastMode_".into(), self.skillCastMode_.encode());
        out.insert("observedSkillId_".into(), self.observedSkillId_.encode());
        out.insert(
            "skillBindingVariant_".into(),
            self.skillBindingVariant_.encode(),
        );
        out.insert(
            "skillCooldownVariant_".into(),
            self.skillCooldownVariant_.encode(),
        );
        out.insert("cooldownTraceKey_".into(), self.cooldownTraceKey_.encode());
        out.insert(
            "cooldownTraceVariant_".into(),
            self.cooldownTraceVariant_.encode(),
        );
        out.insert("qDragArmSkill_".into(), self.qDragArmSkill_.encode());
        out.insert("provisionalSkill_".into(), self.provisionalSkill_.encode());
        out.insert("coreSkill_".into(), self.coreSkill_.encode());
        out.insert("pendingGoalSkill_".into(), self.pendingGoalSkill_.encode());
        out.insert("candidateSkillId_".into(), self.candidateSkillId_.encode());
        out.insert("movementTouch_".into(), self.movementTouch_.encode());
        out.insert("manualTouch_".into(), self.manualTouch_.encode());
        out.insert(
            "movementClickMode_".into(),
            self.movementClickMode_.encode(),
        );
        out.insert("movementHoldMs_".into(), self.movementHoldMs_.encode());
        out.insert("movementWaypoint_".into(), self.movementWaypoint_.encode());
        out.insert("now_".into(), self.now_.encode());
        out.insert("lastFrameNs_".into(), self.lastFrameNs_.encode());
        out.insert("castStartedNs_".into(), self.castStartedNs_.encode());
        out.insert("lastSkillEndNs_".into(), self.lastSkillEndNs_.encode());
        out.insert(
            "frameCoreSampleStartNs_".into(),
            self.frameCoreSampleStartNs_.encode(),
        );
        out.insert(
            "skillCancelSentNs_".into(),
            self.skillCancelSentNs_.encode(),
        );
        out.insert(
            "qDragMismatchSinceNs_".into(),
            self.qDragMismatchSinceNs_.encode(),
        );
        out.insert("qDragArmSentNs_".into(), self.qDragArmSentNs_.encode());
        out.insert(
            "directionGoalTime_".into(),
            self.directionGoalTime_.encode(),
        );
        out.insert(
            "directionSampleStart_".into(),
            self.directionSampleStart_.encode(),
        );
        out.insert(
            "directionSampleEnd_".into(),
            self.directionSampleEnd_.encode(),
        );
        out.insert(
            "directionLastEchoNs_".into(),
            self.directionLastEchoNs_.encode(),
        );
        out.insert(
            "skillVariantSampleNs_".into(),
            self.skillVariantSampleNs_.encode(),
        );
        out.insert(
            "skillVariantSampleStartNs_".into(),
            self.skillVariantSampleStartNs_.encode(),
        );
        out.insert("pendingGoalTime_".into(), self.pendingGoalTime_.encode());
        out.insert("logicalSampleNs_".into(), self.logicalSampleNs_.encode());
        out.insert(
            "lastInputPacketNs_".into(),
            self.lastInputPacketNs_.encode(),
        );
        out.insert("acceptedSampleNs_".into(), self.acceptedSampleNs_.encode());
        out.insert("consumedSampleNs_".into(), self.consumedSampleNs_.encode());
        out.insert("candidateSinceNs_".into(), self.candidateSinceNs_.encode());
        out.insert("traceSampleStart_".into(), self.traceSampleStart_.encode());
        out.insert("traceSampleEnd_".into(), self.traceSampleEnd_.encode());
        out.insert(
            "directionGoalSequence_".into(),
            self.directionGoalSequence_.encode(),
        );
        out.insert(
            "directionMatchedSequence_".into(),
            self.directionMatchedSequence_.encode(),
        );
        out.insert(
            "pendingGoalSequence_".into(),
            self.pendingGoalSequence_.encode(),
        );
        out.insert("touchSequence_".into(), self.touchSequence_.encode());
        out.insert("sentTimeNs_".into(), self.sentTimeNs_.encode());
        out.insert(
            "verifiedTransportSequence_".into(),
            self.verifiedTransportSequence_.encode(),
        );
        out.insert("modelClock_".into(), self.modelClock_.encode());
        out.insert(
            "traceMatchedSequence_".into(),
            self.traceMatchedSequence_.encode(),
        );
        out.insert("pointerUpdates_".into(), self.pointerUpdates_.encode());
        out.insert("pointerFlushes_".into(), self.pointerFlushes_.encode());
        out.insert(
            "pendingWorldResolver_".into(),
            self.pendingWorldResolver_.encode(),
        );
        out.insert(
            "directionGoalRadius_".into(),
            self.directionGoalRadius_.encode(),
        );
        out.insert(
            "pendingGoalRadius_".into(),
            self.pendingGoalRadius_.encode(),
        );
        out.insert("pendingGoalRange_".into(), self.pendingGoalRange_.encode());
        out.insert("solverRadius_".into(), self.solverRadius_.encode());
        out.insert("armingDistance_".into(), self.armingDistance_.encode());
        out.insert("status_".into(), self.status_.encode());
        out.insert("activeSlot_".into(), self.activeSlot_.encode());
        out.insert("frameActiveSlot_".into(), self.frameActiveSlot_.encode());
        out.insert("activeConfigSlot_".into(), self.activeConfigSlot_.encode());
        out.insert("inputReason_".into(), self.inputReason_.encode());
        out.insert(
            "cooldownTraceSlot_".into(),
            self.cooldownTraceSlot_.encode(),
        );
        out.insert("qDragArmSource_".into(), self.qDragArmSource_.encode());
        out.insert(
            "directRangeSource_".into(),
            self.directRangeSource_.encode(),
        );
        out.insert("frameEpoch_".into(), self.frameEpoch_.encode());
        out.insert(
            "pendingGoalSource_".into(),
            self.pendingGoalSource_.encode(),
        );
        out.insert("logicalSource_".into(), self.logicalSource_.encode());
        out.insert("navigationStatus_".into(), self.navigationStatus_.encode());
        out.insert("movementStatus_".into(), self.movementStatus_.encode());
        out.insert("androidSize_".into(), self.androidSize_.encode());
        out.insert("pointer_".into(), self.pointer_.encode());
        out.insert("heroScreen_".into(), self.heroScreen_.encode());
        out.insert("aimWorld_".into(), self.aimWorld_.encode());
        out.insert("frameHero_".into(), self.frameHero_.encode());
        out.insert("joystickCenter_".into(), self.joystickCenter_.encode());
        out.insert("joystickRadius_".into(), self.joystickRadius_.encode());
        out.insert("cancelPoint_".into(), self.cancelPoint_.encode());
        out.insert("skillPoint_".into(), self.skillPoint_.encode());
        out.insert("solverCenter_".into(), self.solverCenter_.encode());
        out.insert("skillDownPoint_".into(), self.skillDownPoint_.encode());
        out.insert("qDragArmPoint_".into(), self.qDragArmPoint_.encode());
        out.insert(
            "qDragMismatchCell_".into(),
            self.qDragMismatchCell_.encode(),
        );
        out.insert(
            "qDragMismatchOffset_".into(),
            self.qDragMismatchOffset_.encode(),
        );
        out.insert("directionCenter_".into(), self.directionCenter_.encode());
        out.insert("directionRadius_".into(), self.directionRadius_.encode());
        out.insert("pendingDirection_".into(), self.pendingDirection_.encode());
        out.insert(
            "directionAcceptedPoint_".into(),
            self.directionAcceptedPoint_.encode(),
        );
        out.insert(
            "directionObserved_".into(),
            self.directionObserved_.encode(),
        );
        out.insert(
            "directionGoalPoint_".into(),
            self.directionGoalPoint_.encode(),
        );
        out.insert(
            "directionGoalCenter_".into(),
            self.directionGoalCenter_.encode(),
        );
        out.insert(
            "pendingGoalCenter_".into(),
            self.pendingGoalCenter_.encode(),
        );
        out.insert("logicalOrigin_".into(), self.logicalOrigin_.encode());
        out.insert("logicalObserved_".into(), self.logicalObserved_.encode());
        out.insert("acceptedPoint_".into(), self.acceptedPoint_.encode());
        out.insert("candidateRaw_".into(), self.candidateRaw_.encode());
        out.insert("candidateTarget_".into(), self.candidateTarget_.encode());
        out.insert("lastAcceptedHero_".into(), self.lastAcceptedHero_.encode());
        out.insert("verifiedActual_".into(), self.verifiedActual_.encode());
        out.insert("traceDesired_".into(), self.traceDesired_.encode());
        out.insert("traceRaw_".into(), self.traceRaw_.encode());
        out.insert("traceDecoded_".into(), self.traceDecoded_.encode());
        out.insert("pendingPointer_".into(), self.pendingPointer_.encode());
        out.insert("pendingWorld_".into(), self.pendingWorld_.encode());
        out.insert("movementPress_".into(), self.movementPress_.encode());
        out.insert("movementCenter_".into(), self.movementCenter_.encode());
        out.insert("movementRadius_".into(), self.movementRadius_.encode());
        out.insert(
            "movementTouchPoint_".into(),
            self.movementTouchPoint_.encode(),
        );
        out.insert("manualPoint_".into(), self.manualPoint_.encode());
        out.insert("movementTarget_".into(), self.movementTarget_.encode());
        out.insert(
            "movementRequestedTarget_".into(),
            self.movementRequestedTarget_.encode(),
        );
        out.insert(
            "movementPreviousHero_".into(),
            self.movementPreviousHero_.encode(),
        );
        out.insert(
            "movementProgressHero_".into(),
            self.movementProgressHero_.encode(),
        );
        out.insert("frameCamera_".into(), self.frameCamera_.encode());
        out.insert(
            "frameMovementCamera_".into(),
            self.frameMovementCamera_.encode(),
        );
        out.insert(
            "directionGoalCamera_".into(),
            self.directionGoalCamera_.encode(),
        );
        out.insert(
            "buttons".into(),
            Value::Array(
                self.buttons
                    .iter()
                    .take((self.button_count as usize).min(32))
                    .map(Encode::encode)
                    .collect(),
            ),
        );
        out.insert("button_count".into(), self.button_count.encode());
        out.insert(
            "variants".into(),
            Value::Array(
                self.variants
                    .iter()
                    .take((self.variant_count as usize).min(32))
                    .map(Encode::encode)
                    .collect(),
            ),
        );
        out.insert("variant_count".into(), self.variant_count.encode());
        out.insert("history_size".into(), self.history_size.encode());
        out.insert("queue_depth".into(), self.queue_depth.encode());
        out.insert("queue_front_slot".into(), self.queue_front_slot.encode());
        out.insert("queue_front_key".into(), self.queue_front_key.encode());
        out.insert("queue_front_held".into(), self.queue_front_held.encode());
        out.insert(
            "movement_path_count".into(),
            self.movement_path_count.encode(),
        );
        out.insert("movement_waypoint".into(), self.movement_waypoint.encode());
        Value::Object(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn covered_direction_recording_keeps_unknown_binding_and_exact_source_separate_from_native_ack() {
        let mut input=s::Direction::default();let missing=input.encode();
        assert_eq!(missing["covered_binding_ready"]["value"],-1);
        assert_eq!(missing["covered_base_skill_id"]["kind"],0);
        assert_eq!(missing["covered_actual_skill_id"]["kind"],0);
        assert_eq!(missing["covered_source"]["present"],0);
        input.covered_base_skill_id=s::Number {value:1_013_110.,kind:1};
        input.covered_actual_skill_id=s::Number {value:2_013_110.,kind:1};
        input.covered_source=crate::source::text("IndicatorMotionBase.lCoverSkills");
        for ready in [-1,0,1] {
            input.covered_binding_ready.value=ready;
            let encoded=input.encode();let raw=serde_json::to_vec(&encoded).unwrap();
            let row:Value=serde_json::from_slice(&raw).unwrap();assert_eq!(row,encoded);
            assert_eq!(row["covered_binding_ready"]["value"],ready);
            assert_eq!(row["covered_base_skill_id"]["value"],1_013_110.);
            assert_eq!(row["covered_actual_skill_id"]["value"],2_013_110.);
            assert_eq!(row["covered_actual_skill_id"]["kind"],1);
            assert_eq!(row["covered_source"]["value"],"IndicatorMotionBase.lCoverSkills");
            assert_eq!(row["ready"]["value"],-1,"a binding observation is not a native unit-vector acknowledgement");
        }
        input.covered_actual_skill_id.kind=2;
        assert_eq!(input.encode()["covered_actual_skill_id"]["kind"],2,"record malformed types without silently converting them");
    }
    #[test]
    fn charge_clock_and_lifecycle_recordings_survive_json_roundtrip_without_losing_unknowns() {
        let mut frame=s::Frame::default();
        assert_eq!(frame.encode()["game_clock_ms"]["kind"],0);
        frame.game_clock_ms=s::Number {value:u32::MAX as f64,kind:1};
        frame.skill_state.count=1;
        for value in [-1,0,1] {
            frame.skill_state.buttons[0].charge_skill.value=value;
            let encoded=frame.encode();let raw=serde_json::to_vec(&encoded).unwrap();
            let row:Value=serde_json::from_slice(&raw).unwrap();
            assert_eq!(row,encoded);
            assert_eq!(row["game_clock_ms"]["kind"],1);
            assert_eq!(row["game_clock_ms"]["value"],json!(u32::MAX as f64));
            assert_eq!(row["skill_state"]["buttons"][0]["charge_skill"]["value"],value);
            let mut variant=clbb_moba_core::controller::view::VariantView::default();
            variant.charge_skill.value=value;
            assert_eq!(variant.encode()["charge_skill"]["value"],value);
        }
        let mut view=clbb_moba_core::controller::view::View::default();
        assert_eq!(view.encode()["chargeEndClock_"]["kind"],0);
        view.chargeStage_=2;view.chargeBase_=1_013_110;view.chargeFuture_=2_013_110;
        view.chargeEndClock_=s::Number {value:u32::MAX as f64,kind:1};
        view.chargeEndSampleNs_=9_007_199_254_740_993;view.chargeLastSampleNs_=9_007_199_254_740_999;
        let encoded=view.encode();let raw=serde_json::to_vec(&encoded).unwrap();
        let row:Value=serde_json::from_slice(&raw).unwrap();
        assert_eq!(row,encoded);assert_eq!(row["chargeStage_"],2);
        assert_eq!(row["chargeBase_"],1_013_110);assert_eq!(row["chargeFuture_"],2_013_110);
        assert_eq!(row["chargeEndClock_"]["kind"],1);
        assert_eq!(row["chargeEndSampleNs_"].as_i64(),Some(9_007_199_254_740_993));
        assert_eq!(row["chargeLastSampleNs_"].as_i64(),Some(9_007_199_254_740_999));
    }
    #[test]
    fn state_forbidden_recording_preserves_unknown_false_and_true() {
        for value in [-1, 0, 1] {
            let mut variant = s::Variant::default();
            variant.state_forbidden.value = value;
            assert_eq!(variant.encode()["state_forbidden"]["value"], value);
            let mut view = clbb_moba_core::controller::view::VariantView::default();
            view.state_forbidden.value = value;
            assert_eq!(view.encode()["state_forbidden"]["value"], value);
        }
    }
    #[test]
    fn missing_values_and_capacity_are_not_silently_flattened() {
        let mut frame = s::Frame::default();
        frame.ui.button_count = 1;
        frame.ui.buttons[0].slot.present = 1;
        frame.ui.buttons[0].slot.size = 0;
        let row = frame.encode();
        assert_eq!(row["ready"]["value"], -1);
        assert_eq!(row["hero_id"]["kind"], 0);
        assert_eq!(row["token"]["present"], 0);
        assert_eq!(row["ui"]["buttons"][0]["slot"]["present"], 1);
        assert_eq!(row["ui"]["buttons"].as_array().unwrap().len(), 1);
        assert_eq!(row["skill_state"]["buttons"].as_array().unwrap().len(), 0);
    }
    #[test]
    fn every_public_snapshot_field_has_an_encoder() {
        // The public input contract is intentionally simple repr(C) structs.
        // Fail when future inputs are added without recorder coverage.
        let source = include_str!("../../core/src/controller/snapshot.rs");
        let encoder = include_str!("flight_schema.rs");
        for block in source.split("pub struct ").skip(1) {
            let name = block.split_whitespace().next().unwrap();
            if matches!(name, "Text") {
                continue;
            }
            let body = block.split_once('{').unwrap().1.split_once('}').unwrap().0;
            let implementation = encoder
                .split_once(&format!("impl Encode for s::{name} "))
                .unwrap()
                .1
                .split_once("Value::Object(out)")
                .unwrap()
                .0;
            for line in body
                .lines()
                .filter_map(|line| line.trim().strip_prefix("pub "))
            {
                let field = line.split(':').next().unwrap();
                assert!(
                    implementation.split_whitespace().collect::<String>().contains(&format!("self.{field}.")),
                    "missing {name}.{field}"
                );
            }
        }
    }
}
