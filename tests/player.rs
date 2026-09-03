use kyoku::mahjong::player::PlayerIndex;

fn player(value: u8) -> PlayerIndex {
    PlayerIndex::new(value).expect("test player index must be valid")
}

#[test]
fn constructor_accepts_only_four_player_indices() {
    for value in 0..=3 {
        assert_eq!(PlayerIndex::new(value), Some(player(value)));
    }

    assert_eq!(PlayerIndex::new(4), None);
    assert_eq!(PlayerIndex::new(u8::MAX), None);
}

#[test]
fn integer_conversion_reports_the_invalid_value() {
    assert_eq!(PlayerIndex::try_from(3), Ok(player(3)));

    let error = PlayerIndex::try_from(4).unwrap_err();
    assert_eq!(error.value(), 4);
    assert_eq!(error.to_string(), "invalid player index 4; expected 0..=3");
}

#[test]
fn get_id_returns_the_original_value() {
    for value in 0..=3 {
        assert_eq!(player(value).get_id(), value);
    }
}
