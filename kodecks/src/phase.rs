use serde::{Deserialize, Serialize};
use strum::Display;

#[derive(Debug, Clone, Display, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Standby,
    Draw,
    Main,
    Block,
    Battle,
    End,
}