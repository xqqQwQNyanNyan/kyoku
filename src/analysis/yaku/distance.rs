use super::types::{Yaku, YakuDistanceError};
use crate::analysis::shanten::{
    COMPONENT_COUNT, ChiitoitsuConstraint, CompiledConstraint, Component, HandFormSpec,
    KokushiConstraint, MAX_COPIES, SEQUENCE_COMPONENT_COUNT, SUIT_TILE_KIND_COUNT, Suit,
    concealed_counts, solve_hand_form, tile_kind, unrestricted_chiitoitsu_constraint,
};
use crate::mahjong::hand::Hand;

const HONOR_TILE_MASK: u64 = ((1u64 << 7) - 1) << 27;
const TERMINAL_TILE_MASK: u64 = (1 << 0) | (1 << 8) | (1 << 9) | (1 << 17) | (1 << 18) | (1 << 26);
const SIMPLE_TILE_MASK: u64 = 0b1111_1110 | (0b1111_1110 << 9) | (0b1111_1110 << 18);
const GREEN_TILE_MASK: u64 = (1 << 19) | (1 << 20) | (1 << 21) | (1 << 23) | (1 << 25) | (1 << 32);
const DRAGON_START: usize = 31;
const WIND_START: usize = 27;

struct YakuEligibility {
    menzen: bool,
}

struct CompiledYaku {
    eligibility: YakuEligibility,
    forms: Vec<HandFormSpec>,
}

impl YakuEligibility {
    fn is_satisfied_by(&self, hand: &Hand) -> bool {
        !self.menzen || hand.melds().iter().all(|meld| !meld.is_open())
    }
}

/// 计算手牌到指定役种和牌形的向听数。
///
/// 返回 `Ok(Some(向听数))`；固定副露已使役种不可能成立时返回 `Ok(None)`。
/// 尚未支持该役种的距离计算时返回 `Err(YakuDistanceError::UnsupportedYaku(yaku))`。
pub fn yaku_shanten(hand: &Hand, yaku: Yaku) -> Result<Option<i8>, YakuDistanceError> {
    let compiled = compile_yaku(yaku)?;
    let counts = concealed_counts(hand);
    debug_assert!(counts.iter().all(|&count| count <= MAX_COPIES as u8));

    if !compiled.eligibility.is_satisfied_by(hand) {
        return Ok(None);
    }

    Ok(compiled
        .forms
        .iter()
        .filter_map(|spec| solve_hand_form(hand, &counts, spec))
        .min())
}

