use crate::analysis::agari::{AgariGroup, AgariInterpretation, AgariPattern, WinningPosition};
use crate::analysis::yaku::types::{Yaku, YakuDetectionError};
use crate::analysis::{AgariContext, RiichiStatus, RonSource, TsumoSource, WinMethod};
use crate::mahjong::round::Wind;
use crate::mahjong::tile::TileKind;
use std::collections::HashMap;
use std::ops::RangeInclusive;

// 先判断与和牌张位置无关的役，再补充当前解释下的等待及暗刻相关役。
fn detect_shape_yaku(pattern: &AgariPattern) -> Vec<Yaku> {
    match pattern {
        AgariPattern::Kokushi { .. } => vec![Yaku::Kokushi],
        AgariPattern::Chiitoitsu { pairs } => {
            let properties = TileProperties::from_kinds(pairs.iter().map(|tile| tile.as_u8()));
            let mut result = properties.detect();
            result.push(Yaku::Chiitoitsu);
            result
        }
        AgariPattern::Standard { groups, pair } => {
            let properties = TileProperties::from_kinds(
                groups
                    .iter()
                    .flat_map(group_tile_kinds)
                    .chain(std::iter::once(pair.as_u8())),
            );
            let mut result = properties.detect();
            let sequences = groups.iter().filter_map(sequence_key).collect::<Vec<_>>();
            let closed = groups.iter().all(|group| !group_open(group));
            let mut counts = HashMap::<_, usize>::new();
            for key in &sequences {
                *counts.entry(*key).or_default() += 1;
            }
            if closed {
                // 四组相同顺子也能组成两组一杯口；同一顺子不能重复参与配对。
                let sequence_pairs: usize = counts.values().map(|count| count / 2).sum();
                if sequence_pairs == 2 {
                    result.push(Yaku::Ryanpeikou);
                } else if sequence_pairs == 1 {
                    result.push(Yaku::Iipeikou);
                }
            }
            if sequences.is_empty() {
                result.push(Yaku::Toitoi);
            }
            if sequences
                .iter()
                .any(|&(_, start)| (0..3).all(|suit| sequences.contains(&(suit, start))))
            {
                result.push(Yaku::SanshokuDoujun);
            }
            if (0..3).any(|suit| {
                [0, 3, 6]
                    .iter()
                    .all(|start| sequences.contains(&(suit, *start)))
            }) {
                result.push(Yaku::Ittsu);
            }
            if (0..9).any(|rank| {
                (0..3).all(|suit| {
                    groups
                        .iter()
                        .any(|group| triplet_key(group) == Some(suit * 9 + rank))
                })
            }) {
                result.push(Yaku::SanshokuDoukou);
            }
            let winds = groups
                .iter()
                .filter_map(triplet_key)
                .filter(|tile| (27..31).contains(tile))
                .count();
            if winds == 4 {
                result.push(Yaku::Daisuushi);
            } else if winds == 3 && (27..31).contains(&pair.as_u8()) {
                result.push(Yaku::Shousuushi);
            }
            match groups
                .iter()
                .filter(|group| matches!(group, AgariGroup::Kan { .. }))
                .count()
            {
                3 => result.push(Yaku::Sankantsu),
                4 => result.push(Yaku::Suukantsu),
                _ => {}
            }
            let dragons = groups
                .iter()
                .filter_map(triplet_key)
                .filter(|&tile| tile >= 31)
                .count();
            if dragons == 3 {
                result.push(Yaku::Daisangen);
            } else if dragons == 2 && pair.as_u8() >= 31 {
                result.push(Yaku::Shousangen);
            }
            if !sequences.is_empty()
                && is_terminal_or_honor(pair.as_u8())
                && groups
                    .iter()
                    .all(|group| group_tile_kinds(group).any(is_terminal_or_honor))
            {
                result.push(if properties.honors {
                    Yaku::Chanta
                } else {
                    Yaku::Junchan
                });
            }
            result
        }
    }
}

