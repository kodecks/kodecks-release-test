use crate::{score::ComputedScore, Bot};
use kodecks::{
    action::{Action, AvailableAction, PlayerAvailableActions},
    env::Environment,
    score::Score,
};
use std::sync::Arc;
use tracing::info;

pub struct SimpleBot;

impl Bot for SimpleBot {
    fn compute(
        &mut self,
        env: Arc<Environment>,
        actions: &PlayerAvailableActions,
    ) -> Vec<(Action, ComputedScore)> {
        for action in actions.actions.as_ref() {
            if let AvailableAction::SelectCard {
                cards,
                score_factor,
            } = action
            {
                let best_candidate = cards
                    .iter()
                    .filter_map(|id| env.state.find_card(*id).ok())
                    .map(|card| {
                        (
                            card.id(),
                            ComputedScore {
                                base: 0,
                                action: card.score()
                                    * score_factor
                                    * if card.zone().player == actions.player {
                                        1
                                    } else {
                                        -1
                                    },
                            },
                        )
                    })
                    .max_by_key(|(_, score)| score.score());
                if let Some((card, score)) = best_candidate {
                    return vec![(Action::SelectCard { card }, score)];
                }
            }

            if let AvailableAction::Attack { attackers } = action {
                let player = env.state.players().get(actions.player);
                let opponent = env
                    .state
                    .players()
                    .get(env.state.players.next(actions.player));

                let blockers = opponent
                    .field
                    .active_cards()
                    .filter_map(|card| card.computed().power)
                    .map(|power| power.value())
                    .collect::<Vec<_>>();
                let blocker_power_sum = blockers.iter().sum::<u32>();
                let max_blocker_power = blockers.iter().copied().max().unwrap_or_default();
                if blocker_power_sum >= player.stats.life {
                    return vec![(
                        Action::Attack { attackers: vec![] },
                        ComputedScore::default(),
                    )];
                }
                let attackers = attackers
                    .iter()
                    .filter_map(|id| env.state.find_card(*id).ok())
                    .filter(|card| {
                        let power = card
                            .computed()
                            .power
                            .map(|power| power.value())
                            .unwrap_or_default();
                        power > 0 && power > max_blocker_power
                    })
                    .map(|card| card.id())
                    .collect::<Vec<_>>();
                if !attackers.is_empty() {
                    return vec![(Action::Attack { attackers }, ComputedScore::default())];
                }
            }

            if let AvailableAction::Block { blockers } = action {
                let player = env.state.players().get(actions.player);
                let opponent = env
                    .state
                    .players()
                    .get(env.state.players.next(actions.player));
                let mut blockers = blockers
                    .iter()
                    .filter_map(|id| env.state.find_card(*id).ok())
                    .collect::<Vec<_>>();
                blockers.sort_by_key(|card| {
                    card.computed()
                        .power
                        .map(|power| power.value())
                        .unwrap_or_default() as i32
                });
                let mut attackers = opponent.field.attacking_cards().collect::<Vec<_>>();
                attackers.sort_by_key(|card| {
                    card.computed()
                        .power
                        .map(|power| power.value())
                        .unwrap_or_default() as i32
                });
                let mut pairs = vec![];
                while !attackers.is_empty() && !blockers.is_empty() {
                    let attackers_power_sum = attackers
                        .iter()
                        .map(|card| {
                            card.computed()
                                .power
                                .map(|power| power.value())
                                .unwrap_or_default()
                        })
                        .sum::<u32>();
                    let attacker = attackers.pop().unwrap();
                    let attacker_power = attacker
                        .computed()
                        .power
                        .map(|power| power.value())
                        .unwrap_or_default();

                    info!(
                        "attacker_power_sum: {} {}",
                        attackers_power_sum, player.stats.life
                    );

                    if attacker_power > 0 {
                        let blocker = blockers.iter().position(|blocker| {
                            blocker
                                .computed()
                                .power
                                .map(|power| power.value())
                                .unwrap_or_default()
                                > attacker_power
                                || attackers_power_sum >= player.stats.life
                        });
                        if let Some(index) = blocker {
                            let blocker = blockers.remove(index);
                            pairs.push((attacker.id(), blocker.id()));
                        }
                    }
                }
                return vec![(Action::Block { pairs }, ComputedScore::default())];
            }

            if let AvailableAction::CastCard { cards } = action {
                let best_candidate = cards
                    .iter()
                    .filter_map(|id| env.state.find_card(*id).ok())
                    .map(|card| {
                        (
                            card.id(),
                            ComputedScore {
                                base: 0,
                                action: card.score(),
                            },
                        )
                    })
                    .max_by_key(|(_, score)| score.score());
                if let Some((card, score)) = best_candidate {
                    return vec![(Action::CastCard { card }, score)];
                }
            }
        }

        actions
            .actions
            .default_action()
            .map(|action| (action, ComputedScore::default()))
            .into_iter()
            .collect()
    }
}