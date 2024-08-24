use crate::{
    ability::{AnonymousAbility, KeywordAbility},
    color::Color,
    computed::ComputedAttribute,
    deck::DeckItem,
    effect::{Effect, NoEffect},
    event::EventFilter,
    id::{CardId, ObjectId, ObjectIdCounter},
    linear::Linear,
    player::{PlayerId, PlayerZone},
    score::Score,
    zone::Zone,
};
use core::{fmt, panic};
use serde::{Deserialize, Serialize};
use std::{ops::Index, sync::LazyLock};
use tinystr::TinyAsciiStr;

pub type CardMap = phf::Map<&'static str, fn() -> &'static CardArchetype>;

pub struct Catalog {
    pub str_index: &'static CardMap,
}

impl Index<&str> for Catalog {
    type Output = CardArchetype;

    fn index(&self, safe_name: &str) -> &Self::Output {
        if safe_name.is_empty() {
            return CardArchetype::NONE();
        }
        if let Some(entry) = self.str_index.get(safe_name) {
            entry()
        } else {
            panic!("Card not found: {}", safe_name)
        }
    }
}

impl Index<TinyAsciiStr<8>> for Catalog {
    type Output = CardArchetype;

    fn index(&self, short_id: TinyAsciiStr<8>) -> &Self::Output {
        self.index(short_id.as_str())
    }
}

pub struct Card {
    id: ObjectId,
    owner: PlayerId,
    zone: PlayerZone,
    controller: PlayerId,
    archetype: &'static CardArchetype,
    computed: ComputedAttribute,
    event_filter: EventFilter,
    effect: Box<dyn Effect>,
    timestamp: u64,
}

impl fmt::Debug for Card {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "<{} {}>", self.id, self.archetype.name)
    }
}

impl Card {
    pub fn new(
        counter: &mut ObjectIdCounter,
        item: &DeckItem,
        archetype: &'static CardArchetype,
        owner: PlayerId,
    ) -> Self {
        let effect = (archetype.effect)();
        Self {
            id: counter.allocate(item.base_id),
            owner,
            zone: PlayerZone::new(owner, Zone::Deck),
            controller: owner,
            archetype,
            computed: archetype.into(),
            event_filter: effect.event_filter(),
            effect,
            timestamp: 0,
        }
    }

    pub fn id(&self) -> ObjectId {
        self.id
    }

    pub fn owner(&self) -> PlayerId {
        self.owner
    }

    pub fn controller(&self) -> PlayerId {
        self.controller
    }

    pub fn zone(&self) -> &PlayerZone {
        &self.zone
    }

    pub fn set_zone(&mut self, zone: PlayerZone) {
        self.zone = zone;
    }

    pub fn archetype(&self) -> &'static CardArchetype {
        self.archetype
    }

    pub fn computed(&self) -> &ComputedAttribute {
        &self.computed
    }

    pub fn computed_mut(&mut self) -> &mut ComputedAttribute {
        &mut self.computed
    }

    pub fn reset_computed(&mut self) {
        self.computed = self.archetype.into();
    }

    pub fn event_filter(&self) -> EventFilter {
        self.event_filter
    }

    pub fn effect(&self) -> Box<dyn Effect> {
        #[allow(clippy::deref_addrof)]
        dyn_clone::clone_box(&**&self.effect)
    }

    pub fn set_effect(&mut self, effect: Box<dyn Effect>) {
        self.effect = effect;
    }

    pub fn timestamp(&self) -> u64 {
        self.timestamp
    }

    pub fn set_timestamp(&mut self, timestamp: u64) {
        self.timestamp = timestamp;
    }

    pub fn snapshot(&self) -> CardSnapshot {
        CardSnapshot {
            id: self.id,
            archetype_id: self.archetype.id,
            owner: self.owner,
            computed: Some(self.computed.clone()),
            timestamp: self.timestamp,
        }
    }

    pub fn duplicate(&self) -> Self {
        Self {
            id: self.id,
            owner: self.owner,
            zone: self.zone,
            controller: self.controller,
            archetype: self.archetype,
            computed: self.computed.clone(),
            event_filter: self.event_filter,
            effect: (self.archetype.effect)(),
            timestamp: self.timestamp,
        }
    }

    pub fn renew_id(&mut self, counter: &mut ObjectIdCounter) {
        self.id = counter.allocate(Some(self.id));
    }
}