/// 结合和牌上下文，判断单个完整和牌解释下成立的役。
///
/// 支持 `Yaku` 中除流局满贯外的和牌役，包括等待相关的役满形。
/// 校验可直接识别的上下文冲突，但事件历史由调用方保证，不检查振听或完整和牌合法性。
/// 当前不支持抢暗杠；不引入规则开关，双倍役满形只识别形态。
///
/// 解释由 `agari::interpretations` 生成；上下文和牌张必须与解释一致。
/// 返回值只对应传入的解释，无役时返回空列表；不枚举或合并其他解释。
/// 调用方应保留解释与结果的对应关系，不能因役种相同而丢弃解释。
/// 四暗刻单骑覆盖四暗刻，四暗刻覆盖三暗刻；不计算番数、符数或点数。
pub fn detect_yaku(
    interpretation: &AgariInterpretation<'_>,
    context: &AgariContext,
) -> Result<Vec<Yaku>, YakuDetectionError> {
    if context.winning_tile != interpretation.winning_tile() {
        return Err(YakuDetectionError::ContextWinningTileMismatch {
            interpretation_tile: interpretation.winning_tile(),
            context_tile: context.winning_tile,
        });
    }
    let pattern = interpretation.pattern();
    validate_context(pattern, context)?;
    let mut yaku = detect_shape_yaku(pattern);
    detect_event_yaku(pattern, context, &mut yaku);
    if let AgariPattern::Kokushi { pair } = pattern {
        if *pair == interpretation.winning_tile() {
            yaku.retain(|&yaku| yaku != Yaku::Kokushi);
            yaku.push(Yaku::KokushiJuusanmen);
        }
        return Ok(yaku);
    }
    let AgariPattern::Standard { groups, pair } = pattern else {
        return Ok(yaku);
    };
    for tile in groups.iter().filter_map(triplet_key) {
        match tile {
            31 => yaku.push(Yaku::Haku),
            32 => yaku.push(Yaku::Hatsu),
            33 => yaku.push(Yaku::Chun),
            _ => {}
        }
        if tile == wind_tile(context.round_wind) {
            yaku.push(Yaku::Bakaze);
        }
        if tile == wind_tile(context.seat_wind) {
            yaku.push(Yaku::Jikaze);
        }
    }
    if let Some(nine_gates) = nine_gates(groups, *pair, interpretation.winning_tile()) {
        yaku.push(nine_gates);
    }
    let position = interpretation.winning_position();
    let winning_group = match position {
        // 解释只能由枚举函数生成，借用的牌型不能在解释存活期间被修改。
        WinningPosition::Group(index) => Some(&groups[index]),
        _ => None,
    };
    let pinfu_shape = groups
        .iter()
        .all(|group| matches!(group, AgariGroup::Sequence { open: false, .. }))
        && pair.as_u8() < 31
        && pair.as_u8() != wind_tile(context.round_wind)
        && pair.as_u8() != wind_tile(context.seat_wind);
    let concealed_triplets = groups
        .iter()
        .filter(|group| {
            matches!(
                group,
                AgariGroup::Triplet { open: false, .. } | AgariGroup::Kan { open: false, .. }
            )
        })
        .count();
    if pinfu_shape
        && let Some(AgariGroup::Sequence { start, .. }) = winning_group
        && is_ryanmen(*start, interpretation.winning_tile())
    {
        yaku.push(Yaku::Pinfu);
    }
    // 荣和只影响被补成的刻子的暗刻计数，不改变原牌型的门前状态。
    let ron_triplet = matches!(context.win_method, WinMethod::Ron(_))
        && matches!(winning_group, Some(AgariGroup::Triplet { .. }));
    let concealed_triplets = concealed_triplets - usize::from(ron_triplet);
    if concealed_triplets == 4 {
        yaku.push(if position == WinningPosition::Pair {
            Yaku::SuuankouTanki
        } else {
            Yaku::Suuankou
        });
    } else if concealed_triplets == 3 {
        yaku.push(Yaku::Sanankou);
    }
    Ok(yaku)
}

fn is_ryanmen(start: TileKind, winning_tile: TileKind) -> bool {
    let start = start.as_u8();
    let start_rank = start % 9;
    let winning_tile = winning_tile.as_u8();
    // 123 和 789 的边张完成不能当作两面。
    (winning_tile == start && start_rank < 6) || (winning_tile == start + 2 && start_rank > 0)
}

fn wind_tile(wind: Wind) -> u8 {
    match wind {
        Wind::East => 27,
        Wind::South => 28,
        Wind::West => 29,
        Wind::North => 30,
    }
}

