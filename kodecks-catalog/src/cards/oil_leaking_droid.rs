use crate::card_def;
use kodecks::prelude::*;

card_def!(
    CardDef,
    "oill",
    "Oil-Leaking Droid",
    color: Color::RUBY,
    cost: 1,
    card_type: CardType::Creature,
    power: 100,
    abilities: &[KeywordAbility::Toxic][..],
);

impl Effect for CardDef {}