fn compile_yaku(yaku: Yaku) -> Result<CompiledYaku, YakuDistanceError> {
    let (menzen, forms) = match yaku {
        Yaku::Chiitoitsu => (
            false,
            vec![HandFormSpec::Chiitoitsu(
                unrestricted_chiitoitsu_constraint(),
            )],
        ),
        Yaku::Kokushi => (false, vec![HandFormSpec::Kokushi(KokushiConstraint)]),
        Yaku::Toitoi => (false, vec![HandFormSpec::Ordinary(toitoi_constraint())]),
        Yaku::Chinitsu => {
            let mut specs = Vec::with_capacity(6);
            for suit in [Suit::Manzu, Suit::Pinzu, Suit::Souzu] {
                specs.push(HandFormSpec::Ordinary(chinitsu_constraint(suit)));
                specs.push(HandFormSpec::Chiitoitsu(ChiitoitsuConstraint {
                    allowed_tiles: suit_tile_mask(suit),
                    required_groups: Vec::new(),
                }));
            }
            (false, specs)
        }
        Yaku::Ittsu => (
            false,
            [Suit::Manzu, Suit::Pinzu, Suit::Souzu]
                .into_iter()
                .map(|suit| HandFormSpec::Ordinary(ittsu_constraint(suit)))
                .collect(),
        ),
        Yaku::Iipeikou => {
            let mut specs = Vec::with_capacity(SEQUENCE_COMPONENT_COUNT);
            for suit in [Suit::Manzu, Suit::Pinzu, Suit::Souzu] {
                for start in 0..=6 {
                    specs.push(HandFormSpec::Ordinary(iipeikou_constraint(suit, start)));
                }
            }
            (true, specs)
        }
        Yaku::Tanyao => (
            false,
            vec![
                HandFormSpec::Ordinary(allowed_tiles_constraint(SIMPLE_TILE_MASK)),
                HandFormSpec::Chiitoitsu(chiitoitsu_constraint(SIMPLE_TILE_MASK, Vec::new())),
            ],
        ),
        Yaku::Honitsu => {
            let mut specs = Vec::with_capacity(6);
            for suit in [Suit::Manzu, Suit::Pinzu, Suit::Souzu] {
                let suited_tiles = suit_tile_mask(suit);
                let allowed_tiles = suited_tiles | HONOR_TILE_MASK;
                specs.push(HandFormSpec::Ordinary(honitsu_constraint(suit)));
                specs.push(HandFormSpec::Chiitoitsu(chiitoitsu_constraint(
                    allowed_tiles,
                    vec![suited_tiles, HONOR_TILE_MASK],
                )));
            }
            (false, specs)
        }
        Yaku::Honroutou => (
            false,
            vec![
                HandFormSpec::Ordinary(allowed_tiles_constraint(
                    TERMINAL_TILE_MASK | HONOR_TILE_MASK,
                )),
                HandFormSpec::Chiitoitsu(chiitoitsu_constraint(
                    TERMINAL_TILE_MASK | HONOR_TILE_MASK,
                    Vec::new(),
                )),
            ],
        ),
        Yaku::Chanta => (false, vec![HandFormSpec::Ordinary(chanta_constraint())]),
        Yaku::Junchan => (false, vec![HandFormSpec::Ordinary(junchan_constraint())]),
        Yaku::SanshokuDoujun => (
            false,
            (0..=6)
                .map(|start| HandFormSpec::Ordinary(sanshoku_doujun_constraint(start)))
                .collect(),
        ),
        Yaku::SanshokuDoukou => (
            false,
            (0..=8)
                .map(|rank| HandFormSpec::Ordinary(sanshoku_doukou_constraint(rank)))
                .collect(),
        ),
        Yaku::Ryanpeikou => {
            let targets = all_sequence_components();
            let mut specs = Vec::with_capacity(targets.len() * (targets.len() + 1) / 2);
            for (a_index, &a) in targets.iter().enumerate() {
                for &b in &targets[a_index..] {
                    specs.push(HandFormSpec::Ordinary(ryanpeikou_constraint(a, b)));
                }
            }
            (true, specs)
        }
        Yaku::Shousangen => (
            false,
            (DRAGON_START..DRAGON_START + 3)
                .map(|pair| HandFormSpec::Ordinary(shousangen_constraint(pair)))
                .collect(),
        ),
        Yaku::Daisangen => (false, vec![HandFormSpec::Ordinary(daisangen_constraint())]),
        Yaku::Shousuushi => (
            false,
            (WIND_START..WIND_START + 4)
                .map(|pair| HandFormSpec::Ordinary(shousuushi_constraint(pair)))
                .collect(),
        ),
        Yaku::Daisuushi => (false, vec![HandFormSpec::Ordinary(daisuushi_constraint())]),
        Yaku::Tsuuiisou => (
            false,
            vec![
                HandFormSpec::Ordinary(allowed_tiles_constraint(HONOR_TILE_MASK)),
                HandFormSpec::Chiitoitsu(chiitoitsu_constraint(HONOR_TILE_MASK, Vec::new())),
            ],
        ),
        Yaku::Chinroutou => (
            false,
            vec![HandFormSpec::Ordinary(allowed_tiles_constraint(
                TERMINAL_TILE_MASK,
            ))],
        ),
        Yaku::Ryuuiisou => (
            false,
            vec![HandFormSpec::Ordinary(allowed_tiles_constraint(
                GREEN_TILE_MASK,
            ))],
        ),
        Yaku::Riichi
        | Yaku::DoubleRiichi
        | Yaku::Ippatsu
        | Yaku::MenzenTsumo
        | Yaku::Pinfu
        | Yaku::Haku
        | Yaku::Hatsu
        | Yaku::Chun
        | Yaku::Bakaze
        | Yaku::Jikaze
        | Yaku::Haitei
        | Yaku::Houtei
        | Yaku::RinshanKaihou
        | Yaku::Chankan
        | Yaku::Sanankou
        | Yaku::Sankantsu
        | Yaku::Suuankou
        | Yaku::SuuankouTanki
        | Yaku::Suukantsu
        | Yaku::ChuurenPoutou
        | Yaku::JunseiChuurenPoutou
        | Yaku::KokushiJuusanmen
        | Yaku::Tenhou
        | Yaku::Chiihou
        | Yaku::NagashiMangan => return Err(YakuDistanceError::UnsupportedYaku(yaku)),
    };

    Ok(CompiledYaku {
        eligibility: YakuEligibility { menzen },
        forms,
    })
}

fn toitoi_constraint() -> CompiledConstraint {
    let mut transitions = [Some(0); COMPONENT_COUNT];
    transitions[..SEQUENCE_COMPONENT_COUNT].fill(None);

    CompiledConstraint {
        start_state: 0,
        transitions: vec![transitions],
        accepting: vec![true],
    }
}

