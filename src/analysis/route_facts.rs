//! 役种路线的距离和一次摸切事实，不估计成手概率或推荐动作。

use super::{
    AnalysisError, Yaku, YakuDistanceError,
    shanten::concealed_counts,
    tile_efficiency::EfficiencyCache,
    yaku::{available_yaku_shanten, yaku_shanten},
};
use crate::mahjong::{
    hand::Hand,
    tile::{Tile, TileKind},
};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RouteError {
    Hand(AnalysisError),
    Yaku(YakuDistanceError),
    InvalidAvailability { kind: TileKind },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RouteDistance {
    pub shape: Option<i8>,
    pub available: Option<i8>,
}

pub(crate) struct RouteFacts {
    pub distance: RouteDistance,
    pub progression: Option<Vec<RouteDraw>>,
}

pub(crate) struct RouteDraw {
    pub tile: TileKind,
    pub unseen: u8,
    pub completes_route_shape: bool,
    pub discards: Vec<RouteDiscard>,
}

pub(crate) struct RouteDiscard {
    pub tile: Tile,
    pub distance: i8,
    pub ordinary: super::DiscardEfficiency,
}

/// unseen 已排除自家暗牌、副露和所有其他可见牌；切出的牌不会回到未知池。
pub(crate) fn analyze(
    hand: &Hand,
    unseen: &[u8; 34],
    yaku: Yaku,
    expand: bool,
) -> Result<RouteFacts, RouteError> {
    if hand.effective_tile_count() != Hand::MIN_TILE_COUNT {
        return Err(RouteError::Hand(AnalysisError::InvalidHandSize {
            expected: Hand::MIN_TILE_COUNT,
            actual: hand.effective_tile_count(),
        }));
    }
    let counts = concealed_counts(hand);
    let mut held = counts;
    for tile in hand.melds().iter().flat_map(|meld| meld.tiles()) {
        held[tile.kind().as_u8() as usize] += 1;
    }
    let mut limits = [0; 34];
    for kind in 0..34 {
        if u16::from(held[kind]) + u16::from(unseen[kind]) > 4 {
            return Err(RouteError::InvalidAvailability {
                kind: super::shanten::tile_kind(kind),
            });
        }
        limits[kind] = counts[kind] + unseen[kind];
    }
    let distance = RouteDistance {
        shape: yaku_shanten(hand, yaku).map_err(RouteError::Yaku)?,
        available: available_yaku_shanten(hand, yaku, &limits).map_err(RouteError::Yaku)?,
    };
    let mut facts = RouteFacts {
        distance,
        progression: expand.then(Vec::new),
    };
    let Some(current) = distance.available.filter(|_| expand) else {
        return Ok(facts);
    };
    let mut cache = EfficiencyCache::default();
    let mut progression = Vec::new();
    for (kind, &copies) in unseen.iter().enumerate().filter(|(_, count)| **count > 0) {
        let kind = super::shanten::tile_kind(kind);
        let tile = Tile::new(kind.as_u8()).unwrap_or_else(|| unreachable!("合法普通牌种"));
        let mut drawn = hand.clone();
        drawn
            .draw(tile)
            .map_err(|e| RouteError::Hand(AnalysisError::HandMutation(e)))?;
        let Some(after_draw) =
            available_yaku_shanten(&drawn, yaku, &limits).map_err(RouteError::Yaku)?
        else {
            continue;
        };
        // 先检查摸牌能否推进目标，避免在无关摸牌上枚举所有切牌。
        if after_draw >= current {
            continue;
        }
        let mut discards = Vec::new();
        if after_draw != -1 {
            let mut choices = drawn.concealed().to_vec();
            choices.dedup();
            let mut next_unseen = *unseen;
            next_unseen[kind.as_u8() as usize] -= 1;
            for discard in choices {
                let mut after = drawn.clone();
                after
                    .discard(discard)
                    .map_err(|e| RouteError::Hand(AnalysisError::HandMutation(e)))?;
                let mut next_limits = limits;
                next_limits[discard.kind().as_u8() as usize] -= 1;
                let Some(next) =
                    available_yaku_shanten(&after, yaku, &next_limits).map_err(RouteError::Yaku)?
                else {
                    continue;
                };
                if next >= current {
                    continue;
                }
                discards.push(RouteDiscard {
                    tile: discard,
                    distance: next,
                    ordinary: cache
                        .discard(&drawn, discard, |kind| next_unseen[kind.as_u8() as usize])
                        .map_err(RouteError::Hand)?,
                });
            }
        }
        if after_draw == -1 || !discards.is_empty() {
            progression.push(RouteDraw {
                tile: kind,
                unseen: copies,
                completes_route_shape: after_draw == -1,
                discards,
            });
        }
    }
    facts.progression = Some(progression);
    Ok(facts)
}

#[cfg(test)]
mod tests;
