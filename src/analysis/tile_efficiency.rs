use std::collections::HashMap;
use std::error::Error;
use std::fmt;

use crate::mahjong::hand::{Hand, HandMutationError};
use crate::mahjong::player_index::PlayerIndex;
use crate::mahjong::round::RoundState;
use crate::mahjong::tile::{Tile, TileKind};

use super::shanten::hand_shanten;

const TILE_KIND_COUNT: usize = 34;

/// 一种候选牌及其当前未见枚数。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileAvailability {
    pub kind: TileKind,
    pub unseen: u8,
}

/// 打出一张牌后的进张候选。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DrawCandidates {
    /// 尚未听牌时，能够降低向听数的牌。
    Effective(Vec<TileAvailability>),
    /// 听牌时，摸到后向听数为 `-1`、牌形完成的牌。
    ///
    /// 这里只判断牌形，不保证存在役或满足完整的合法和牌条件。
    Winning(Vec<TileAvailability>),
}

/// 一种切牌选择的基础牌效率结果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiscardEfficiency {
    pub discard: Tile,
    pub shanten: i8,
    pub candidates: DrawCandidates,
    pub total_unseen: u8,
}

/// 牌效率分析无法完成的原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnalysisError {
    /// 当前操作要求另一种等效手牌张数。
    InvalidHandSize { expected: usize, actual: usize },
    /// 有效牌只对一向听及更远的手牌定义。
    EffectiveTilesRequireShantenAtLeastOne { actual: i8 },
    /// 和了牌枚举只对听牌手牌定义。
    WinningTilesRequireTenpai { actual: i8 },
    /// 当前暗牌中不存在指定的切牌。
    DiscardNotFound { discard: Tile },
    /// 模拟摸牌或切牌时手牌修改失败。
    HandMutation(HandMutationError),
    /// 切牌后得到的状态既不是听牌，也不是一向听及更远。
    InvalidShantenAfterDiscard { actual: i8 },
}

/// 枚举能降低当前向听数的牌种。
pub fn effective_tile_kinds(hand: &Hand) -> Result<Vec<TileKind>, AnalysisError> {
    validate_hand_size(hand, Hand::MIN_TILE_COUNT)?;

    let current_shanten = hand_shanten(hand);
    if current_shanten < 1 {
        return Err(AnalysisError::EffectiveTilesRequireShantenAtLeastOne {
            actual: current_shanten,
        });
    }

    candidate_tile_kinds(hand, |candidate_shanten| {
        candidate_shanten < current_shanten
    })
}

/// 枚举能使听牌手牌达到完成牌形的牌种。
///
/// 候选牌只要求摸入后向听数为 `-1`，不保证存在役或满足完整的合法和牌条件。
pub fn winning_tile_kinds(hand: &Hand) -> Result<Vec<TileKind>, AnalysisError> {
    validate_hand_size(hand, Hand::MIN_TILE_COUNT)?;

    let current_shanten = hand_shanten(hand);
    if current_shanten != 0 {
        return Err(AnalysisError::WinningTilesRequireTenpai {
            actual: current_shanten,
        });
    }

    candidate_tile_kinds(hand, |candidate_shanten| candidate_shanten == -1)
}

/// 返回当前玩家视角下一种牌的未见枚数。
pub fn unseen_count(state: &RoundState, player: PlayerIndex, kind: TileKind) -> u8 {
    let own_concealed = state
        .player(player)
        .hand()
        .concealed()
        .iter()
        .filter(|tile| tile.kind() == kind)
        .count();

    let visible_discards = state
        .players()
        .iter()
        .flat_map(|player| player.discards())
        .filter(|discard| !discard.is_called() && discard.tile().kind() == kind)
        .count();

    let visible_melds = state
        .players()
        .iter()
        .flat_map(|player| player.hand().melds())
        .flat_map(|meld| meld.tiles())
        .filter(|tile| tile.kind() == kind)
        .count();

    let dora_indicators = state
        .dora_indicators()
        .iter()
        .filter(|tile| tile.kind() == kind)
        .count();

    let visible = own_concealed + visible_discards + visible_melds + dora_indicators;
    4usize.saturating_sub(visible) as u8
}

/// 分析打出指定牌后的向听数、候选进张和未见枚数。
pub fn discard_efficiency(
    state: &RoundState,
    player: PlayerIndex,
    discard: Tile,
) -> Result<DiscardEfficiency, AnalysisError> {
    let hand = state.player(player).hand();
    analyze_discard(hand, discard, |kind| unseen_count(state, player, kind))
}

// 假设分支复用同一套牌效率计算，枚数由分支的可见信息提供。
pub(crate) fn analyze_discard(
    hand: &Hand,
    discard: Tile,
    unseen: impl Fn(TileKind) -> u8,
) -> Result<DiscardEfficiency, AnalysisError> {
    analyze_discard_using(hand, discard, unseen, hand_shanten)
}

/// 缓存仅活在一次比较内；牌形不含可见枚数，分支的枚数始终另外计算。
#[derive(Default)]
pub(crate) struct EfficiencyCache {
    distances: HashMap<([u8; 34], Vec<crate::mahjong::meld::Meld>), i8>,
}

impl EfficiencyCache {
    pub(crate) fn shanten(&mut self, hand: &Hand) -> i8 {
        let key = (
            super::shanten::concealed_counts(hand),
            hand.melds().to_vec(),
        );
        *self
            .distances
            .entry(key)
            .or_insert_with(|| hand_shanten(hand))
    }

