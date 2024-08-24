use crate::{
    action::{Action, PlayerAvailableActions},
    computed::ComputedSequence,
    config::{DebugFlags, GameConfig},
    continuous::ContinuousEffectList,
    effect::EffectTriggerContext,
    error::Error,
    filter_vec,
    game::Report,
    log::LogAction,
    opcode::OpcodeList,
    phase::Phase,
    player::{PlayerId, PlayerList, PlayerState},
    stack::{Stack, StackItem},
    zone::CardZone,
};
use serde::{Deserialize, Serialize};
use std::{collections::VecDeque, fmt};
use tracing::{error, warn};

mod action;
mod event;
mod local;
mod opcode;
mod phase;
mod state;

pub use local::LocalEnvironment;
pub use state::*;

#[derive(Clone)]
pub struct Environment {
    pub state: GameState,
    opcodes: VecDeque<OpcodeList>,
    stack: Stack<StackItem>,
    continuous: ContinuousEffectList,
    game_condition: GameCondition,
    ts_counter: u64,
    last_available_actions: Option<PlayerAvailableActions>,
}

impl Environment {
    pub fn new(config: GameConfig, players: Vec<PlayerState>) -> Self {
        let current_player = players.first().as_ref().unwrap().id;

        Environment {
            state: GameState {
                config,
                turn: 0,
                phase: Phase::Standby,
                players: PlayerList::new(current_player, players),
            },
            opcodes: VecDeque::new(),
            stack: Stack::new(),
            continuous: Default::default(),
            game_condition: GameCondition::Progress,
            ts_counter: 0,
            last_available_actions: None,
        }
    }

    fn compute_effects(&mut self) -> Result<(), Error> {
        let sides = self
            .state
            .players
            .iter()
            .map(|player| player.id)
            .collect::<Vec<_>>();
        for side in sides {
            let abilities = self.continuous.apply_player(&self.state, side);
            let player = self.state.players.get(side);
            let field_states = player
                .field
                .iter()
                .map(|card| self.continuous.apply_card(&self.state, card))
                .collect();
            let hand_states = player
                .hand
                .items()
                .map(|item| {
                    let mut attrs = self.continuous.apply_card(&self.state, &item.card);
                    attrs.cost.add(item.cost_delta);
                    attrs
                })
                .collect();
            let player = self.state.players.get_mut(side);
            player.abilities = abilities;
            player.field.update_computed(field_states);
            player.hand.update_computed(hand_states);
        }
        Ok(())
    }

    pub fn process(&mut self, player: PlayerId, action: Option<Action>) -> Report {
        let report = match (&self.last_available_actions, action.clone()) {
            (None, _) => self.process_turn(player, None),
            (Some(available), Some(action)) if available.validate(player, &action) => {
                self.process_turn(player, Some(action))
            }
            _ => {
                warn!("Invalid action: {:?} for player: {}", action, player);
                Report {
                    available_actions: self.last_available_actions.clone(),
                    logs: vec![],
                    condition: self.game_condition,
                }
            }
        };
        self.last_available_actions = report.available_actions.clone();
        report
    }

    fn process_turn(&mut self, player: PlayerId, mut action: Option<Action>) -> Report {
        let action = match action.take() {
            Some(Action::Concede) => {
                let loser = self.state.players.get_mut(player);
                loser.stats.life = 0;
                self.game_condition = GameCondition::Win(loser.id);
                None
            }
            Some(Action::DebugCommand(commands))
                if self.state.config.debug.contains(DebugFlags::DEBUG_COMMAND) =>
            {
                for command in commands {
                    match command.into_opcodes(self) {
                        Ok(log) => self.opcodes.extend(log),
                        Err(err) => {
                            error!("Error processing command: {:?}", err);
                        }
                    }
                }
                None
            }
            other => other,
        };

        if self.game_condition.is_ended() {
            return Report {
                available_actions: None,
                logs: vec![],
                condition: self.game_condition,
            };
        }

        if let Some(item) = self.stack.pop() {
            let card = self.state.find_card(item.source).unwrap();
            let mut ctx = EffectTriggerContext::new(&self.state, card);

            let targeted = match &action {
                Some(Action::SelectCard { card }) => Some(LogAction::CardsTargeted {
                    source: item.source,
                    targets: vec![*card],
                }),
                _ => None,
            };

            match (item.handler)(&mut ctx, action) {
                Ok(report) => {
                    let (continuous, _) = ctx.into_inner();
                    self.continuous.extend(continuous);

                    let mut list = vec![];
                    for command in report.commands {
                        match command.into_opcodes(self) {
                            Ok(codes) => list.extend(codes),
                            Err(err) => {
                                error!("Error processing command: {:?}", err);
                            }
                        }
                    }

                    let mut logs = filter_vec![targeted,];

                    for item in list {
                        for opcode in item {
                            match self.execute(&opcode) {
                                Ok(log) => logs.extend(log),
                                Err(err) => {
                                    error!("Error executing opcode: {:?}", err);
                                }
                            }
                        }
                    }

                    self.continuous.update(&self.state);
                    if let Err(err) = self.compute_effects() {
                        error!("Error computing effects: {:?}", err);
                    }

                    if !self.game_condition.is_ended() {
                        self.game_condition = self.state.check_game_condition();
                    }
                    if !report
                        .available_actions
                        .as_ref()
                        .map_or(true, |item| item.actions.is_empty())
                    {
                        self.stack.push(item);
                    }
                    return Report {
                        available_actions: report.available_actions,
                        logs,
                        condition: self.game_condition,
                    };
                }
                Err(err) => {
                    error!("Error processing stack item: {:?}", err);
                }
            }
        } else if self.opcodes.is_empty() {
            let mut phase = self.state.phase.clone();
            let opcodes = match self.process_player_phase(action, &mut phase) {
                Ok(opcodes) => opcodes,
                Err(err) => {
                    error!("Error processing player phase: {:?}", err);
                    vec![]
                }
            };
            self.opcodes.extend(opcodes);
            self.state.phase = phase;
        }

        let next = self.opcodes.pop_front();
        let next_empty = next.is_none();

        let mut logs = vec![];
        if let Some(log) = next {
            for opcode in log {
                match self.execute(&opcode) {
                    Ok(log) => logs.extend(log),
                    Err(err) => {
                        error!("Error executing opcode: {:?}", err);
                    }
                }
            }
        }

        self.continuous.update(&self.state);
        if let Err(err) = self.compute_effects() {
            error!("Error computing effects: {:?}", err);
        }

        let available_actions = if next_empty {
            self.available_actions()
        } else {
            None
        };

        if !self.game_condition.is_ended() {
            self.game_condition = self.state.check_game_condition();
        }

        Report {
            available_actions,
            logs,
            condition: self.game_condition,
        }
    }

    pub fn last_available_actions(&self) -> Option<&PlayerAvailableActions> {
        self.last_available_actions.as_ref()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GameCondition {
    Progress,
    Win(PlayerId),
    Draw,
}

impl GameCondition {
    pub fn is_ended(&self) -> bool {
        !matches!(self, GameCondition::Progress)
    }
}

impl fmt::Display for GameCondition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GameCondition::Progress => write!(f, "Progress"),
            GameCondition::Win(player) => write!(f, "{} wins", player),
            GameCondition::Draw => write!(f, "Draw"),
        }
    }
}