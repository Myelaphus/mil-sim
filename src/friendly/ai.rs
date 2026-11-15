use bevy::prelude::*;
use std::collections::HashMap;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SquadId(pub usize);

/// Standard U.S. Army Infantry Squad Order of Succession (Paragraph 5, Command and Signal, OPORD):
/// 1. Squad Leader (Primary): `(fireteam == 0, slot == 0)` (or `(1, 0)` when no dedicated SL slot was spawned)
/// 2. Alpha Team Leader (Senior Team Leader): `(fireteam == 1, slot == 0)` (or `(1, 1)` in 9-man `(1..=2)` layout)
/// 3. Bravo Team Leader: `(fireteam == 2, slot == 0)` (takes command if SL and Alpha TL are casualties or
///    Alpha TL is pinned down / separated out of communication)
/// 4. Senior surviving squad member
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SquadLeaderRole {
    #[default]
    SquadLeader,
    ActingAlphaTeamLeader,
    ActingBravoTeamLeader,
    ActingSeniorRifleman,
    NoCommand,
}

impl SquadLeaderRole {
    pub fn short_title(&self) -> &'static str {
        match self {
            SquadLeaderRole::SquadLeader => "SL",
            SquadLeaderRole::ActingAlphaTeamLeader => "Acting SL: Alpha TL",
            SquadLeaderRole::ActingBravoTeamLeader => "Acting SL: Bravo TL",
            SquadLeaderRole::ActingSeniorRifleman => "Acting SL: Senior Rifleman",
            SquadLeaderRole::NoCommand => "NO COMMAND",
        }
    }

    pub fn is_acting(&self) -> bool {
        !matches!(
            self,
            SquadLeaderRole::SquadLeader | SquadLeaderRole::NoCommand
        )
    }
}

/// Evaluates the Standard Order of Succession across a squad's living members:
/// `(fireteam, slot, pos, is_suppressed)` and returns `Some((leader_ft, leader_slot, leader_pos, role))`.
///
/// Order of Succession:
/// 1. Primary Squad Leader (`(0, 0)`, or `(1, 0)` in 9-man RTS config where `(1, 4)` exists).
/// 2. Alpha Team Leader (`(1, 0)`, or `(1, 1)` when `(1, 0)` was Primary SL) — unless pinned down
///    by suppression or separated (`> 32m` from squad centroid) while Bravo Team Leader `(2, 0)` is
///    able to command.
/// 3. Bravo Team Leader `(2, 0)` — steps up if Alpha Team Leader is a casualty, or is pinned down /
///    separated out of communication.
/// 4. Senior surviving rifleman in chain of command `(fireteam, slot)`.
pub fn select_squad_commander(
    members: &[(usize, usize, Vec2, bool)],
    had_nine_man_sl_at_1_0: bool,
) -> Option<(usize, usize, Vec2, SquadLeaderRole)> {
    if members.is_empty() {
        return None;
    }

    let centroid = members
        .iter()
        .fold(Vec2::ZERO, |acc, (_, _, pos, _)| acc + *pos)
        / (members.len() as f32);

    // 1. Primary Squad Leader: dedicated `(0, 0)` or `(1, 0)` in the 9-man RTS squad
    if let Some(&(ft, slot, pos, _)) = members
        .iter()
        .find(|&&(ft, slot, _, _)| (ft == 0 && slot == 0) || (had_nine_man_sl_at_1_0 && ft == 1 && slot == 0))
    {
        return Some((ft, slot, pos, SquadLeaderRole::SquadLeader));
    }

    // Determine the candidate slot for Alpha Team Leader:
    // - If `(1, 0)` is alive (e.g., 8-man squad after Player SL is KIA, or `(1, 0)` in smaller test squads): `(1, 0)`
    // - Otherwise, in the 9-man RTS squad where `(1, 0)` was the Squad Leader, `(1, 1)` is the Alpha Team Leader.
    let alpha_tl_candidate = members
        .iter()
        .find(|&&(ft, slot, _, _)| ft == 1 && slot == 0)
        .copied()
        .or_else(|| {
            if had_nine_man_sl_at_1_0 {
                members
                    .iter()
                    .find(|&&(ft, slot, _, _)| ft == 1 && slot == 1)
                    .copied()
            } else {
                None
            }
        });

    let bravo_tl_candidate = members
        .iter()
        .find(|&&(ft, slot, _, _)| ft == 2 && slot == 0)
        .copied();

    match (alpha_tl_candidate, bravo_tl_candidate) {
        (Some((a_ft, a_slot, a_pos, a_suppressed)), Some((b_ft, b_slot, b_pos, b_suppressed))) => {
            let alpha_separated = members.len() >= 3 && (a_pos - centroid).length() > 32.0;
            let bravo_separated = members.len() >= 3 && (b_pos - centroid).length() > 32.0;
            let alpha_out_of_comm = a_suppressed || alpha_separated;
            let bravo_able = !b_suppressed && !bravo_separated;

            // If Alpha Team Leader is pinned down or separated out of communication and Bravo TL is able,
            // Bravo Team Leader assumes command of the squad.
            if alpha_out_of_comm && bravo_able {
                Some((
                    b_ft,
                    b_slot,
                    b_pos,
                    SquadLeaderRole::ActingBravoTeamLeader,
                ))
            } else {
                Some((
                    a_ft,
                    a_slot,
                    a_pos,
                    SquadLeaderRole::ActingAlphaTeamLeader,
                ))
            }
        }
        (Some((a_ft, a_slot, a_pos, _)), None) => Some((
            a_ft,
            a_slot,
            a_pos,
            SquadLeaderRole::ActingAlphaTeamLeader,
        )),
        (None, Some((b_ft, b_slot, b_pos, _))) => Some((
            b_ft,
            b_slot,
            b_pos,
            SquadLeaderRole::ActingBravoTeamLeader,
        )),
        (None, None) => {
            // Both Team Leaders are casualties: senior surviving squad member takes command
            let &(ft, slot, pos, _) = members
                .iter()
                .min_by_key(|&&(ft, slot, _, _)| (ft, slot))?;
            Some((ft, slot, pos, SquadLeaderRole::ActingSeniorRifleman))
        }
    }
}