    pub(crate) fn discard(
        &mut self,
        hand: &Hand,
        discard: Tile,
        unseen: impl Fn(TileKind) -> u8,
    ) -> Result<DiscardEfficiency, AnalysisError> {
        analyze_discard_using(hand, discard, unseen, |hand| self.shanten(hand))
    }
}

fn analyze_discard_using(
    hand: &Hand,
    discard: Tile,
    unseen: impl Fn(TileKind) -> u8,
    mut distance: impl FnMut(&Hand) -> i8,
) -> Result<DiscardEfficiency, AnalysisError> {
    validate_hand_size(hand, Hand::MAX_TILE_COUNT)?;
    if !hand.concealed().contains(&discard) {
        return Err(AnalysisError::DiscardNotFound { discard });
    }

    let mut after_discard = hand.clone();
    after_discard
        .discard(discard)
        .map_err(AnalysisError::HandMutation)?;
    let shanten = distance(&after_discard);

    if shanten < 0 {
        return Err(AnalysisError::InvalidShantenAfterDiscard { actual: shanten });
    }
    let kinds = candidate_tile_kinds_using(&after_discard, |s| s < shanten, &mut distance)?;

    let candidates = match shanten {
        1.. => DrawCandidates::Effective(with_availability(kinds, &unseen)),
        0 => DrawCandidates::Winning(with_availability(kinds, &unseen)),
        actual => return Err(AnalysisError::InvalidShantenAfterDiscard { actual }),
    };
    let total_unseen = match &candidates {
        DrawCandidates::Effective(tiles) | DrawCandidates::Winning(tiles) => {
            tiles.iter().map(|tile| tile.unseen).sum()
        }
    };

    Ok(DiscardEfficiency {
        discard,
        shanten,
        candidates,
        total_unseen,
    })
}

/// 分析当前手牌中所有不同的切牌选择。
pub fn discard_efficiencies(
    state: &RoundState,
    player: PlayerIndex,
) -> Result<Vec<DiscardEfficiency>, AnalysisError> {
    let hand = state.player(player).hand();
    validate_hand_size(hand, Hand::MAX_TILE_COUNT)?;

    let mut discards = hand.concealed().to_vec();
    discards.dedup();
    discards
        .into_iter()
        .map(|discard| discard_efficiency(state, player, discard))
        .collect()
}

fn validate_hand_size(hand: &Hand, expected: usize) -> Result<(), AnalysisError> {
    let actual = hand.effective_tile_count();
    if actual == expected {
        Ok(())
    } else {
        Err(AnalysisError::InvalidHandSize { expected, actual })
    }
}

fn candidate_tile_kinds(
    hand: &Hand,
    accepts: impl Fn(i8) -> bool,
) -> Result<Vec<TileKind>, AnalysisError> {
    candidate_tile_kinds_using(hand, accepts, hand_shanten)
}

fn candidate_tile_kinds_using(
    hand: &Hand,
    accepts: impl Fn(i8) -> bool,
    mut distance: impl FnMut(&Hand) -> i8,
) -> Result<Vec<TileKind>, AnalysisError> {
    let counts = all_hand_counts(hand);
    let mut candidates = Vec::new();

    for value in 0..TILE_KIND_COUNT as u8 {
        let kind = TileKind::new(value).unwrap_or_else(|| unreachable!("0..34 are valid kinds"));
        if counts[value as usize] >= 4 {
            continue;
        }

        let tile = Tile::new(value).unwrap_or_else(|| unreachable!("tile kinds are valid tiles"));
        let mut after_draw = hand.clone();
        after_draw.draw(tile).map_err(AnalysisError::HandMutation)?;
        if accepts(distance(&after_draw)) {
            candidates.push(kind);
        }
    }

    Ok(candidates)
}

fn all_hand_counts(hand: &Hand) -> [u8; TILE_KIND_COUNT] {
    let mut counts = [0; TILE_KIND_COUNT];
    for tile in hand
        .concealed()
        .iter()
        .chain(hand.melds().iter().flat_map(|meld| meld.tiles()))
    {
        counts[tile.kind().as_u8() as usize] += 1;
    }
    counts
}

fn with_availability(
    kinds: Vec<TileKind>,
    unseen: impl Fn(TileKind) -> u8,
) -> Vec<TileAvailability> {
    kinds
        .into_iter()
        .map(|kind| TileAvailability {
            kind,
            unseen: unseen(kind),
        })
        .collect()
}

impl fmt::Display for AnalysisError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidHandSize { expected, actual } => write!(
                formatter,
                "analysis requires {expected} effective tiles, but the hand has {actual}"
            ),
            Self::EffectiveTilesRequireShantenAtLeastOne { actual } => write!(
                formatter,
                "effective tiles require shanten >= 1, but the hand has shanten {actual}"
            ),
            Self::WinningTilesRequireTenpai { actual } => write!(
                formatter,
                "winning tiles require shanten 0, but the hand has shanten {actual}"
            ),
            Self::DiscardNotFound { discard } => write!(
                formatter,
                "tile {} is not in the concealed hand",
                discard.as_u8()
            ),
            Self::HandMutation(error) => error.fmt(formatter),
            Self::InvalidShantenAfterDiscard { actual } => write!(
                formatter,
                "discarding produced unsupported shanten {actual}; expected shanten >= 0"
            ),
        }
    }
}

impl Error for AnalysisError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::HandMutation(error) => Some(error),
            _ => None,
        }
    }
}
