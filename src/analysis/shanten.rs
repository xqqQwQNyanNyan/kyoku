use crate::mahjong::hand::Hand;
use crate::mahjong::meld::Meld;
use crate::mahjong::tile::TileKind;

const TILE_KIND_COUNT: usize = 34;
pub(super) const SUIT_TILE_KIND_COUNT: usize = 9;
pub(super) const MAX_COPIES: usize = 4;
const MAX_MELDS: usize = 4;
pub(super) const SEQUENCE_COMPONENT_COUNT: usize = 3 * 7;
const TRIPLET_COMPONENT_OFFSET: usize = SEQUENCE_COMPONENT_COUNT;
const PAIR_COMPONENT_OFFSET: usize = TRIPLET_COMPONENT_OFFSET + TILE_KIND_COUNT;
pub(super) const COMPONENT_COUNT: usize = PAIR_COMPONENT_OFFSET + TILE_KIND_COUNT;
const UNREACHABLE: u8 = u8::MAX;
const KOKUSHI_TILES: [usize; 13] = [0, 8, 9, 17, 18, 26, 27, 28, 29, 30, 31, 32, 33];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Suit {
    Manzu,
    Pinzu,
    Souzu,
}

/// 普通型目标牌形中已经选择的一个组成部分。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Component {
    Sequence { suit: Suit, start: u8 },
    Triplet(TileKind),
    Pair(TileKind),
}

type ConstraintState = usize;
type TileMask = u64;

/// 当前普通型约束编译后的转移表。
pub(super) struct CompiledConstraint {
    pub(super) start_state: ConstraintState,
    pub(super) transitions: Vec<[Option<ConstraintState>; COMPONENT_COUNT]>,
    pub(super) accepting: Vec<bool>,
}

pub(super) struct ChiitoitsuConstraint {
    pub(super) allowed_tiles: TileMask,
    /// 每组至少要有一种牌被选作对子；用于表达混一色等严格语义。
    pub(super) required_groups: Vec<TileMask>,
}

pub(super) struct KokushiConstraint;

pub(super) enum HandFormSpec {
    Ordinary(CompiledConstraint),
    Chiitoitsu(ChiitoitsuConstraint),
    Kokushi(KokushiConstraint),
}

impl Component {
    pub(super) fn index(self) -> usize {
        match self {
            Self::Sequence { suit, start } => {
                debug_assert!(start <= 6);
                suit as usize * 7 + start as usize
            }
            Self::Triplet(tile) => TRIPLET_COMPONENT_OFFSET + tile.as_u8() as usize,
            Self::Pair(tile) => PAIR_COMPONENT_OFFSET + tile.as_u8() as usize,
        }
    }
}

impl CompiledConstraint {
    fn state_count(&self) -> usize {
        self.transitions.len()
    }

    pub(super) fn transition(
        &self,
        state: ConstraintState,
        component: Component,
    ) -> Option<ConstraintState> {
        self.transitions[state][component.index()]
    }

    pub(super) fn is_accepting(&self, state: ConstraintState) -> bool {
        self.accepting[state]
    }
}

/// 计算普通型“四面子一雀头”的向听数。
///
/// `counts` 保存 34 种牌各自的数量。赤五必须事先折叠到对应的普通五中，
/// 每种牌的数量必须在 `0..=4` 范围内。
///
/// 本函数仅处理无副露手牌的普通型，不处理七对子和国士无双。
pub fn standard_shanten(counts: &[u8; TILE_KIND_COUNT]) -> i8 {
    debug_assert!(counts.iter().all(|&count| count <= MAX_COPIES as u8));

    ordinary_shanten_with_constraint(counts, &[], &standard_constraint())
        .unwrap_or_else(|| unreachable!("standard constraint always accepts ordinary hands"))
}