impl CardId for Card {
    fn id(&self) -> ObjectId {
        self.id
    }
}

impl Score for Card {
    fn score(&self) -> i32 {
        self.computed.abilities.score()
            + self.computed.anon_abilities.score()
            + self.computed.power.map(|power| power.value()).unwrap_or(0) as i32 / 100
            + if self.computed.is_creature() { 1 } else { 0 }
    }
}

pub fn safe_name(name: &str) -> Result<String, idna::Errors> {
    idna::domain_to_ascii(&name.replace(' ', "-"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardSnapshot {
    pub id: ObjectId,
    pub archetype_id: TinyAsciiStr<8>,
    pub owner: PlayerId,
    pub computed: Option<ComputedAttribute>,
    pub timestamp: u64,
}

impl CardId for CardSnapshot {
    fn id(&self) -> ObjectId {
        self.id
    }
}

impl CardSnapshot {
    pub fn redacted(self) -> Self {
        Self {
            archetype_id: TinyAsciiStr::from_bytes_lossy(b""),
            computed: None,
            timestamp: 0,
            ..self
        }
    }

    pub fn color(&self) -> Color {
        self.computed
            .as_ref()
            .map(|c| c.color)
            .unwrap_or(Color::empty())
    }

    pub fn cost(&self) -> Linear<u8> {
        self.computed.as_ref().map(|c| c.cost).unwrap_or_default()
    }

    pub fn power(&self) -> Option<Linear<u32>> {
        self.computed.as_ref().and_then(|c| c.power)
    }
}

impl fmt::Display for CardSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let computed = if let Some(computed) = &self.computed {
            computed
        } else {
            return write!(f, "<???>",);
        };
        let color = match computed.color {
            Color::RUBY => "C",
            Color::TOPAZ => "A",
            Color::JADE => "J",
            Color::AZURE => "Z",
            _ => "--",
        };
        let clock = format!(" {}", self.power().map(|p| p.value()).unwrap_or(0));
        write!(f, "<({color}{}){clock} {}>", computed.cost.value(), self.id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardArchetype {
    pub id: TinyAsciiStr<8>,
    pub name: &'static str,
    pub safe_name: &'static str,
    pub attribute: CardAttribute,
    pub effect: fn() -> Box<dyn Effect>,
}

impl CardArchetype {
    pub const NONE: fn() -> &'static CardArchetype = || {
        static CACHE: LazyLock<CardArchetype> = LazyLock::new(CardArchetype::default);
        &CACHE
    };
}

impl Default for CardArchetype {
    fn default() -> Self {
        Self {
            id: TinyAsciiStr::from_bytes_lossy(b""),
            name: "",
            safe_name: "",
            attribute: CardAttribute::default(),
            effect: no_effect(),
        }
    }
}

fn no_effect() -> fn() -> Box<dyn Effect> {
    NoEffect::NEW
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardAttribute {
    pub color: Color,
    pub cost: u8,
    pub card_type: CardType,
    pub abilities: &'static [KeywordAbility],
    pub anon_abilities: &'static [AnonymousAbility],
    pub power: Option<u32>,
}

impl Default for CardAttribute {
    fn default() -> Self {
        Self {
            color: Color::COLORLESS,
            cost: 0,
            card_type: CardType::Hex,
            abilities: &[],
            anon_abilities: &[],
            power: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardType {
    Creature,
    Hex,
}