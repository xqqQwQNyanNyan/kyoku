use super::*;
use kyoku::mahjong::tile::TileKind;
use kyoku::mortal::{Action, Candidate, Decision, KanCandidate};

fn render(kan_candidates: Vec<KanCandidate>) -> String {
    let decision = Decision {
        recommended: convlog::Event::None,
        candidates: vec![
            Candidate {
                action: Action::Kan,
                q_value: 0.8,
            },
            Candidate {
                action: Action::Pass,
                q_value: 0.2,
            },
        ],
        kan_candidates,
        shanten: None,
        at_furiten: None,
    };
    let mut output = Vec::new();
    write_decision(&mut output, decision).unwrap();
    String::from_utf8(output).unwrap()
}

#[test]
fn single_kan_uses_main_q_and_hides_selection_q() {
    let output = render(vec![KanCandidate {
        tile: TileKind::new(0).unwrap(),
        q_value: 0.0,
    }]);
    let kan = output.lines().find(|line| line.contains("Kan 1m")).unwrap();
    assert!(kan.contains("Q=0.80000 (main)"));
    assert!(!output.contains("Q=0.00000"));
    assert!(!output.contains("Kan selection"));
    // 候选表即使以 Kan 为首项，最终动作也必须保持为引擎返回的跳过。
    assert!(output.starts_with(&format!(
        "Mortal: {}\n",
        format_event(&convlog::Event::None)
    )));
}

#[test]
fn multiple_kans_keep_main_and_selection_q_separate() {
    let output = render(vec![
        KanCandidate {
            tile: TileKind::new(0).unwrap(),
            q_value: 0.0,
        },
        KanCandidate {
            tile: TileKind::new(1).unwrap(),
            q_value: 0.3,
        },
    ]);
    let (main, selection) = output
        .split_once("Kan selection (separate evaluation):")
        .unwrap();
    assert!(main.contains("Q=0.80000 (main)"));
    assert!(!main.contains("1m"));
    assert!(!main.contains("2m"));
    assert!(selection.contains("1m Q=0.00000"));
    assert!(selection.contains("2m Q=0.30000"));
}

#[test]
fn kan_without_selection_keeps_main_q() {
    let output = render(vec![]);
    assert!(output.contains("Q=0.80000 (main)"));
    assert!(!output.contains("Kan selection"));
}