fn chinitsu_constraint(suit: Suit) -> CompiledConstraint {
    let mut transitions = [None; COMPONENT_COUNT];
    for start in 0..=6 {
        transitions[Component::Sequence { suit, start }.index()] = Some(0);
    }

    let suit_start = suit as usize * SUIT_TILE_KIND_COUNT;
    for tile in suit_start..suit_start + SUIT_TILE_KIND_COUNT {
        let tile = tile_kind(tile);
        transitions[Component::Triplet(tile).index()] = Some(0);
        transitions[Component::Pair(tile).index()] = Some(0);
    }

    CompiledConstraint {
        start_state: 0,
        transitions: vec![transitions],
        accepting: vec![true],
    }
}

fn ittsu_constraint(suit: Suit) -> CompiledConstraint {
    let mut transitions = vec![[None; COMPONENT_COUNT]; 8];
    for (state, state_transitions) in transitions.iter_mut().enumerate() {
        state_transitions.fill(Some(state));
        state_transitions[Component::Sequence { suit, start: 0 }.index()] = Some(state | 0b001);
        state_transitions[Component::Sequence { suit, start: 3 }.index()] = Some(state | 0b010);
        state_transitions[Component::Sequence { suit, start: 6 }.index()] = Some(state | 0b100);
    }

    CompiledConstraint {
        start_state: 0,
        transitions,
        accepting: vec![false, false, false, false, false, false, false, true],
    }
}

fn iipeikou_constraint(suit: Suit, start: u8) -> CompiledConstraint {
    let target = Component::Sequence { suit, start };
    let mut transitions = vec![[None; COMPONENT_COUNT]; 3];
    for (state, state_transitions) in transitions.iter_mut().enumerate() {
        state_transitions.fill(Some(state));
        state_transitions[target.index()] = Some((state + 1).min(2));
    }

    CompiledConstraint {
        start_state: 0,
        transitions,
        accepting: vec![false, false, true],
    }
}

fn chiitoitsu_constraint(allowed_tiles: u64, required_groups: Vec<u64>) -> ChiitoitsuConstraint {
    ChiitoitsuConstraint {
        allowed_tiles,
        required_groups,
    }
}

fn allowed_tiles_constraint(allowed_tiles: u64) -> CompiledConstraint {
    let mut state = [None; COMPONENT_COUNT];

    for suit in [Suit::Manzu, Suit::Pinzu, Suit::Souzu] {
        let suit_start = suit as usize * SUIT_TILE_KIND_COUNT;
        for start in 0..=6u8 {
            let sequence_mask = 0b111u64 << (suit_start + start as usize);
            if sequence_mask & !allowed_tiles == 0 {
                state[Component::Sequence { suit, start }.index()] = Some(0);
            }
        }
    }

    for tile in 0..34 {
        if (allowed_tiles >> tile) & 1 != 0 {
            let tile = tile_kind(tile);
            state[Component::Triplet(tile).index()] = Some(0);
            state[Component::Pair(tile).index()] = Some(0);
        }
    }

    CompiledConstraint {
        start_state: 0,
        transitions: vec![state],
        accepting: vec![true],
    }
}

fn honitsu_constraint(suit: Suit) -> CompiledConstraint {
    const SEEN_NUMBERED: usize = 0b01;
    const SEEN_HONOR: usize = 0b10;

    let mut transitions = vec![[None; COMPONENT_COUNT]; 4];
    let suit_start = suit as usize * SUIT_TILE_KIND_COUNT;
    for (state, state_transitions) in transitions.iter_mut().enumerate() {
        for start in 0..=6 {
            state_transitions[Component::Sequence { suit, start }.index()] =
                Some(state | SEEN_NUMBERED);
        }
        for tile in suit_start..suit_start + SUIT_TILE_KIND_COUNT {
            let tile = tile_kind(tile);
            state_transitions[Component::Triplet(tile).index()] = Some(state | SEEN_NUMBERED);
            state_transitions[Component::Pair(tile).index()] = Some(state | SEEN_NUMBERED);
        }
        for tile in 27..34 {
            let tile = tile_kind(tile);
            state_transitions[Component::Triplet(tile).index()] = Some(state | SEEN_HONOR);
            state_transitions[Component::Pair(tile).index()] = Some(state | SEEN_HONOR);
        }
    }

    CompiledConstraint {
        start_state: 0,
        transitions,
        accepting: vec![false, false, false, true],
    }
}

