use kyoku::mahjong::tile::{Tile, TileKind};

fn tile(value: u8) -> Tile {
    Tile::new(value).expect("test tile must be valid")
}

fn kind(value: u8) -> TileKind {
    TileKind::new(value).expect("test tile kind must be valid")
}

#[test]
fn constructors_validate_values() {
    assert_eq!(Tile::new(0), Some(tile(0)));
    assert_eq!(Tile::new(36), Some(tile(36)));
    assert_eq!(Tile::new(37), None);

    assert_eq!(TileKind::new(0), Some(kind(0)));
    assert_eq!(TileKind::new(33), Some(kind(33)));
    assert_eq!(TileKind::new(34), None);
}

#[test]
fn integer_conversions_validate_and_preserve_values() {
    assert_eq!(Tile::try_from(36), Ok(tile(36)));
    assert_eq!(Tile::try_from(37).unwrap_err().value(), 37);
    assert_eq!(tile(36).as_u8(), 36);

    assert_eq!(TileKind::try_from(33), Ok(kind(33)));
    assert_eq!(TileKind::try_from(34).unwrap_err().value(), 34);
    assert_eq!(kind(33).as_u8(), 33);
}

#[test]
fn regular_tiles_keep_their_kind() {
    for value in 0..=33 {
        assert_eq!(TileKind::from(tile(value)), kind(value));
    }
}

#[test]
fn red_fives_map_to_regular_five_kinds() {
    assert_eq!(TileKind::from(tile(34)), kind(4));
    assert_eq!(TileKind::from(tile(35)), kind(13));
    assert_eq!(TileKind::from(tile(36)), kind(22));
}

#[test]
fn only_red_fives_are_aka() {
    for value in 0..=33 {
        assert!(!tile(value).is_aka());
    }

    for value in 34..=36 {
        assert!(tile(value).is_aka());
    }
}

#[test]
fn invalid_value_errors_expose_the_original_value() {
    assert_eq!(Tile::try_from(37).unwrap_err().value(), 37);
    assert_eq!(TileKind::try_from(34).unwrap_err().value(), 34);
}

#[test]
fn tiles_sort_by_kind_with_aka_before_the_regular_five() {
    let mut tiles = [
        tile(36),
        tile(23),
        tile(35),
        tile(14),
        tile(34),
        tile(5),
        tile(22),
        tile(13),
        tile(4),
        tile(3),
    ];

    tiles.sort();

    assert_eq!(
        tiles,
        [
            tile(3),
            tile(34),
            tile(4),
            tile(5),
            tile(35),
            tile(13),
            tile(14),
            tile(36),
            tile(22),
            tile(23),
        ]
    );
}
