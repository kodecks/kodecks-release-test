use crate::{
    card::Card,
    config::GameConfig,
    error::Error,
    id::ObjectId,
    phase::Phase,
    player::{PlayerList, PlayerState, PlayerZone},
    zone::CardZone,
};

use super::GameCondition;

#[derive(Clone)]
pub struct GameState {
    pub config: GameConfig,
    pub turn: u32,
    pub phase: Phase,
    pub players: PlayerList<PlayerState>,
}

impl GameState {
    pub fn find_card(&self, card: ObjectId) -> Result<&Card, Error> {
        self.players
            .iter()
            .filter_map(|player| player.find_card(card))
            .next()
            .ok_or(Error::CardNotFound { id: card })
    }

    pub fn find_card_mut(&mut self, card: ObjectId) -> Result<&mut Card, Error> {
        self.players
            .iter_mut()
            .filter_map(|player| player.find_card_mut(card))
            .next()
            .ok_or(Error::CardNotFound { id: card })
    }

    pub fn find_zone(&self, card: ObjectId) -> Result<PlayerZone, Error> {
        for player in self.players.iter() {
            if let Some(zone) = player.find_zone(card) {
                return Ok(PlayerZone {
                    player: player.id,
                    zone,
                });
            }
        }
        Err(Error::CardNotFound { id: card })
    }

    pub fn players(&self) -> &PlayerList<PlayerState> {
        &self.players
    }

    pub fn check_game_condition(&self) -> GameCondition {
        let survived_players = self
            .players
            .iter()
            .filter(|player| !(player.stats.life == 0 || player.deck.is_empty()))
            .collect::<Vec<_>>();

        match survived_players.as_slice() {
            [] => GameCondition::Draw,
            [winner] => GameCondition::Win(winner.id),
            _ => GameCondition::Progress,
        }
    }
}