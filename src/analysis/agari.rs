use crate::mahjong::{hand::Hand, meld::Meld, tile::TileKind};
use std::error::Error;
use std::fmt;

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

/// 和牌张补成的位置，普通型面子索引对应原拆分的 `groups`。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WinningPosition {
    /// 普通型的雀头。
    Pair,
    /// 普通型的一个暗牌顺子或刻子，不包含副露和杠。
    Group(usize),
    /// 七对子中由和牌张补成的对子。
    Chiitoitsu,
    /// 国士中的和牌张；结合重复牌与和牌张可区分单面和十三面。
    Kokushi,
}

/// 一个完整拆分及和牌张对它的具体完成方式。
///
/// 只能通过 [`interpretations`] 生成，保证和牌张与归属匹配。
/// 借用原牌型，保留面子索引；即使役种相同，不同归属也仍是不同解释。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AgariInterpretation<'a> {
    pattern: &'a AgariPattern,
    winning_tile: TileKind,
    winning_position: WinningPosition,
}

impl<'a> AgariInterpretation<'a> {
    /// 返回本解释对应的原始拆分。
    pub const fn pattern(&self) -> &'a AgariPattern {
        self.pattern
    }

    /// 返回生成本解释时使用的和牌张。
    pub const fn winning_tile(&self) -> TileKind {
        self.winning_tile
    }

    /// 返回和牌张补成的位置。
    pub const fn winning_position(&self) -> WinningPosition {
        self.winning_position
    }
}

/// 无法为指定牌型枚举和牌解释的原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgariError {
    /// 和牌张不能补成当前拆分中的任何合法位置。
    WinningTileMismatch { winning_tile: TileKind },
}

impl fmt::Display for AgariError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WinningTileMismatch { winning_tile } => write!(
                formatter,
                "winning tile {} cannot complete this agari pattern",
                winning_tile.as_u8()
            ),
        }
    }
}

impl Error for AgariError {}

/// 枚举和牌张完成一个合法、完整拆分的所有方式。
///
/// 输入应来自 [`patterns`] 或满足同样的牌型约束，不在这里重新校验牌型。
/// 普通型依次检查雀头和面子，副露面子与所有杠均不能被和牌张补成。
/// 不同面子索引分别保留，包括内容相同的顺子；本函数不判断役种或符数。
/// 没有合法归属时返回错误。
pub fn interpretations(
    pattern: &AgariPattern,
    winning_tile: TileKind,
) -> Result<Vec<AgariInterpretation<'_>>, AgariError> {
    let mut result = Vec::new();
    let mut push = |winning_position| {
        result.push(AgariInterpretation {
            pattern,
            winning_tile,
            winning_position,
        });
    };
    match pattern {
        AgariPattern::Standard { groups, pair } => {
            if *pair == winning_tile {
                push(WinningPosition::Pair);
            }
            for (index, group) in groups.iter().enumerate() {
                match group {
                    AgariGroup::Sequence { start, open: false }
                        if (start.as_u8()..=start.as_u8() + 2).contains(&winning_tile.as_u8()) =>
                    {
                        push(WinningPosition::Group(index));
                    }
                    AgariGroup::Triplet { tile, open: false } if *tile == winning_tile => {
                        push(WinningPosition::Group(index));
                    }
                    _ => {}
                }
            }
        }
        AgariPattern::Chiitoitsu { pairs } if pairs.contains(&winning_tile) => {
            push(WinningPosition::Chiitoitsu);
        }
        AgariPattern::Kokushi { .. } => {
            let tile = winning_tile.as_u8();
            if tile >= 27 || matches!(tile % 9, 0 | 8) {
                push(WinningPosition::Kokushi);
            }
        }
        _ => {}
    }
    if result.is_empty() {
        Err(AgariError::WinningTileMismatch { winning_tile })
    } else {
        Ok(result)
    }
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
    if let Some(pair) = p {
        if g.len() == 4 && c.iter().all(|&n| n == 0) {
            let x = AgariPattern::Standard {
                groups: g.clone(),
                pair,
            };
            if !out.contains(&x) {
                out.push(x);
            }
        }
        if g.len() == 4 {
            return;
        }
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
    // 四面子齐全时仍需允许剩余两张组成雀头，但不能再添加面子。
    if g.len() == 4 {
        return;
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
