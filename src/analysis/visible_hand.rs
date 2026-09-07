//! 分支计算共同使用的可见牌校验，不读取牌山或对手暗牌。

use crate::mahjong::{
    hand::Hand,
    meld::Meld,
    tile::{Tile, TileKind},
};

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum VisibleHandError {
    InvalidMeld,
    TooManyCopies { kind: TileKind },
}

/// 额外可见牌不重复包含自家暗牌和副露；被鸣走的弃牌应只在副露中计数。
pub(crate) fn unseen_tiles(hand: &Hand, additional: &[Tile]) -> Result<[u8; 34], VisibleHandError> {
    for meld in hand.melds() {
        if !valid_meld(meld) {
            return Err(VisibleHandError::InvalidMeld);
        }
    }
    let mut unseen = [4; 34];
    for tile in hand
        .concealed()
        .iter()
        .chain(hand.melds().iter().flat_map(|m| m.tiles()))
        .chain(additional)
    {
        let count = &mut unseen[tile.kind().as_u8() as usize];
        if *count == 0 {
            return Err(VisibleHandError::TooManyCopies { kind: tile.kind() });
        }
        *count -= 1;
    }
    Ok(unseen)
}

pub(crate) fn valid_meld(meld: &Meld) -> bool {
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
    valid
        && meld
            .called()
            .is_none_or(|tile| meld.tiles().contains(&tile))
}