fn is_closed(pattern: &AgariPattern) -> bool {
    match pattern {
        AgariPattern::Standard { groups, .. } => groups.iter().all(|group| !group_open(group)),
        _ => true,
    }
}

fn validate_context(
    pattern: &AgariPattern,
    context: &AgariContext,
) -> Result<(), YakuDetectionError> {
    let has_kan = matches!(pattern, AgariPattern::Standard { groups, .. }
        if groups.iter().any(|group| matches!(group, AgariGroup::Kan { .. })));
    if context.riichi != RiichiStatus::None && !is_closed(pattern) {
        return Err(YakuDetectionError::RiichiRequiresClosedHand);
    }
    match context.win_method {
        WinMethod::Tsumo(TsumoSource::FirstDraw) => {
            if !is_closed(pattern) || has_kan {
                return Err(YakuDetectionError::FirstDrawRequiresInitialHand);
            }
            if context.riichi != RiichiStatus::None {
                return Err(YakuDetectionError::FirstDrawWithRiichi);
            }
        }
        WinMethod::Tsumo(TsumoSource::Rinshan) => {
            if !has_kan {
                return Err(YakuDetectionError::RinshanRequiresKan);
            }
            if matches!(
                context.riichi,
                RiichiStatus::Riichi { ippatsu: true }
                    | RiichiStatus::DoubleRiichi { ippatsu: true }
            ) {
                return Err(YakuDetectionError::IppatsuWithRinshan);
            }
        }
        WinMethod::Ron(RonSource::Ankan) => {
            return Err(YakuDetectionError::RobbingAnkanUnsupported);
        }
        WinMethod::Ron(RonSource::Kakan) => {
            // 他家碰子已占三张，和牌者不可能另有同种牌；抢入的只能是第四张。
            let tile = context.winning_tile;
            let already_held = match pattern {
                AgariPattern::Standard { groups, pair } => {
                    let count: usize = groups
                        .iter()
                        .map(|group| match group {
                            AgariGroup::Sequence { .. } => {
                                usize::from(group_tile_kinds(group).contains(&tile.as_u8()))
                            }
                            AgariGroup::Triplet {
                                tile: group_tile, ..
                            } if *group_tile == tile => 3,
                            AgariGroup::Kan {
                                tile: group_tile, ..
                            } if *group_tile == tile => 4,
                            _ => 0,
                        })
                        .sum();
                    count + 2 * usize::from(*pair == tile) > 1
                }
                AgariPattern::Chiitoitsu { .. } => true,
                AgariPattern::Kokushi { pair } => *pair == tile,
            };
            if already_held {
                return Err(YakuDetectionError::RobbedKanTileAlreadyHeld { winning_tile: tile });
            }
        }
        _ => {}
    }
    Ok(())
}

fn detect_event_yaku(pattern: &AgariPattern, context: &AgariContext, yaku: &mut Vec<Yaku>) {
    let ippatsu = match context.riichi {
        RiichiStatus::None => false,
        RiichiStatus::Riichi { ippatsu } => {
            yaku.push(Yaku::Riichi);
            ippatsu
        }
        RiichiStatus::DoubleRiichi { ippatsu } => {
            yaku.push(Yaku::DoubleRiichi);
            ippatsu
        }
    };
    if ippatsu {
        yaku.push(Yaku::Ippatsu);
    }
    if matches!(context.win_method, WinMethod::Tsumo(_)) && is_closed(pattern) {
        yaku.push(Yaku::MenzenTsumo);
    }
    match context.win_method {
        WinMethod::Tsumo(TsumoSource::LastWall) => yaku.push(Yaku::Haitei),
        WinMethod::Tsumo(TsumoSource::Rinshan) => yaku.push(Yaku::RinshanKaihou),
        WinMethod::Tsumo(TsumoSource::FirstDraw) => yaku.push(if context.seat_wind == Wind::East {
            Yaku::Tenhou
        } else {
            Yaku::Chiihou
        }),
        WinMethod::Ron(RonSource::LastDiscard) => yaku.push(Yaku::Houtei),
        WinMethod::Ron(RonSource::Kakan) => yaku.push(Yaku::Chankan),
        _ => {}
    }
}