/// Five primary U.S. Army Squad Leader tactical decisions following React to Contact & METT-TC assessment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SquadLeaderDecision {
    /// 1. Squad Attack (Flank and Destroy): FT1 establishes stationary support-by-fire to suppress;
    ///    Squad Leader personally leads FT2 along a covered route to assault through the enemy's flank.
    SquadAttack {
        flank_right: bool,
        target_center: Vec2,
    },
    /// 2. Break Contact (Disengage and Withdraw): Reverse bounding overwatch where one fire team
    ///    lays maximum suppressive fire while the other bounds backward toward a rally point.
    BreakContact {
        rally_point: Vec2,
        active_bound_ft: usize,
        bound_timer: f32,
    },
    /// 3. Fix and Call for Support (Hold in Place): Both fire teams take strong cover and maintain
    ///    sustained suppressive fire to pin the enemy down while reporting contact up to higher echelon.
    FixAndSupport {
        target_center: Vec2,
    },
    /// 4. Bypass (Avoid and Report): Plots a wide detour around enemy observation/weapons range
    ///    with strict hold-fire discipline to prioritize reaching a deeper objective.
    Bypass {
        detour_waypoint: Vec2,
        threat_center: Vec2,
    },
    /// 5. Hasty Ambush (Wait and Spring): Squad drops into concealed positions facing an approaching
    ///    enemy's route, holds fire until the enemy enters the kill zone, then opens simultaneous fire.
    HastyAmbush {
        kill_zone: Vec2,
        triggered: bool,
        trigger_timer: f32,
    },
}

