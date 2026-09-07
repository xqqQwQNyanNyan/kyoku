use super::*;
use kyoku::mahjong::{meld::Meld, tile::Tile};
use kyoku::mahjong::{
    player::{Discard, RiichiState},
    round::{RoundId, RoundPhase, Wind},
};
use kyoku::mortal::{Action, Candidate, Decision, ModelInfo};
use kyoku::review::Review;
use kyoku::review::{PublicPlayer, VisiblePosition};

#[test]
fn cli_requires_player_event_and_one_input() {
    for input in [
        "",
        "--event 2 log.json",
        "--player 4 --event 2 log.json",
        "--player 0 --event -1 log.json",
        "--player 0 --event 2 a b",
        "--player 0 --event 2 --unknown log.json",
    ] {
        assert!(
            Args::parse(input.split_whitespace().map(str::to_owned)).is_err(),
            "{input}"
        );
    }
    let args = Args::parse(
        "--player 2 --event 12 -"
            .split_whitespace()
            .map(str::to_owned),
    )
    .unwrap()
    .unwrap();
    assert_eq!(args.player.get_id(), 2);
    assert_eq!(args.event, Some(12));
    assert!(
        Args::parse("--player 0 log.json".split_whitespace().map(str::to_owned))
            .unwrap()
            .unwrap()
            .event
            .is_none()
    );
    assert_eq!(args.input, "-");
    assert!(Args::parse(["--help".to_owned()]).unwrap().is_none());
}

#[test]
fn output_shows_public_state_and_preserves_final_recommendation() {
    let tile = Tile::new(0).unwrap();
    let review = Review {
        event_index: 12,
        player: PlayerIndex::new(0).unwrap(),
        position: VisiblePosition {
            history: None,
            round: RoundId::new(Wind::East, 1).unwrap(),
            honba: 1,
            riichi_sticks: 2,
            remaining_draws: 50,
            phase: RoundPhase::Initial,
            dora_indicators: vec![tile],
            concealed: vec![tile],
            players: std::array::from_fn(|_| PublicPlayer {
                score: 25_000,
                riichi: RiichiState::Accepted,
                discards: vec![Discard::new(tile, true, true, true)],
                melds: vec![Meld::Ankan { tiles: [tile; 4] }],
            }),
        },
        model: ModelInfo {
            version: 4,
            tag: "test-only".into(),
            sha256: "0".repeat(64),
        },
        decision: Some(Decision {
            recommended: convlog::Event::None,
            candidates: vec![
                Candidate {
                    action: Action::Win,
                    q_value: 0.9,
                },
                Candidate {
                    action: Action::Pass,
                    q_value: 0.1,
                },
            ],
            kan_candidates: vec![],
            shanten: Some(0),
            at_furiten: Some(true),
        }),
        discards: vec![],
    };
    let mut output = Vec::new();
    write_review(&mut output, &review).unwrap();
    let output = String::from_utf8(output).unwrap();
    assert!(output.contains("G012 after event, P0 view"));
    assert!(output.contains("Concealed: [1m]"));
    assert!(output.contains("P3: score=25000 riichi=Accepted"));
    assert!(output.contains("1m(tsumogiri)(riichi)(called)"));
    assert!(output.contains("ankan[1m 1m 1m 1m]"));
    assert!(output.contains("Mortal: None"));
    assert!(output.contains("No discard candidates"));
    assert!(output.contains("sha256="));
}
