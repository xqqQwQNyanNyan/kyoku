use super::shanten::{
    COMPONENT_COUNT, ChiitoitsuConstraint, CompiledConstraint, Component, HandFormSpec,
    KokushiConstraint, MAX_COPIES, SEQUENCE_COMPONENT_COUNT, SUIT_TILE_KIND_COUNT, Suit,
    concealed_counts, solve_hand_form, tile_kind, unrestricted_chiitoitsu_constraint,
};
use crate::mahjong::hand::Hand;

/// 可单独计算向听数的役种。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Yaku {
    /// 七对子。
    Chiitoitsu,
    /// 国士无双。
    Kokushi,
    /// 对对和。
    Toitoi,
    /// 清一色。
    Chinitsu,
    /// 一气通贯。
    Ittsu,
    /// 一杯口。
    Iipeikou,
}

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

/// 计算手牌到指定役种和牌形的向听数；固定副露已使役种不可能成立时返回 `None`。
pub fn yaku_shanten(hand: &Hand, yaku: Yaku) -> Option<i8> {
    let counts = concealed_counts(hand);
    debug_assert!(counts.iter().all(|&count| count <= MAX_COPIES as u8));

    let compiled = compile_yaku(yaku);
    if !compiled.eligibility.is_satisfied_by(hand) {
        return None;
    }

    compiled
        .forms
        .iter()
        .filter_map(|spec| solve_hand_form(hand, &counts, spec))
        .min()
}

fn compile_yaku(yaku: Yaku) -> CompiledYaku {
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
    };

    CompiledYaku {
        eligibility: YakuEligibility { menzen },
        forms,
    }
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
    fn iipeikou_compiles_to_twenty_one_menzen_ordinary_forms() {
        let compiled = compile_yaku(Yaku::Iipeikou);

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
        let compiled = compile_yaku(Yaku::Chinitsu);
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

        assert_eq!(yaku_shanten(&hand, Yaku::Toitoi), None);
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

        assert_eq!(yaku_shanten(&hand, Yaku::Toitoi), Some(-1));
    }
}