impl SquadLeaderDecision {
    pub fn label(&self) -> &'static str {
        match self {
            SquadLeaderDecision::SquadAttack { flank_right, .. } => {
                if *flank_right {
                    "SQUAD ATTACK (Alpha Suppressing • SL + Bravo Flanking East)"
                } else {
                    "SQUAD ATTACK (Alpha Suppressing • SL + Bravo Flanking West)"
                }
            }
            SquadLeaderDecision::BreakContact { active_bound_ft, .. } => {
                if *active_bound_ft == 2 {
                    "BREAK CONTACT (Alpha Suppressing • Bravo Bounding Rearward)"
                } else {
                    "BREAK CONTACT (Bravo Suppressing • Alpha Bounding Rearward)"
                }
            }
            SquadLeaderDecision::FixAndSupport { .. } => {
                "FIX & CALL FOR SUPPORT (Alpha & Bravo Suppressing from Cover)"
            }
            SquadLeaderDecision::Bypass { .. } => {
                "BYPASS (Hold Fire • Detouring Around Enemy Sector)"
            }
            SquadLeaderDecision::HastyAmbush { triggered, .. } => {
                if *triggered {
                    "HASTY AMBUSH (Ambush Sprung • Rapid Fire Into Kill Zone!)"
                } else {
                    "HASTY AMBUSH (Concealed • Waiting for Enemy in Kill Zone)"
                }
            }
        }
    }
}

/// Per-squad battle drill and decision state machine.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum SquadTacticalPhase {
    #[default]
    Normal,
    ReactToContact {
        assess_timer: f32,
        contact_center: Vec2,
        spotted_unaware_enemy: bool,
    },
    DecisionActive(SquadLeaderDecision),
}

#[derive(Debug, Clone)]
pub struct SquadDoctrineEntry {
    pub phase: SquadTacticalPhase,
    pub contact_lost_timer: f32,
    pub perceived_enemy_count: usize,
    pub leader_role: SquadLeaderRole,
    pub leader_slot: Option<(usize, usize)>,
    pub had_nine_man_sl_at_1_0: bool,
    pub last_commander_order: Option<crate::common::types::CommanderOrder>,
}

impl Default for SquadDoctrineEntry {
    fn default() -> Self {
        Self {
            phase: SquadTacticalPhase::Normal,
            contact_lost_timer: 0.0,
            perceived_enemy_count: 0,
            leader_role: SquadLeaderRole::SquadLeader,
            leader_slot: None,
            had_nine_man_sl_at_1_0: false,
            last_commander_order: None,
        }
    }
}

#[derive(Resource, Default, Debug, Clone)]
pub struct SquadDoctrineState {
    pub squads: HashMap<usize, SquadDoctrineEntry>,
}

impl SquadDoctrineState {
    pub fn reset(&mut self) {
        self.squads.clear();
    }

    pub fn entry_mut(&mut self, sq_id: usize) -> &mut SquadDoctrineEntry {
        self.squads.entry(sq_id).or_default()
    }

    pub fn summary_for_squad(&self, sq_id: usize) -> String {
        let entry = self.squads.get(&sq_id);
        let phase = entry.map(|e| e.phase).unwrap_or_default();
        let role = entry.map(|e| e.leader_role).unwrap_or_default();
        let cmd_title = role.short_title();

        match phase {
            SquadTacticalPhase::Normal => {
                if role.is_acting() {
                    format!(
                        "SQUAD 1 ({cmd_title}): NORMAL (Following Commander Intent)"
                    )
                } else {
                    "SQUAD 1: NORMAL (Following Commander Intent)".to_string()
                }
            }
            SquadTacticalPhase::ReactToContact {
                spotted_unaware_enemy,
                ..
            } => {
                if spotted_unaware_enemy {
                    format!(
                        "SQUAD 1 [REACT TO CONTACT]: Visual Contact Only • {cmd_title} Assessing METT-TC..."
                    )
                } else {
                    format!(
                        "SQUAD 1 [REACT TO CONTACT]: Taking Cover & Returning Fire • {cmd_title} Assessing METT-TC..."
                    )
                }
            }
            SquadTacticalPhase::DecisionActive(decision) => {
                format!("SQUAD 1 [{cmd_title} ORDER]: {}", decision.label())
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub enum FriendlyAiState {
    #[default]
    Follow,
    Engage,
}

#[derive(Component, Debug, Clone)]
pub struct Friendly {
    pub fireteam: usize, // 1 = First Fire Team (Alpha), 2 = Second Fire Team (Bravo)
    pub slot: usize,     // Slot within the fire team
    pub state: FriendlyAiState,
    pub shoot_timer: Timer,
    pub speed: f32,
    pub vision_range: f32,
    pub vision_half_angle: f32,
    pub inner_radius: f32,
    pub target_pos: Option<Vec2>,
    pub target_lost_timer: f32,
}

