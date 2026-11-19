// SPDX-License-Identifier: GPL-3.0-or-later
//! Skill resources sampled from the local Actor and the exact skill/form/level.
use crate::{
    platform::Memory,
    reflection::{Reflection, Result, i32_at},
    skill_config::{entries, integer, owner, pointer, table, typed_field},
    source::{ButtonPlan, Checks, capture, check, flag},
};
use clbb_moba_core::controller::snapshot::CooldownState;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Cost {
    mana: i32,
    energy: i32,
}
impl Cost {
    fn discover<M: Memory>(
        r: &mut Reflection<M>,
        checks: &mut Checks,
        row: u64,
        skill: i32,
        level: i32,
    ) -> Result<Self> {
        owner(r, checks, row, "CData_Skill_Element")?;
        if integer(r, checks, row, "m_SkillID")? != skill
            || integer(r, checks, row, "m_SkillLevel")? != level
        {
            return Err("Skill cost belongs to another form or level".into());
        }
        let cost = Self {
            mana: integer(r, checks, row, "m_NeedMP")?,
            energy: integer(r, checks, row, "m_NeedXP")?,
        };
        if !(0..=100_000).contains(&cost.mana) || !(0..=100_000).contains(&cost.energy) {
            return Err("Skill cost outside supported bounds".into());
        }
        Ok(cost)
    }
}

struct Catalog {
    guards: Checks,
    skills: HashMap<i32, u64>,
    levels: HashMap<(i32, i32), (i64, Option<Cost>)>,
}
impl Catalog {
    fn discover<M: Memory>(r: &mut Reflection<M>) -> Result<Self> {
        r.discover(&["CData_Skill"])?;
        let mut guards = Vec::new();
        let (_, _, dictionary) = table(r, &mut guards, "CData_Skill")?;
        let skills = entries(r, &mut guards, dictionary, 20_000)?
            .into_iter()
            .collect();
        check(r, &guards)?;
        // The dictionary identity, count, version and entry array remain
        // guarded. The large, immutable entry array is checked once on load.
        guards.pop();
        Ok(Self {
            guards,
            skills,
            levels: HashMap::new(),
        })
    }

    fn cost<M: Memory>(
        &mut self,
        r: &mut Reflection<M>,
        skill: i32,
        level: i32,
        now: i64,
    ) -> Option<Cost> {
        let key = (skill, level);
        if let Some(&(retry, cached)) = self.levels.get(&key) {
            if cached.is_some() || now < retry {
                return cached;
            }
        }
        let loaded = (|| -> Result<Cost> {
            let mut checks = Vec::new();
            let skill_rows = *self
                .skills
                .get(&skill)
                .ok_or("Skill form absent from the cost table")?;
            owner(r, &mut checks, skill_rows, "SharedDictionary`2")?;
            let dictionary = pointer(r, &mut checks, skill_rows, "m_dic")?;
            let exact = entries(r, &mut checks, dictionary, 100)?
                .into_iter()
                .find(|(candidate, _)| *candidate == level)
                .ok_or("Skill level absent from the cost table")?;
            let cost = Cost::discover(r, &mut checks, exact.1, skill, level)?;
            check(r, &checks)?;
            Ok(cost)
        })()
        .ok();
        self.levels.insert(key, (now + 250_000_000, loaded));
        loaded
    }
}

pub(crate) struct ResourcePlan {
    pay_cost: u64,
    mana: u64,
    mana_max: u64,
    mana_modifier: u64,
    energy: u64,
    energy_max: u64,
    energy_modifier: u64,
    catalog: Option<Catalog>,
    retry_ns: i64,
}

struct Resources {
    mana: i32,
    mana_max: i32,
    mana_modifier: i32,
    energy: i32,
    energy_max: i32,
    energy_modifier: i32,
}

impl Resources {
    fn shortage(&self, cost: Cost) -> Option<bool> {
        // Only compare costs with an observed, applicable resource. An
        // unverified modifier must never turn a payable cast into a rejection.
        let lacks = |cost, amount, maximum, modifier| {
            if cost == 0 {
                Some(false)
            } else if maximum > 0 && (0..=maximum).contains(&amount) && modifier == 0 {
                Some(amount < cost)
            } else {
                None
            }
        };
        match (
            lacks(cost.mana, self.mana, self.mana_max, self.mana_modifier),
            lacks(
                cost.energy,
                self.energy,
                self.energy_max,
                self.energy_modifier,
            ),
        ) {
            (Some(true), _) | (_, Some(true)) => Some(true),
            (Some(false), Some(false)) => Some(false),
            _ => None,
        }
    }
}

