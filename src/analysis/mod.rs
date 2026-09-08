pub mod agari;
mod agari_context;
mod bonus;
pub(crate) mod discard_comparison;
pub(crate) mod hand_structure;
mod points;
pub(crate) mod route_facts;
pub(crate) mod score_scenario;
mod scoring;
pub mod shanten;
pub mod tile_efficiency;
pub(crate) mod visible_hand;
pub(crate) mod winning_value;
mod yaku;

pub use agari_context::{AgariContext, RiichiStatus, RonSource, TsumoSource, WinMethod};
pub use bonus::{BonusError, BonusHan, calculate_bonus_han};
pub(crate) use bonus::{dora_from_indicator, known_bonus};
pub use points::{Payments, PointsError, calculate_payments};
pub use scoring::{HandValue, ScoringError, calculate_hand_value};
pub use shanten::{chiitoitsu_shanten, kokushi_shanten, shanten, standard_shanten};
pub use tile_efficiency::{
    AnalysisError, DiscardEfficiency, DrawCandidates, TileAvailability, discard_efficiencies,
    discard_efficiency, effective_tile_kinds, unseen_count, winning_tile_kinds,
};
pub use yaku::{Yaku, YakuDetectionError, YakuDistanceError, detect_yaku, yaku_shanten};