fn nine_gates(groups: &[AgariGroup], pair: TileKind, winning_tile: TileKind) -> Option<Yaku> {
    if pair.as_u8() >= 27 {
        return None;
    }
    let suit = pair.as_u8() / 9;
    let mut counts = [0u8; 9];
    counts[usize::from(pair.as_u8() % 9)] = 2;
    for group in groups {
        if group_open(group) || matches!(group, AgariGroup::Kan { .. }) {
            return None;
        }
        for tile in group_tile_kinds(group) {
            if tile / 9 != suit {
                return None;
            }
            counts[usize::from(tile % 9)] += if matches!(group, AgariGroup::Triplet { .. }) {
                3
            } else {
                1
            };
        }
    }
    let base = [3, 1, 1, 1, 1, 1, 1, 1, 3];
    if counts
        .iter()
        .zip(base)
        .any(|(&count, required)| count < required)
    {
        return None;
    }
    // 移除和牌张后恰为 1112345678999，才是纯正九莲。
    counts[usize::from(winning_tile.as_u8() % 9)] -= 1;
    Some(if counts == base {
        Yaku::JunseiChuurenPoutou
    } else {
        Yaku::ChuurenPoutou
    })
}

// 整手牌性质与牌的重复次数无关，普通型和七对子共用同一套判断。
struct TileProperties {
    honors: bool,
    terminals: bool,
    simples: bool,
    suits: [bool; 3],
    green: bool,
}

impl TileProperties {
    fn from_kinds(kinds: impl Iterator<Item = u8>) -> Self {
        let mut properties = Self {
            honors: false,
            terminals: false,
            simples: false,
            suits: [false; 3],
            green: true,
        };
        for tile in kinds {
            properties.green &= matches!(tile, 19 | 20 | 21 | 23 | 25 | 32);
            if is_honor(tile) {
                properties.honors = true;
            } else {
                properties.suits[usize::from(tile / 9)] = true;
                if is_terminal(tile) {
                    properties.terminals = true;
                } else {
                    properties.simples = true;
                }
            }
        }
        properties
    }

    fn detect(&self) -> Vec<Yaku> {
        let mut result = Vec::new();
        if self.green {
            result.push(Yaku::Ryuuiisou);
        }
        if self.simples && !self.honors && !self.terminals {
            result.push(Yaku::Tanyao);
        }
        if !self.simples {
            if self.terminals {
                result.push(if self.honors {
                    Yaku::Honroutou
                } else {
                    Yaku::Chinroutou
                });
            } else if self.honors {
                result.push(Yaku::Tsuuiisou);
            }
        }
        if self.suits.iter().filter(|&&present| present).count() == 1 {
            result.push(if self.honors {
                Yaku::Honitsu
            } else {
                Yaku::Chinitsu
            });
        }
        result
    }
}

fn is_honor(tile: u8) -> bool {
    tile >= 27
}

fn is_terminal(tile: u8) -> bool {
    !is_honor(tile) && matches!(tile % 9, 0 | 8)
}

fn is_terminal_or_honor(tile: u8) -> bool {
    is_honor(tile) || is_terminal(tile)
}

// 只枚举组内不同的牌种，不表示实际张数；刻子和杠子都只有一种牌。
fn group_tile_kinds(group: &AgariGroup) -> RangeInclusive<u8> {
    match group {
        AgariGroup::Sequence { start, .. } => start.as_u8()..=start.as_u8() + 2,
        AgariGroup::Triplet { tile, .. } | AgariGroup::Kan { tile, .. } => {
            tile.as_u8()..=tile.as_u8()
        }
    }
}

fn group_open(group: &AgariGroup) -> bool {
    match group {
        AgariGroup::Sequence { open, .. }
        | AgariGroup::Triplet { open, .. }
        | AgariGroup::Kan { open, .. } => *open,
    }
}

fn sequence_key(group: &AgariGroup) -> Option<(u8, u8)> {
    match group {
        AgariGroup::Sequence { start, .. } => Some((start.as_u8() / 9, start.as_u8() % 9)),
        _ => None,
    }
}

fn triplet_key(group: &AgariGroup) -> Option<u8> {
    match group {
        AgariGroup::Triplet { tile, .. } | AgariGroup::Kan { tile, .. } => Some(tile.as_u8()),
        _ => None,
    }
}
