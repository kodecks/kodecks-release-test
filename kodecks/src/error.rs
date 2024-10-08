use crate::{
    color::Color,
    id::{ObjectId, TimedObjectId},
};
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ActionError {
    #[error("Insufficient shards: {color} {amount}")]
    InsufficientShards { color: Color, amount: u32 },
    #[error("Creature already free-casted")]
    CreatureAlreadyFreeCasted,
    #[error("Card not found: {id}")]
    CardNotFound { id: ObjectId },
    #[error("Key not found: {key}")]
    KeyNotFound { key: String },
    #[error("Invalid value type")]
    InvalidValueType,
    #[error("Target lost: {target}")]
    TargetLost { target: TimedObjectId },
}