impl ResourcePlan {
    pub(crate) fn discover<M: Memory>(r: &mut Reflection<M>, actor: u64) -> Result<Self> {
        let mut checks = Vec::new();
        let mut at = |name, kind| {
            typed_field(r, &mut checks, actor, name, &[kind]).map(|(address, _)| address)
        };
        Ok(Self {
            pay_cost: at("m_bNeedSkillCost", 2)?,
            mana: at("<m_Mp>k__BackingField", 8)?,
            mana_max: at("_MpMax", 8)?,
            mana_modifier: at("<m_MpCostPer>k__BackingField", 8)?,
            energy: at("<m_Xp>k__BackingField", 8)?,
            energy_max: at("_XpMax", 8)?,
            energy_modifier: at("<m_XpCostPer>k__BackingField", 8)?,
            catalog: None,
            retry_ns: 0,
        })
    }

    pub(crate) fn sample<M: Memory>(
        &mut self,
        r: &mut Reflection<M>,
        hero: i32,
        now: i64,
        buttons: &[ButtonPlan],
        state: &mut CooldownState,
        consumed: &mut Checks,
    ) {
        // Hylos pays with health when he cannot afford the mana cost.
        if hero == 49 {
            return;
        }
        let spans = [
            (self.pay_cost, 1),
            (self.mana, 4),
            (self.mana_max, 4),
            (self.mana_modifier, 4),
            (self.energy, 4),
            (self.energy_max, 4),
            (self.energy_modifier, 4),
        ];
        let mut raw = [0; 25];
        if r.memory.read_vectored(&spans, &mut raw).is_err() || raw[0] != 1 {
            return;
        }
        let resources = Resources {
            mana: i32_at(&raw, 1),
            mana_max: i32_at(&raw, 5),
            mana_modifier: i32_at(&raw, 9),
            energy: i32_at(&raw, 13),
            energy_max: i32_at(&raw, 17),
            energy_modifier: i32_at(&raw, 21),
        };
        if resources.mana_max <= 0 && resources.energy_max <= 0 {
            return;
        }
        if self
            .catalog
            .as_ref()
            .is_some_and(|c| check(r, &c.guards).is_err())
        {
            self.catalog = None;
            self.retry_ns = now;
        }
        if self.catalog.is_none() && now >= self.retry_ns {
            self.catalog = Catalog::discover(r).ok();
            self.retry_ns = now + 250_000_000;
        }
        let Some(catalog) = &mut self.catalog else {
            return;
        };
        let mut offset = 0;
        for (address, size) in spans {
            consumed.push((address, raw[offset..offset + size].to_vec()));
            offset += size;
        }
        consumed.extend(catalog.guards.iter().cloned());
        for row in state.buttons.iter_mut().take(state.count.min(32) as usize) {
            if row.resource_shortage.get() {
                continue;
            }
            let Some(button) = buttons
                .iter()
                .find(|b| b.slot == row.slot.as_str() && b.kind == "skill")
            else {
                continue;
            };
            let skill = row.active_skill_id.int();
            if skill <= 0 || button.data == 0 {
                continue;
            }
            let checkpoint = consumed.len();
            let level = button.skill_level.and_then(|address| {
                capture(r, consumed, address, 4)
                    .ok()
                    .map(|raw| i32_at(&raw, 0))
            });
            let Some(level) = level.filter(|v| (1..=64).contains(v)) else {
                consumed.truncate(checkpoint);
                continue;
            };
            let cost = catalog.cost(r, skill, level, now);
            if let Some(shortage) = cost.and_then(|cost| resources.shortage(cost)) {
                row.resource_shortage = flag(shortage);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn costs_use_their_own_resource_and_allow_an_exact_balance() {
        let resources = Resources {
            mana: 139,
            mana_max: 500,
            mana_modifier: 0,
            energy: 12,
            energy_max: 100,
            energy_modifier: 0,
        };
        assert_eq!(
            resources.shortage(Cost {
                mana: 140,
                energy: 0
            }),
            Some(true)
        );
        assert_eq!(
            resources.shortage(Cost {
                mana: 139,
                energy: 0
            }),
            Some(false)
        );
        assert_eq!(
            resources.shortage(Cost {
                mana: 0,
                energy: 14
            }),
            Some(true)
        );
        assert_eq!(
            resources.shortage(Cost {
                mana: 0,
                energy: 12
            }),
            Some(false)
        );
        assert_eq!(resources.shortage(Cost { mana: 0, energy: 0 }), Some(false));
        let discounted = Resources {
            mana_modifier: 25,
            ..resources
        };
        assert_eq!(
            discounted.shortage(Cost {
                mana: 140,
                energy: 0
            }),
            None
        );
    }
}
