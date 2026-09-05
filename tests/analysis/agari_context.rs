use kyoku::analysis::{AgariContext, RiichiStatus, RonSource, TsumoSource, WinMethod};
use kyoku::mahjong::round::Wind;
use kyoku::mahjong::tile::{Tile, TileKind};

#[test]
fn context_keeps_win_method_and_both_winds_explicit() {
    let winds = [Wind::East, Wind::South, Wind::West, Wind::North];
    let winning_tile = TileKind::new(27).unwrap();
    for win_method in [
        WinMethod::Tsumo(TsumoSource::Wall),
        WinMethod::Ron(RonSource::Discard),
    ] {
        for round_wind in winds {
            for seat_wind in winds {
                let context = AgariContext {
                    winning_tile,
                    win_method,
                    round_wind,
                    seat_wind,
                    riichi: RiichiStatus::None,
                };
                assert_eq!(context.winning_tile, winning_tile);
                assert_eq!(context.win_method, win_method);
                assert_eq!(context.round_wind, round_wind);
                assert_eq!(context.seat_wind, seat_wind);
            }
        }
    }
}

#[test]
fn red_and_regular_winning_tiles_share_the_same_context() {
    for (regular, red) in [(4, 34), (13, 35), (22, 36)] {
        let context = AgariContext {
            winning_tile: Tile::new(regular).unwrap().kind(),
            win_method: WinMethod::Ron(RonSource::Discard),
            round_wind: Wind::East,
            seat_wind: Wind::East,
            riichi: RiichiStatus::None,
        };
        assert_eq!(
            context,
            AgariContext {
                winning_tile: Tile::new(red).unwrap().kind(),
                ..context
            }
        );
    }
}