/// 使用约束状态机计算普通型向听数。
fn ordinary_shanten_with_constraint(
    counts: &[u8; TILE_KIND_COUNT],
    existing_melds: &[Meld],
    constraint: &CompiledConstraint,
) -> Option<i8> {
    let constraint_state_count = constraint.state_count();
    if constraint_state_count == 0 || existing_melds.len() > MAX_MELDS {
        return None;
    }
    debug_assert_eq!(constraint.accepting.len(), constraint_state_count);
    debug_assert!(constraint.start_state < constraint_state_count);

    let mut initial_constraint_state = constraint.start_state;
    for meld in existing_melds {
        initial_constraint_state =
            constraint.transition(initial_constraint_state, component_from_meld(meld))?;
    }

    // dp[i][a][b][p][m][c]：处理完前 i 种牌后，在此前顺子仍需要当前牌 a 张、
    // 下一种牌 b 张，且已选择 p 个雀头、m 个面子、约束状态为 c 时的最小缺牌数。
    let mut dp = vec![
        UNREACHABLE;
        (TILE_KIND_COUNT + 1)
            * (MAX_COPIES + 1)
            * (MAX_COPIES + 1)
            * 2
            * (MAX_MELDS + 1)
            * constraint_state_count
    ];
    let initial_index = ordinary_dp_index(
        0,
        0,
        0,
        0,
        existing_melds.len(),
        initial_constraint_state,
        constraint_state_count,
    );
    dp[initial_index] = 0;

    for (tile, &tile_count) in counts.iter().enumerate() {
        for a in 0..=MAX_COPIES {
            for b in 0..=MAX_COPIES {
                for pairs in 0..=1 {
                    for melds in 0..=MAX_MELDS {
                        for constraint_state in 0..constraint_state_count {
                            let current_index = ordinary_dp_index(
                                tile,
                                a,
                                b,
                                pairs,
                                melds,
                                constraint_state,
                                constraint_state_count,
                            );
                            let missing_so_far = dp[current_index];
                            if missing_so_far == UNREACHABLE {
                                continue;
                            }

                            for pair in 0..=1 {
                                if pairs + pair > 1 {
                                    continue;
                                }

                                for triplet in 0..=1 {
                                    for sequence in 0..=max_sequences_starting_at(tile) {
                                        let next_melds = melds + triplet + sequence;
                                        let required = a + 2 * pair + 3 * triplet + sequence;

                                        if next_melds > MAX_MELDS
                                            || required > MAX_COPIES
                                            || b + sequence > MAX_COPIES
                                        {
                                            continue;
                                        }

                                        let Some(next_constraint_state) = transition_components(
                                            constraint,
                                            constraint_state,
                                            tile,
                                            pair,
                                            triplet,
                                            sequence,
                                        ) else {
                                            continue;
                                        };

                                        let missing_here =
                                            required.saturating_sub(tile_count as usize) as u8;
                                        let candidate = missing_so_far + missing_here;
                                        let next_index = ordinary_dp_index(
                                            tile + 1,
                                            b + sequence,
                                            sequence,
                                            pairs + pair,
                                            next_melds,
                                            next_constraint_state,
                                            constraint_state_count,
                                        );
                                        dp[next_index] = dp[next_index].min(candidate);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    (0..constraint_state_count)
        .filter(|&state| constraint.is_accepting(state))
        .map(|state| {
            dp[ordinary_dp_index(
                TILE_KIND_COUNT,
                0,
                0,
                1,
                MAX_MELDS,
                state,
                constraint_state_count,
            )]
        })
        .filter(|&missing| missing != UNREACHABLE)
        .min()
        .map(|missing| missing as i8 - 1)
}

/// 计算无副露手牌的七对子向听数。
///
/// 四张相同的牌仍然只算一种对子，因为七对子必须由七种不同牌组成。
pub fn chiitoitsu_shanten(counts: &[u8; TILE_KIND_COUNT]) -> i8 {
    debug_assert!(counts.iter().all(|&count| count <= MAX_COPIES as u8));

    chiitoitsu_shanten_with_constraint(counts, &unrestricted_chiitoitsu_constraint())
}

/// 计算无副露手牌的国士无双向听数。
pub fn kokushi_shanten(counts: &[u8; TILE_KIND_COUNT]) -> i8 {
    debug_assert!(counts.iter().all(|&count| count <= MAX_COPIES as u8));

    let unique = KOKUSHI_TILES
        .iter()
        .filter(|&&tile| counts[tile] > 0)
        .count();
    let has_pair = KOKUSHI_TILES.iter().any(|&tile| counts[tile] >= 2);

    13 - unique as i8 - i8::from(has_pair)
}

/// 返回无副露手牌在普通型、七对子和国士无双中的最小向听数。
pub fn shanten(counts: &[u8; TILE_KIND_COUNT]) -> i8 {
    standard_shanten(counts)
        .min(chiitoitsu_shanten(counts))
        .min(kokushi_shanten(counts))
}

pub(super) fn concealed_counts(hand: &Hand) -> [u8; TILE_KIND_COUNT] {
    let mut counts = [0; TILE_KIND_COUNT];
    for tile in hand.concealed() {
        counts[tile.kind().as_u8() as usize] += 1;
    }
    counts
}

pub(super) fn solve_hand_form(
    hand: &Hand,
    counts: &[u8; TILE_KIND_COUNT],
    spec: &HandFormSpec,
) -> Option<i8> {
    match spec {
        HandFormSpec::Ordinary(constraint) => {
            ordinary_shanten_with_constraint(counts, hand.melds(), constraint)
        }
        HandFormSpec::Chiitoitsu(constraint) => hand
            .melds()
            .is_empty()
            .then(|| chiitoitsu_shanten_with_constraint(counts, constraint)),
        HandFormSpec::Kokushi(_) => hand.melds().is_empty().then(|| kokushi_shanten(counts)),
    }
}

fn standard_constraint() -> CompiledConstraint {
    CompiledConstraint {
        start_state: 0,
        transitions: vec![[Some(0); COMPONENT_COUNT]],
        accepting: vec![true],
    }
}

pub(super) fn unrestricted_chiitoitsu_constraint() -> ChiitoitsuConstraint {
    ChiitoitsuConstraint {
        allowed_tiles: (1u64 << TILE_KIND_COUNT) - 1,
        required_groups: Vec::new(),
    }
}

fn chiitoitsu_shanten_with_constraint(
    counts: &[u8; TILE_KIND_COUNT],
    constraint: &ChiitoitsuConstraint,
) -> i8 {
    let required_state_count = 1usize << constraint.required_groups.len();
    let accepting_state = required_state_count - 1;
    let mut dp = vec![vec![UNREACHABLE; required_state_count]; 8];
    dp[0][0] = 0;

    for (tile, &count) in counts.iter().enumerate() {
        if (constraint.allowed_tiles >> tile) & 1 == 0 {
            continue;
        }

        let mut group_bits = 0;
        for (group, &mask) in constraint.required_groups.iter().enumerate() {
            if (mask >> tile) & 1 != 0 {
                group_bits |= 1 << group;
            }
        }

        for selected in (0..7).rev() {
            for state in 0..required_state_count {
                let missing = dp[selected][state];
                if missing == UNREACHABLE {
                    continue;
                }

                let next_state = state | group_bits;
                let missing_pair = 2u8.saturating_sub(count);
                dp[selected + 1][next_state] =
                    dp[selected + 1][next_state].min(missing + missing_pair);
            }
        }
    }

    let missing = dp[7][accepting_state];
    if missing == UNREACHABLE {
        unreachable!("compiled chiitoitsu constraint must be satisfiable");
    }
    missing as i8 - 1
}

fn ordinary_dp_index(
    tile: usize,
    a: usize,
    b: usize,
    pairs: usize,
    melds: usize,
    constraint_state: ConstraintState,
    constraint_state_count: usize,
) -> usize {
    (((((tile * (MAX_COPIES + 1) + a) * (MAX_COPIES + 1) + b) * 2 + pairs) * (MAX_MELDS + 1)
        + melds)
        * constraint_state_count)
        + constraint_state
}

fn transition_components(
    constraint: &CompiledConstraint,
    mut state: ConstraintState,
    tile: usize,
    pair: usize,
    triplet: usize,
    sequence: usize,
) -> Option<ConstraintState> {
    let tile_kind = tile_kind(tile);

    if pair == 1 {
        state = constraint.transition(state, Component::Pair(tile_kind))?;
    }
    if triplet == 1 {
        state = constraint.transition(state, Component::Triplet(tile_kind))?;
    }
    for _ in 0..sequence {
        state = constraint.transition(state, sequence_component(tile))?;
    }

    Some(state)
}

fn component_from_meld(meld: &Meld) -> Component {
    match meld {
        Meld::Chi { tiles, .. } => {
            let start = tiles[0]
                .kind()
                .as_u8()
                .min(tiles[1].kind().as_u8())
                .min(tiles[2].kind().as_u8()) as usize;
            sequence_component(start)
        }
        _ => Component::Triplet(meld.tiles()[0].kind()),
    }
}

fn sequence_component(tile: usize) -> Component {
    debug_assert!(max_sequences_starting_at(tile) > 0);

    let suit = match tile / SUIT_TILE_KIND_COUNT {
        0 => Suit::Manzu,
        1 => Suit::Pinzu,
        2 => Suit::Souzu,
        _ => unreachable!("a sequence cannot start with an honor tile"),
    };

    Component::Sequence {
        suit,
        start: (tile % SUIT_TILE_KIND_COUNT) as u8,
    }
}

pub(super) fn tile_kind(tile: usize) -> TileKind {
    TileKind::new(tile as u8).unwrap_or_else(|| unreachable!("DP only scans the 34 tile kinds"))
}

/// 返回可以从该牌种开始的顺子数量上限。
/// 只有数牌每门花色的 1 到 7 可以作为顺子的起点。
fn max_sequences_starting_at(tile: usize) -> usize {
    if tile < 27 && tile % 9 <= 6 {
        MAX_COPIES
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mahjong::player_index::PlayerIndex;
    use crate::mahjong::tile::Tile;

    fn tile(value: u8) -> Tile {
        Tile::new(value).expect("test tile must be valid")
    }

    fn player(value: u8) -> PlayerIndex {
        PlayerIndex::new(value).expect("test player index must be valid")
    }

    fn counts(tiles: &[(usize, u8)]) -> [u8; TILE_KIND_COUNT] {
        let mut counts = [0; TILE_KIND_COUNT];
        for &(tile, count) in tiles {
            counts[tile] = count;
        }
        counts
    }

    #[test]
    fn multi_state_constraint_tracks_and_requires_a_component() {
        let required_triplet = Component::Triplet(tile_kind(27));
        let mut transitions = vec![[Some(0); COMPONENT_COUNT], [Some(1); COMPONENT_COUNT]];
        transitions[0][required_triplet.index()] = Some(1);
        let constraint = CompiledConstraint {
            start_state: 0,
            transitions,
            accepting: vec![false, true],
        };

        let matching_hand = counts(&[(0, 3), (8, 3), (9, 3), (27, 3), (28, 2)]);
        assert_eq!(
            ordinary_shanten_with_constraint(&matching_hand, &[], &constraint),
            Some(-1)
        );

        // 123m 456m 789m 123p 11s 本来已经完成，但加入东刻要求后还缺三张东。
        let missing_required_triplet = counts(&[
            (0, 1),
            (1, 1),
            (2, 1),
            (3, 1),
            (4, 1),
            (5, 1),
            (6, 1),
            (7, 1),
            (8, 1),
            (9, 1),
            (10, 1),
            (11, 1),
            (18, 2),
        ]);
        assert_eq!(standard_shanten(&missing_required_triplet), -1);
        assert_eq!(
            ordinary_shanten_with_constraint(&missing_required_triplet, &[], &constraint),
            Some(2)
        );
    }

    #[test]
    fn chi_component_uses_the_lowest_tile_as_sequence_start() {
        let chi = Meld::Chi {
            tiles: [tile(4), tile(2), tile(3)],
            called: tile(2),
            from: player(1),
        };

        assert_eq!(
            component_from_meld(&chi),
            Component::Sequence {
                suit: Suit::Manzu,
                start: 2,
            }
        );
    }

    #[test]
    fn existing_chi_counts_as_a_valid_standard_meld() {
        let hand = counts(&[(0, 3), (8, 3), (9, 3), (27, 2)]);
        let chi = Meld::Chi {
            tiles: [tile(18), tile(19), tile(20)],
            called: tile(18),
            from: player(1),
        };

        assert_eq!(
            ordinary_shanten_with_constraint(&hand, &[chi], &standard_constraint()),
            Some(-1)
        );
    }
}
