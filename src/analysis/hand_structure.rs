//! 列出暗牌中实际存在的局部组合及用牌冲突；不强行选定整手拆分。

use super::shanten::{concealed_counts, tile_kind};
use crate::mahjong::{hand::Hand, tile::TileKind};

pub(crate) struct Structure {
    pub components: Vec<LocalComponent>,
    pub conflicts: Vec<ComponentConflict>,
    pub isolated: Vec<TileKind>,
}

pub(crate) struct LocalComponent {
    pub kind: &'static str,
    pub tiles: Vec<TileKind>,
}

pub(crate) struct ComponentConflict {
    pub first: usize,
    pub second: usize,
    pub insufficient_tiles: Vec<TileKind>,
}

pub(crate) fn analyze(hand: &Hand) -> Structure {
    let counts = concealed_counts(hand);
    let mut components = Vec::new();
    for (tile, &count) in counts.iter().enumerate() {
        if count >= 2 {
            components.push(LocalComponent {
                kind: "pair",
                tiles: vec![tile_kind(tile); 2],
            });
        }
        if count >= 3 {
            components.push(LocalComponent {
                kind: "triplet",
                tiles: vec![tile_kind(tile); 3],
            });
        }
        if tile >= 27 || count == 0 {
            continue;
        }
        if tile % 9 <= 6 && counts[tile + 1] > 0 && counts[tile + 2] > 0 {
            components.push(LocalComponent {
                kind: "sequence",
                tiles: (tile..tile + 3).map(tile_kind).collect(),
            });
        }
        for gap in 1..=2 {
            if tile % 9 + gap >= 9 || counts[tile + gap] == 0 {
                continue;
            }
            let kind = if gap == 2 {
                "kanchan"
            } else if tile % 9 == 0 || tile % 9 == 7 {
                "penchan"
            } else {
                "ryanmen"
            };
            components.push(LocalComponent {
                kind,
                tiles: vec![tile_kind(tile), tile_kind(tile + gap)],
            });
        }
    }
    let mut conflicts = Vec::new();
    for (first, a) in components.iter().enumerate() {
        for (second, b) in components.iter().enumerate().skip(first + 1) {
            let mut used = [0; 34];
            for tile in a.tiles.iter().chain(&b.tiles) {
                used[tile.as_u8() as usize] += 1;
            }
            let insufficient_tiles = (0..34)
                .filter(|&tile| used[tile] > counts[tile])
                .map(tile_kind)
                .collect::<Vec<_>>();
            if !insufficient_tiles.is_empty() {
                conflicts.push(ComponentConflict {
                    first,
                    second,
                    insufficient_tiles,
                });
            }
        }
    }
    let isolated = (0..34)
        .filter(|&tile| {
            counts[tile] > 0
                && !components
                    .iter()
                    .any(|c| c.tiles.contains(&tile_kind(tile)))
        })
        .map(tile_kind)
        .collect();
    Structure {
        components,
        conflicts,
        isolated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mahjong::tile::Tile;

    #[test]
    fn shared_shape_components_cannot_be_counted_as_independent_blocks() {
        let hand = Hand::new(
            [0, 1, 2, 3, 5, 8, 9, 12, 16, 18, 22, 27, 27]
                .map(|t| Tile::new(t).unwrap())
                .to_vec(),
            vec![],
        )
        .unwrap();
        let facts = analyze(&hand);
        let first = facts
            .components
            .iter()
            .position(|c| {
                c.kind == "sequence" && c.tiles == [tile_kind(0), tile_kind(1), tile_kind(2)]
            })
            .unwrap();
        let second = facts
            .components
            .iter()
            .position(|c| {
                c.kind == "sequence" && c.tiles == [tile_kind(1), tile_kind(2), tile_kind(3)]
            })
            .unwrap();
        assert!(facts.conflicts.iter().any(|c| c.first == first
            && c.second == second
            && c.insufficient_tiles == [tile_kind(1), tile_kind(2)]));
        assert!(!facts.isolated.contains(&tile_kind(5)));
        assert!(facts.isolated.contains(&tile_kind(8)));
        assert!(!facts.isolated.contains(&tile_kind(27)));
    }
}
