use super::super::position::{Snapshot, ToolError, bad_position, kind_name, parse_tile};
use crate::{mahjong::tile::Tile, replay::inspector::format_tile};
use serde_json::{Value, json};

pub(super) fn analyze(snapshot: &Snapshot) -> Result<Value, ToolError> {
    let mut opponents = serde_json::Map::new();
    let mut safe_by_player = [[false; 34]; 4];
    for (index, public) in snapshot.position.players.iter().enumerate() {
        if index == snapshot.player {
            continue;
        }
        let mut river = [false; 34];
        for discard in &public.discards {
            river[parse_tile(&discard.tile)
                .ok_or_else(bad_position)?
                .kind()
                .as_u8() as usize] = true;
        }
        let mut passed = serde_json::Map::<String, Value>::new();
        if public.riichi == "accepted"
            && let Some(history) = &snapshot.position.history
            && let Some(accept) = history
                .iter()
                .position(|e| e.player as usize == index && e.kind == "riichi_accepted")
        {
            for (i, event) in history.iter().enumerate().skip(accept + 1) {
                // 末尾仍开放的荣和窗口不能当作“已经通过”。立直受理本身不推进此窗口。
                if event.kind != "discard"
                    || !history[i + 1..]
                        .iter()
                        .any(|e| matches!(e.kind.as_str(), "draw" | "call" | "discard"))
                {
                    continue;
                }
                let tile = event
                    .tile
                    .as_deref()
                    .and_then(parse_tile)
                    .ok_or_else(bad_position)?;
                passed.insert(
                    kind_name(tile.kind()),
                    json!({"event_index":event.event_index,"player":event.player}),
                );
            }
        }
        let safe = &mut safe_by_player[index];
        for kind in 0..34u8 {
            let name = kind_name(Tile::new(kind).unwrap().kind());
            safe[kind as usize] = river[kind as usize] || passed.contains_key(&name);
        }
        let mut tiles = serde_json::Map::new();
        for &tile in snapshot.hand.concealed() {
            let kind = tile.kind().as_u8();
            let suji_sources: Vec<_> = (0..27u8)
                .filter(|&other| {
                    kind < 27
                        && kind / 9 == other / 9
                        && kind.abs_diff(other) == 3
                        && safe[other as usize]
                })
                .map(|k| kind_name(Tile::new(k).unwrap().kind()))
                .collect();
            let missing_suji_endpoints: Vec<_> = (0..27u8)
                .filter(|&other| {
                    kind < 27
                        && kind / 9 == other / 9
                        && kind.abs_diff(other) == 3
                        && !safe[other as usize]
                })
                .map(|k| kind_name(Tile::new(k).unwrap().kind()))
                .collect();
            let adjacent_exhausted: Vec<_> = (0..27u8)
                .filter(|&other| {
                    kind < 27
                        && kind / 9 == other / 9
                        && (1..=2).contains(&kind.abs_diff(other))
                        && snapshot.unseen[other as usize] == 0
                })
                .map(|k| kind_name(Tile::new(k).unwrap().kind()))
                .collect();
            let remaining_safe_copies = snapshot
                .hand
                .concealed()
                .iter()
                .filter(|t| safe[t.kind().as_u8() as usize])
                .count()
                - usize::from(safe[kind as usize]);
            tiles.insert(format_tile(tile),json!({
                "in_opponent_river":river[kind as usize],"passed_after_riichi":passed.get(&kind_name(tile.kind())),
                "known_safe_against_ron_from_this_player":safe[kind as usize],
                "suji_sources":suji_sources,"nearby_fully_visible_kinds":adjacent_exhausted,
                "suji_covers_both_possible_ryanmen_sides":kind<27 && missing_suji_endpoints.is_empty(),
                "missing_suji_endpoints":missing_suji_endpoints,
                "visible_copies":4-snapshot.unseen[kind as usize],"unseen_copies":snapshot.unseen[kind as usize],
                "remaining_known_safe_copies_after_discard":remaining_safe_copies,
            }));
        }
        opponents.insert(index.to_string(),json!({"player":index,"riichi":public.riichi,"open_meld_count":public.melds.iter().filter(|m| m.kind!="ankan").count(),"tiles":tiles}));
    }
    let safe_against_all: Vec<_> = snapshot
        .hand
        .concealed()
        .iter()
        .filter(|t| {
            (0..4)
                .filter(|&i| i != snapshot.player)
                .all(|i| safe_by_player[i][t.kind().as_u8() as usize])
        })
        .copied()
        .map(format_tile)
        .collect();
    Ok(
        json!({"opponents":opponents,"known_safe_against_all_opponents_ron":safe_against_all,
        "scope":{"history_available":snapshot.position.history.is_some(),"no_deal_in_probability":true,
            "suji_and_visible_walls_are_not_guarantees":true,"no_estimated_opponent_hand_value":true,
            "safety_is_opponent_specific":true,"false_means_not_proven_safe":true,"post_riichi_safety_uses_discard_events_only":true,"does_not_measure_future_push_fold_value":true}}),
    )
}