fn chanta_constraint() -> CompiledConstraint {
    const SEEN_SEQUENCE: usize = 0b01;
    const SEEN_HONOR: usize = 0b10;

    let mut transitions = vec![[None; COMPONENT_COUNT]; 4];
    for (state, state_transitions) in transitions.iter_mut().enumerate() {
        for suit in [Suit::Manzu, Suit::Pinzu, Suit::Souzu] {
            for start in [0, 6] {
                state_transitions[Component::Sequence { suit, start }.index()] =
                    Some(state | SEEN_SEQUENCE);
            }
        }
        for tile in terminal_tiles() {
            state_transitions[Component::Triplet(tile).index()] = Some(state);
            state_transitions[Component::Pair(tile).index()] = Some(state);
        }
        for tile in 27..34 {
            let tile = tile_kind(tile);
            state_transitions[Component::Triplet(tile).index()] = Some(state | SEEN_HONOR);
            state_transitions[Component::Pair(tile).index()] = Some(state | SEEN_HONOR);
        }
    }

    CompiledConstraint {
        start_state: 0,
        transitions,
        accepting: vec![false, false, false, true],
    }
}

fn junchan_constraint() -> CompiledConstraint {
    let mut transitions = vec![[None; COMPONENT_COUNT]; 2];
    for (state, state_transitions) in transitions.iter_mut().enumerate() {
        for suit in [Suit::Manzu, Suit::Pinzu, Suit::Souzu] {
            for start in [0, 6] {
                state_transitions[Component::Sequence { suit, start }.index()] = Some(1);
            }
        }
        for tile in terminal_tiles() {
            state_transitions[Component::Triplet(tile).index()] = Some(state);
            state_transitions[Component::Pair(tile).index()] = Some(state);
        }
    }

    CompiledConstraint {
        start_state: 0,
        transitions,
        accepting: vec![false, true],
    }
}

fn sanshoku_doujun_constraint(start: u8) -> CompiledConstraint {
    let mut transitions = vec![[None; COMPONENT_COUNT]; 8];
    for (state, state_transitions) in transitions.iter_mut().enumerate() {
        state_transitions.fill(Some(state));
        for (suit, bit) in [
            (Suit::Manzu, 0b001),
            (Suit::Pinzu, 0b010),
            (Suit::Souzu, 0b100),
        ] {
            state_transitions[Component::Sequence { suit, start }.index()] = Some(state | bit);
        }
    }

    CompiledConstraint {
        start_state: 0,
        transitions,
        accepting: accepting_only(8, 0b111),
    }
}

fn sanshoku_doukou_constraint(rank: u8) -> CompiledConstraint {
    let mut transitions = vec![[None; COMPONENT_COUNT]; 8];
    for (state, state_transitions) in transitions.iter_mut().enumerate() {
        state_transitions.fill(Some(state));
        for (suit, bit) in [
            (Suit::Manzu, 0b001),
            (Suit::Pinzu, 0b010),
            (Suit::Souzu, 0b100),
        ] {
            let tile = tile_kind(suit as usize * SUIT_TILE_KIND_COUNT + rank as usize);
            state_transitions[Component::Triplet(tile).index()] = Some(state | bit);
        }
    }

    CompiledConstraint {
        start_state: 0,
        transitions,
        accepting: accepting_only(8, 0b111),
    }
}

fn ryanpeikou_constraint(a: Component, b: Component) -> CompiledConstraint {
    if a == b {
        let mut transitions = vec![[None; COMPONENT_COUNT]; 5];
        for (count, state_transitions) in transitions.iter_mut().enumerate() {
            state_transitions.fill(Some(count));
            state_transitions[a.index()] = Some((count + 1).min(4));
        }
        return CompiledConstraint {
            start_state: 0,
            transitions,
            accepting: accepting_only(5, 4),
        };
    }

    let mut transitions = vec![[None; COMPONENT_COUNT]; 9];
    for (state, state_transitions) in transitions.iter_mut().enumerate() {
        state_transitions.fill(Some(state));
        let a_count = state / 3;
        let b_count = state % 3;
        state_transitions[a.index()] = Some((a_count + 1).min(2) * 3 + b_count);
        state_transitions[b.index()] = Some(a_count * 3 + (b_count + 1).min(2));
    }

    CompiledConstraint {
        start_state: 0,
        transitions,
        accepting: accepting_only(9, 8),
    }
}

fn shousangen_constraint(pair: usize) -> CompiledConstraint {
    let mut transitions = vec![[None; COMPONENT_COUNT]; 8];
    for (state, state_transitions) in transitions.iter_mut().enumerate() {
        state_transitions.fill(Some(state));
        state_transitions[Component::Pair(tile_kind(pair)).index()] = Some(state | 0b001);

        let mut bit = 0b010;
        for dragon in DRAGON_START..DRAGON_START + 3 {
            if dragon != pair {
                state_transitions[Component::Triplet(tile_kind(dragon)).index()] =
                    Some(state | bit);
                bit <<= 1;
            }
        }
    }

    CompiledConstraint {
        start_state: 0,
        transitions,
        accepting: accepting_only(8, 0b111),
    }
}

