use bevy::prelude::*;
use std::collections::HashMap;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SquadId(pub usize);

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
}

impl Default for SquadDoctrineEntry {
    fn default() -> Self {
        Self {
            phase: SquadTacticalPhase::Normal,
            contact_lost_timer: 0.0,
            perceived_enemy_count: 0,
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
        match self.squads.get(&sq_id).map(|e| e.phase).unwrap_or_default() {
            SquadTacticalPhase::Normal => "SQUAD 1: NORMAL (Following Commander Intent)".to_string(),
            SquadTacticalPhase::ReactToContact {
                spotted_unaware_enemy,
                ..
            } => {
                if spotted_unaware_enemy {
                    "SQUAD 1 [REACT TO CONTACT]: Visual Contact Only • SL Assessing METT-TC..."
                        .to_string()
                } else {
                    "SQUAD 1 [REACT TO CONTACT]: Taking Cover & Returning Fire • SL Assessing METT-TC..."
                        .to_string()
                }
            }
            SquadTacticalPhase::DecisionActive(decision) => {
                format!("SQUAD 1 [SL ORDER]: {}", decision.label())
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

