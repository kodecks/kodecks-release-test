use crate::card_def;
use kodecks::prelude::*;

card_def!(
    CardDef,
    "tung", // spellchecker:disable-line
    "Tungsten Rhino",
    color: Color::RED,
    cost: 5,
    card_type: CardType::Creature,
    creature_type: CreatureType::Robot,
    power: 500,
    abilities: &[KeywordAbility::Piercing][..],
    shields: 1,
);

impl Effect for CardDef {}