fn daisangen_constraint() -> CompiledConstraint {
    let mut transitions = vec![[None; COMPONENT_COUNT]; 8];
    for (state, state_transitions) in transitions.iter_mut().enumerate() {
        state_transitions.fill(Some(state));
        for dragon in 0..3 {
            state_transitions[Component::Triplet(tile_kind(DRAGON_START + dragon)).index()] =
                Some(state | (1 << dragon));
        }
    }

    CompiledConstraint {
        start_state: 0,
        transitions,
        accepting: accepting_only(8, 0b111),
    }
}

fn shousuushi_constraint(pair: usize) -> CompiledConstraint {
    let mut transitions = vec![[None; COMPONENT_COUNT]; 16];
    for (state, state_transitions) in transitions.iter_mut().enumerate() {
        state_transitions.fill(Some(state));
        state_transitions[Component::Pair(tile_kind(pair)).index()] = Some(state | 0b0001);

        let mut bit = 0b0010;
        for wind in WIND_START..WIND_START + 4 {
            if wind != pair {
                state_transitions[Component::Triplet(tile_kind(wind)).index()] = Some(state | bit);
                bit <<= 1;
            }
        }
    }

    CompiledConstraint {
        start_state: 0,
        transitions,
        accepting: accepting_only(16, 0b1111),
    }
}

fn daisuushi_constraint() -> CompiledConstraint {
    let mut transitions = vec![[None; COMPONENT_COUNT]; 16];
    for (state, state_transitions) in transitions.iter_mut().enumerate() {
        state_transitions.fill(Some(state));
        for wind in 0..4 {
            state_transitions[Component::Triplet(tile_kind(WIND_START + wind)).index()] =
                Some(state | (1 << wind));
        }
    }

    CompiledConstraint {
        start_state: 0,
        transitions,
        accepting: accepting_only(16, 0b1111),
    }
}

fn accepting_only(state_count: usize, accepting_state: usize) -> Vec<bool> {
    let mut accepting = vec![false; state_count];
    accepting[accepting_state] = true;
    accepting
}

fn all_sequence_components() -> Vec<Component> {
    let mut components = Vec::with_capacity(SEQUENCE_COMPONENT_COUNT);
    for suit in [Suit::Manzu, Suit::Pinzu, Suit::Souzu] {
        for start in 0..=6 {
            components.push(Component::Sequence { suit, start });
        }
    }
    components
}

fn terminal_tiles() -> impl Iterator<Item = crate::mahjong::tile::TileKind> {
    [0, 8, 9, 17, 18, 26].into_iter().map(tile_kind)
}

