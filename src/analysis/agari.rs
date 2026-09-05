use crate::mahjong::{hand::Hand, meld::Meld, tile::TileKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AgariGroup {
    Sequence { start: TileKind, open: bool },
    Triplet { tile: TileKind, open: bool },
    Kan { tile: TileKind, open: bool },
}
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum AgariPattern {
    Standard {
        groups: Vec<AgariGroup>,
        pair: TileKind,
    },
    Chiitoitsu {
        pairs: Vec<TileKind>,
    },
    Kokushi {
        pair: TileKind,
    },
}

pub fn patterns(hand: &Hand) -> Vec<AgariPattern> {
    let mut counts = [0u8; 34];
    for tile in hand.concealed() {
        counts[usize::from(tile.kind().as_u8())] += 1;
    }
    let mut groups = hand.melds().iter().map(group_from_meld).collect::<Vec<_>>();
    let mut result = Vec::new();
    enumerate(&mut counts, &mut groups, None, &mut result);
    if hand.melds().is_empty() && hand.concealed().len() == 14 {
        let pairs = (0..34)
            .filter(|&kind| counts[kind] == 2)
            .map(|kind| TileKind::new(kind as u8).unwrap())
            .collect::<Vec<_>>();
        if pairs.len() == 7 {
            result.push(AgariPattern::Chiitoitsu { pairs });
        }
        let terminals = [0, 8, 9, 17, 18, 26, 27, 28, 29, 30, 31, 32, 33];
        if terminals.iter().all(|&kind| counts[kind] >= 1) {
            let pairs = terminals
                .iter()
                .filter(|&&kind| counts[kind] >= 2)
                .collect::<Vec<_>>();
            if pairs.len() == 1 {
                result.push(AgariPattern::Kokushi {
                    pair: TileKind::new(*pairs[0] as u8).unwrap(),
                });
            }
        }
    }
    result
}
fn enumerate(
    c: &mut [u8; 34],
    g: &mut Vec<AgariGroup>,
    p: Option<TileKind>,
    out: &mut Vec<AgariPattern>,
) {
    if g.len() == 4 {
        if let Some(pair) = p
            && c.iter().all(|&n| n == 0)
        {
            let x = AgariPattern::Standard {
                groups: g.clone(),
                pair,
            };
            if !out.contains(&x) {
                out.push(x);
            }
        }
        return;
    }
    let Some(i) = c.iter().position(|&n| n > 0) else {
        return;
    };
    let kind = TileKind::new(i as u8).unwrap();
    if p.is_none() && c[i] >= 2 {
        c[i] -= 2;
        enumerate(c, g, Some(kind), out);
        c[i] += 2;
    }
    if c[i] >= 3 {
        c[i] -= 3;
        g.push(AgariGroup::Triplet {
            tile: kind,
            open: false,
        });
        enumerate(c, g, p, out);
        g.pop();
        c[i] += 3;
    }
    if i < 27 && i % 9 <= 6 && c[i + 1] > 0 && c[i + 2] > 0 {
        c[i] -= 1;
        c[i + 1] -= 1;
        c[i + 2] -= 1;
        g.push(AgariGroup::Sequence {
            start: kind,
            open: false,
        });
        enumerate(c, g, p, out);
        g.pop();
        c[i] += 1;
        c[i + 1] += 1;
        c[i + 2] += 1;
    }
}
fn group_from_meld(m: &Meld) -> AgariGroup {
    match m {
        Meld::Chi { tiles, .. } => AgariGroup::Sequence {
            start: tiles.iter().map(|t| t.kind()).min().unwrap(),
            open: true,
        },
        Meld::Ankan { tiles } => AgariGroup::Kan {
            tile: tiles[0].kind(),
            open: false,
        },
        m if m.is_kan() => AgariGroup::Kan {
            tile: m.tiles()[0].kind(),
            open: true,
        },
        _ => AgariGroup::Triplet {
            tile: m.tiles()[0].kind(),
            open: true,
        },
    }
}
