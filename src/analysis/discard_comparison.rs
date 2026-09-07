//! 在同一份可见信息下比较切牌，以及指定摸牌后的牌形变化。

use super::{
    AnalysisError, DiscardEfficiency, shanten::hand_shanten, tile_efficiency::analyze_discard,
};
use crate::mahjong::{
    hand::Hand,
    meld::Meld,
    tile::{Tile, TileKind},
};

/// 仅供复盘工具使用；不暴露牌局状态，也不执行任何实际动作。
pub(crate) struct ComparisonContext {
    hand: Hand,
    unseen: [u8; 34],
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ComparisonError {
    Analysis(AnalysisError),
    InvalidMeld,
    TooManyCopies { kind: TileKind },
    SameDiscard,
    ExhaustedDraw { kind: TileKind },
}

impl From<AnalysisError> for ComparisonError {
    fn from(error: AnalysisError) -> Self {
        Self::Analysis(error)
    }
}

pub(crate) struct DiscardComparison {
    pub first: DiscardBranch,
    pub second: DiscardBranch,
    pub connections_before: Vec<TileConnections>,
}

pub(crate) struct DiscardBranch {
    pub concealed: Vec<Tile>,
    pub efficiency: DiscardEfficiency,
    pub followup: Option<DrawBranch>,
}

pub(crate) struct DrawBranch {
    pub draw: TileKind,
    pub completed_shape: bool,
    pub next_discards: Vec<DiscardEfficiency>,
}

/// 局部连接可重叠，不表示整手牌已经选定了一种拆分。
pub(crate) struct TileConnections {
    pub kind: TileKind,
    pub copies: u8,
    pub sequence_neighbors: Vec<TileKind>,
}

impl ComparisonContext {
    /// additional_visible 包含其他玩家副露、未被鸣走的舍牌及指示牌，不重复包含自家手牌和副露。
    pub(crate) fn new(hand: Hand, additional_visible: &[Tile]) -> Result<Self, ComparisonError> {
        if hand.effective_tile_count() != Hand::MAX_TILE_COUNT {
            return Err(AnalysisError::InvalidHandSize {
                expected: Hand::MAX_TILE_COUNT,
                actual: hand.effective_tile_count(),
            }
            .into());
        }
        for meld in hand.melds() {
            let kinds: Vec<_> = meld.tiles().iter().map(|t| t.kind().as_u8()).collect();
            let valid = match meld {
                Meld::Chi { .. } => {
                    kinds[0] < 27
                        && kinds[0] / 9 == kinds[2] / 9
                        && kinds[0] + 1 == kinds[1]
                        && kinds[1] + 1 == kinds[2]
                }
                _ => kinds.iter().all(|k| *k == kinds[0]),
            };
            if !valid
                || meld
                    .called()
                    .is_some_and(|tile| !meld.tiles().contains(&tile))
            {
                return Err(ComparisonError::InvalidMeld);
            }
        }
        let mut unseen = [4; 34];
        for tile in hand
            .concealed()
            .iter()
            .chain(hand.melds().iter().flat_map(|m| m.tiles()))
            .chain(additional_visible)
        {
            let count = &mut unseen[tile.kind().as_u8() as usize];
            if *count == 0 {
                return Err(ComparisonError::TooManyCopies { kind: tile.kind() });
            }
            *count -= 1;
        }
        Ok(Self { hand, unseen })
    }

    pub(crate) fn compare(
        &self,
        first: Tile,
        second: Tile,
        draw: Option<TileKind>,
    ) -> Result<DiscardComparison, ComparisonError> {
        if first == second {
            return Err(ComparisonError::SameDiscard);
        }
        if let Some(kind) = draw
            && self.unseen[kind.as_u8() as usize] == 0
        {
            return Err(ComparisonError::ExhaustedDraw { kind });
        }
        Ok(DiscardComparison {
            first: self.branch(first, draw)?,
            second: self.branch(second, draw)?,
            connections_before: connections(&self.hand),
        })
    }

    fn branch(
        &self,
        discard: Tile,
        draw: Option<TileKind>,
    ) -> Result<DiscardBranch, ComparisonError> {
        let efficiency = analyze_discard(&self.hand, discard, |kind| {
            self.unseen[kind.as_u8() as usize]
        })?;
        let mut hand = self.hand.clone();
        hand.discard(discard).map_err(AnalysisError::HandMutation)?;
        let concealed = hand.concealed().to_vec();
        let followup = match draw {
            None => None,
            Some(draw) => {
                // 切出的牌仍然可见；假设摸入的牌才会让不可见枚数减少一张。
                let mut unseen = self.unseen;
                unseen[draw.as_u8() as usize] -= 1;
                let tile = Tile::try_from(draw.as_u8())
                    .unwrap_or_else(|_| unreachable!("牌种必然是合法普通牌"));
                hand.draw(tile).map_err(AnalysisError::HandMutation)?;
                let completed_shape = hand_shanten(&hand) == -1;
                let mut discards = hand.concealed().to_vec();
                discards.dedup();
                let next_discards = discards
                    .into_iter()
                    .map(|tile| analyze_discard(&hand, tile, |kind| unseen[kind.as_u8() as usize]))
                    .collect::<Result<_, _>>()?;
                Some(DrawBranch {
                    draw,
                    completed_shape,
                    next_discards,
                })
            }
        };
        Ok(DiscardBranch {
            concealed,
            efficiency,
            followup,
        })
    }
}

fn connections(hand: &Hand) -> Vec<TileConnections> {
    let mut counts = [0; 34];
    for tile in hand.concealed() {
        counts[tile.kind().as_u8() as usize] += 1;
    }
    (0..34u8)
        .filter(|&value| counts[value as usize] > 0)
        .map(|value| {
            let kind =
                TileKind::try_from(value).unwrap_or_else(|_| unreachable!("0..34 是合法牌种"));
            let sequence_neighbors = (0..27u8)
                .filter(|&other| {
                    value < 27
                        && other / 9 == value / 9
                        && other != value
                        && other.abs_diff(value) <= 2
                        && counts[other as usize] > 0
                })
                .map(|other| {
                    TileKind::try_from(other).unwrap_or_else(|_| unreachable!("0..27 是合法牌种"))
                })
                .collect();
            TileConnections {
                kind,
                copies: counts[value as usize],
                sequence_neighbors,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests;