fn suit_tile_mask(suit: Suit) -> u64 {
    ((1u64 << SUIT_TILE_KIND_COUNT) - 1) << (suit as usize * SUIT_TILE_KIND_COUNT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mahjong::meld::Meld;
    use crate::mahjong::player_index::PlayerIndex;
    use crate::mahjong::tile::Tile;

    fn tile(value: u8) -> Tile {
        Tile::new(value).expect("test tile must be valid")
    }

    fn player(value: u8) -> PlayerIndex {
        PlayerIndex::new(value).expect("test player index must be valid")
    }

    fn counts(tiles: &[(usize, u8)]) -> [u8; 34] {
        let mut counts = [0; 34];
        for &(tile, count) in tiles {
            counts[tile] = count;
        }
        counts
    }

    fn hand_from_counts(counts: &[u8; 34]) -> Hand {
        hand_with_melds(counts, Vec::new())
    }

    fn hand_with_melds(counts: &[u8; 34], melds: Vec<Meld>) -> Hand {
        let concealed = counts
            .iter()
            .enumerate()
            .flat_map(|(tile_index, &count)| {
                std::iter::repeat_n(tile(tile_index as u8), count as usize)
            })
            .collect();
        Hand::new(concealed, melds).expect("test hand must have a valid size")
    }

    #[test]
    fn toitoi_constraint_accepts_triplets_and_pairs_but_rejects_sequences() {
        let constraint = toitoi_constraint();
        let state = constraint.start_state;

        assert_eq!(
            constraint.transition(state, Component::Triplet(tile_kind(0))),
            Some(state)
        );
        assert_eq!(
            constraint.transition(state, Component::Pair(tile_kind(27))),
            Some(state)
        );
        assert_eq!(
            constraint.transition(
                state,
                Component::Sequence {
                    suit: Suit::Manzu,
                    start: 0,
                }
            ),
            None
        );
    }

    #[test]
    fn ittsu_constraint_sets_each_segment_bit() {
        let constraint = ittsu_constraint(Suit::Manzu);

        let state = constraint
            .transition(
                0b000,
                Component::Sequence {
                    suit: Suit::Manzu,
                    start: 0,
                },
            )
            .expect("123m transition must be valid");
        assert_eq!(state, 0b001);

        let state = constraint
            .transition(
                state,
                Component::Sequence {
                    suit: Suit::Manzu,
                    start: 3,
                },
            )
            .expect("456m transition must be valid");
        assert_eq!(state, 0b011);

        let state = constraint
            .transition(
                state,
                Component::Sequence {
                    suit: Suit::Manzu,
                    start: 6,
                },
            )
            .expect("789m transition must be valid");
        assert_eq!(state, 0b111);
        assert!(constraint.is_accepting(state));

        assert_eq!(
            constraint.transition(
                0b001,
                Component::Sequence {
                    suit: Suit::Pinzu,
                    start: 3,
                }
            ),
            Some(0b001)
        );
        assert_eq!(
            constraint.transition(0b011, Component::Triplet(tile_kind(27))),
            Some(0b011)
        );
    }

    #[test]
    fn iipeikou_constraint_counts_the_target_sequence_twice() {
        let constraint = iipeikou_constraint(Suit::Manzu, 0);
        let target = Component::Sequence {
            suit: Suit::Manzu,
            start: 0,
        };

        assert_eq!(constraint.transition(0, target), Some(1));
        assert_eq!(constraint.transition(1, target), Some(2));
        assert_eq!(constraint.transition(2, target), Some(2));
        assert!(!constraint.is_accepting(1));
        assert!(constraint.is_accepting(2));

        assert_eq!(
            constraint.transition(
                1,
                Component::Sequence {
                    suit: Suit::Manzu,
                    start: 1,
                }
            ),
            Some(1)
        );
        assert_eq!(
            constraint.transition(1, Component::Triplet(tile_kind(27))),
            Some(1)
        );
    }

    #[test]
    fn allowed_tiles_constraint_rejects_every_component_with_a_forbidden_tile() {
        let tanyao = allowed_tiles_constraint(SIMPLE_TILE_MASK);

        assert_eq!(
            tanyao.transition(
                0,
                Component::Sequence {
                    suit: Suit::Manzu,
                    start: 1,
                }
            ),
            Some(0)
        );
        assert_eq!(
            tanyao.transition(
                0,
                Component::Sequence {
                    suit: Suit::Manzu,
                    start: 0,
                }
            ),
            None
        );
        assert_eq!(
            tanyao.transition(0, Component::Triplet(tile_kind(4))),
            Some(0)
        );
        assert_eq!(tanyao.transition(0, Component::Pair(tile_kind(27))), None);
    }

    #[test]
    fn honitsu_constraint_tracks_numbered_and_honor_components() {
        let constraint = honitsu_constraint(Suit::Pinzu);
        let state = constraint
            .transition(
                0,
                Component::Sequence {
                    suit: Suit::Pinzu,
                    start: 2,
                },
            )
            .expect("target-suit sequence must be valid");
        assert_eq!(state, 0b01);

        let state = constraint
            .transition(state, Component::Triplet(tile_kind(31)))
            .expect("honor triplet must be valid");
        assert_eq!(state, 0b11);
        assert!(constraint.is_accepting(state));
        assert_eq!(
            constraint.transition(state, Component::Pair(tile_kind(0))),
            None
        );
    }

    #[test]
    fn chanta_and_junchan_apply_their_component_rules_directly() {
        let chanta = chanta_constraint();
        let state = chanta
            .transition(
                0,
                Component::Sequence {
                    suit: Suit::Souzu,
                    start: 6,
                },
            )
            .expect("789s must be valid");
        assert_eq!(state, 0b01);
        assert_eq!(
            chanta.transition(state, Component::Triplet(tile_kind(4))),
            None
        );
        let honor_only = chanta
            .transition(0, Component::Triplet(tile_kind(27)))
            .expect("honor triplet must be valid");
        assert!(!chanta.is_accepting(honor_only));
        let state = chanta
            .transition(state, Component::Pair(tile_kind(27)))
            .expect("honor pair must be valid");
        assert!(chanta.is_accepting(state));

        let junchan = junchan_constraint();
        assert_eq!(
            junchan.transition(0, Component::Triplet(tile_kind(27))),
            None
        );
        assert_eq!(
            junchan.transition(0, Component::Pair(tile_kind(8))),
            Some(0)
        );
        assert!(!junchan.is_accepting(0));
    }

    #[test]
    fn sanshoku_constraints_set_only_the_matching_suit_bits() {
        let doujun = sanshoku_doujun_constraint(3);
        assert_eq!(
            doujun.transition(
                0,
                Component::Sequence {
                    suit: Suit::Pinzu,
                    start: 3,
                }
            ),
            Some(0b010)
        );
        assert_eq!(
            doujun.transition(
                0b010,
                Component::Sequence {
                    suit: Suit::Souzu,
                    start: 2,
                }
            ),
            Some(0b010)
        );

        let doukou = sanshoku_doukou_constraint(4);
        assert_eq!(
            doukou.transition(0, Component::Triplet(tile_kind(13))),
            Some(0b010)
        );
        assert_eq!(
            doukou.transition(0b010, Component::Pair(tile_kind(22))),
            Some(0b010)
        );
    }

    #[test]
    fn ryanpeikou_same_target_requires_four_transitions() {
        let target = Component::Sequence {
            suit: Suit::Manzu,
            start: 0,
        };
        let constraint = ryanpeikou_constraint(target, target);

        assert_eq!(constraint.transitions.len(), 5);
        let mut state = constraint.start_state;
        for expected in 1..=4 {
            state = constraint
                .transition(state, target)
                .expect("target sequence must be valid");
            assert_eq!(state, expected);
            assert_eq!(constraint.is_accepting(state), expected == 4);
        }
    }

    #[test]
    fn dragon_and_wind_constraints_require_pair_and_triplet_roles() {
        let shousangen = shousangen_constraint(33);
        let state = shousangen
            .transition(0, Component::Pair(tile_kind(33)))
            .and_then(|state| shousangen.transition(state, Component::Triplet(tile_kind(31))))
            .and_then(|state| shousangen.transition(state, Component::Triplet(tile_kind(32))))
            .expect("required dragon components must be valid");
        assert!(shousangen.is_accepting(state));
        assert_eq!(
            shousangen.transition(0, Component::Triplet(tile_kind(33))),
            Some(0)
        );

        let daisuushi = daisuushi_constraint();
        let mut state = 0;
        for wind in WIND_START..WIND_START + 4 {
            state = daisuushi
                .transition(state, Component::Triplet(tile_kind(wind)))
                .expect("wind triplet must be valid");
        }
        assert!(daisuushi.is_accepting(state));
    }

    #[test]
    fn new_yaku_compile_to_the_expected_forms_and_state_sizes() {
        let cases = [
            (Yaku::Tanyao, 2),
            (Yaku::Honitsu, 6),
            (Yaku::Honroutou, 2),
            (Yaku::Chanta, 1),
            (Yaku::Junchan, 1),
            (Yaku::SanshokuDoujun, 7),
            (Yaku::SanshokuDoukou, 9),
            (Yaku::Ryanpeikou, 231),
            (Yaku::Shousangen, 3),
            (Yaku::Daisangen, 1),
            (Yaku::Shousuushi, 4),
            (Yaku::Daisuushi, 1),
            (Yaku::Tsuuiisou, 2),
            (Yaku::Chinroutou, 1),
            (Yaku::Ryuuiisou, 1),
        ];

        for (yaku, form_count) in cases {
            assert_eq!(
                compile_yaku(yaku)
                    .expect("existing yaku distance must be supported")
                    .forms
                    .len(),
                form_count,
                "{yaku:?}"
            );
        }

        assert_eq!(honitsu_constraint(Suit::Manzu).transitions.len(), 4);
        assert_eq!(chanta_constraint().transitions.len(), 4);
        assert_eq!(junchan_constraint().transitions.len(), 2);
        assert_eq!(sanshoku_doujun_constraint(0).transitions.len(), 8);
        assert_eq!(sanshoku_doukou_constraint(0).transitions.len(), 8);
        assert_eq!(daisangen_constraint().transitions.len(), 8);
        assert_eq!(shousuushi_constraint(27).transitions.len(), 16);
        assert_eq!(daisuushi_constraint().transitions.len(), 16);
    }

    #[test]
    fn dual_family_yaku_are_solved_by_both_ordinary_and_chiitoitsu_specs() {
        let cases = [
            (
                Yaku::Tanyao,
                counts(&[
                    (1, 1),
                    (2, 2),
                    (3, 2),
                    (4, 1),
                    (12, 1),
                    (13, 3),
                    (14, 1),
                    (23, 1),
                    (24, 1),
                    (25, 1),
                ]),
                counts(&[(1, 2), (3, 2), (5, 2), (7, 2), (10, 2), (13, 2), (16, 2)]),
            ),
            (
                Yaku::Honitsu,
                counts(&[
                    (0, 1),
                    (1, 1),
                    (2, 1),
                    (3, 1),
                    (4, 1),
                    (5, 1),
                    (6, 1),
                    (7, 1),
                    (8, 1),
                    (27, 3),
                    (28, 2),
                ]),
                counts(&[(0, 2), (2, 2), (5, 2), (27, 2), (29, 2), (31, 2), (33, 2)]),
            ),
            (
                Yaku::Honroutou,
                counts(&[(0, 3), (8, 3), (9, 3), (27, 3), (31, 2)]),
                counts(&[(0, 2), (8, 2), (9, 2), (17, 2), (27, 2), (31, 2), (33, 2)]),
            ),
            (
                Yaku::Tsuuiisou,
                counts(&[(27, 3), (28, 3), (29, 3), (30, 3), (31, 2)]),
                counts(&[
                    (27, 2),
                    (28, 2),
                    (29, 2),
                    (30, 2),
                    (31, 2),
                    (32, 2),
                    (33, 2),
                ]),
            ),
        ];

        for (yaku, ordinary_counts, chiitoitsu_counts) in cases {
            let compiled = compile_yaku(yaku).expect("existing yaku distance must be supported");
            let ordinary_hand = hand_from_counts(&ordinary_counts);
            let chiitoitsu_hand = hand_from_counts(&chiitoitsu_counts);

            assert!(compiled.forms.iter().any(|spec| {
                matches!(spec, HandFormSpec::Ordinary(_))
                    && solve_hand_form(&ordinary_hand, &ordinary_counts, spec) == Some(-1)
            }));
            assert!(compiled.forms.iter().any(|spec| {
                matches!(spec, HandFormSpec::Chiitoitsu(_))
                    && solve_hand_form(&chiitoitsu_hand, &chiitoitsu_counts, spec) == Some(-1)
            }));
        }
    }

    #[test]
    fn iipeikou_compiles_to_twenty_one_menzen_ordinary_forms() {
        let compiled =
            compile_yaku(Yaku::Iipeikou).expect("existing yaku distance must be supported");

        assert!(compiled.eligibility.menzen);
        assert_eq!(compiled.forms.len(), SEQUENCE_COMPONENT_COUNT);
        assert!(
            compiled
                .forms
                .iter()
                .all(|spec| matches!(spec, HandFormSpec::Ordinary(_)))
        );
    }

    #[test]
    fn chinitsu_compiles_ordinary_and_chiitoitsu_families() {
        let closed_counts = counts(&[(0, 2), (1, 2), (2, 2), (3, 2), (5, 2), (7, 2), (8, 2)]);
        let hand = hand_from_counts(&closed_counts);
        let compiled =
            compile_yaku(Yaku::Chinitsu).expect("existing yaku distance must be supported");
        let specs = &compiled.forms;

        assert!(!compiled.eligibility.menzen);
        assert_eq!(specs.len(), 6);
        assert!(specs.iter().any(|spec| {
            matches!(spec, HandFormSpec::Chiitoitsu(_))
                && solve_hand_form(&hand, &closed_counts, spec) == Some(-1)
        }));
        assert!(specs.iter().all(|spec| {
            !matches!(spec, HandFormSpec::Ordinary(_))
                || solve_hand_form(&hand, &closed_counts, spec) != Some(-1)
        }));

        let open_counts = counts(&[
            (0, 3),
            (1, 2),
            (3, 1),
            (4, 1),
            (5, 1),
            (6, 1),
            (7, 1),
            (8, 1),
        ]);
        let chi = Meld::Chi {
            tiles: [tile(0), tile(1), tile(2)],
            called: tile(0),
            from: player(1),
        };
        let open_hand = hand_with_melds(&open_counts, vec![chi]);
        assert!(specs.iter().all(|spec| {
            !matches!(spec, HandFormSpec::Chiitoitsu(_))
                || solve_hand_form(&open_hand, &open_counts, spec).is_none()
        }));
        assert!(specs.iter().any(|spec| {
            matches!(spec, HandFormSpec::Ordinary(_))
                && solve_hand_form(&open_hand, &open_counts, spec) == Some(-1)
        }));
    }

    #[test]
    fn existing_chi_makes_toitoi_unreachable() {
        let concealed = counts(&[(0, 3), (8, 3), (9, 3), (27, 2)]);
        let chi = Meld::Chi {
            tiles: [tile(18), tile(19), tile(20)],
            called: tile(18),
            from: player(1),
        };
        let hand = hand_with_melds(&concealed, vec![chi]);

        assert_eq!(
            yaku_shanten(&hand, Yaku::Toitoi).expect("existing yaku distance must be supported"),
            None
        );
    }

    #[test]
    fn existing_pon_counts_as_a_valid_toitoi_meld() {
        let concealed = counts(&[(0, 3), (8, 3), (9, 3), (27, 2)]);
        let pon = Meld::Pon {
            tiles: [tile(26); 3],
            called: tile(26),
            from: player(1),
        };
        let hand = hand_with_melds(&concealed, vec![pon]);

        assert_eq!(
            yaku_shanten(&hand, Yaku::Toitoi).expect("existing yaku distance must be supported"),
            Some(-1)
        );
    }
}
