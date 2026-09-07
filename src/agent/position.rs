//! 所有局面工具共用同一份可见快照，并在计算前校验牌张和玩家。

use crate::{
    analysis::visible_hand::{unseen_tiles, valid_meld},
    mahjong::{
        hand::Hand,
        meld::Meld,
        player_index::PlayerIndex,
        tile::{Tile, TileKind},
    },
    replay::inspector::format_tile,
};
use serde::Deserialize;
use serde_json::Value;

pub(super) type ToolError = (&'static str, String);

#[derive(Deserialize)]
pub(super) struct Position {
    pub concealed: Vec<String>,
    pub dora_indicators: Vec<String>,
    pub players: Vec<PublicPlayer>,
    pub remaining_draws: u8,
    pub dealer: u8,
    pub round: Round,
    pub honba: u8,
    pub riichi_sticks: u8,
    pub phase: Value,
    #[serde(default)]
    pub history: Option<Vec<HistoryEvent>>,
}

#[derive(Deserialize)]
pub(super) struct PublicPlayer {
    pub player: u8,
    pub riichi: String,
    pub score: i32,
    pub melds: Vec<PublicMeld>,
    pub discards: Vec<PublicDiscard>,
}

#[derive(Deserialize)]
pub(super) struct PublicDiscard {
    pub tile: String,
    pub called: bool,
}

#[derive(Deserialize)]
pub(super) struct PublicMeld {
    pub kind: String,
    pub tiles: Vec<String>,
    pub called: Option<String>,
    pub from: Option<u8>,
}

#[derive(Deserialize)]
pub(super) struct Round {
    pub wind: String,
    pub number: u8,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct HistoryEvent {
    pub event_index: usize,
    pub player: u8,
    pub kind: String,
    pub tile: Option<String>,
}

pub(super) struct Snapshot {
    pub player: usize,
    pub position: Position,
    pub hand: Hand,
    pub additional_visible: Vec<Tile>,
    pub unseen: [u8; 34],
}

impl Snapshot {
    pub fn read(evidence: &Value) -> Result<Self, ToolError> {
        let player = evidence["player"]
            .as_u64()
            .filter(|&p| p < 4)
            .ok_or_else(bad_position)? as usize;
        let position: Position =
            serde_json::from_value(evidence["position"].clone()).map_err(|_| bad_position())?;
        if position.players.len() != 4
            || position.dealer >= 4
            || !(1..=4).contains(&position.round.number)
            || position.dealer + 1 != position.round.number
            || !matches!(position.round.wind.as_str(), "E" | "S" | "W" | "N")
            || position.players.iter().enumerate().any(|(i, p)| {
                p.player as usize != i
                    || !matches!(p.riichi.as_str(), "not_declared" | "declared" | "accepted")
            })
        {
            return Err(bad_position());
        }
        for (index, player) in position.players.iter().enumerate() {
            for meld in &player.melds {
                let parsed = parse_meld(meld)?;
                if !valid_meld(&parsed)
                    || parsed.from().is_some_and(|p| p.get_id() as usize == index)
                {
                    return Err(bad_position());
                }
            }
        }
        let hand = Hand::new(
            parse_tiles(&position.concealed)?,
            position.players[player]
                .melds
                .iter()
                .map(parse_meld)
                .collect::<Result<_, _>>()?,
        )
        .map_err(|_| bad_position())?;
        let mut additional_visible = parse_tiles(&position.dora_indicators)?;
        for (index, public) in position.players.iter().enumerate() {
            for discard in &public.discards {
                let tile = parse_tile(&discard.tile).ok_or_else(bad_position)?;
                if !discard.called {
                    additional_visible.push(tile);
                }
            }
            if index != player {
                for meld in &public.melds {
                    additional_visible.extend_from_slice(parse_meld(meld)?.tiles());
                }
            }
        }
        let unseen = unseen_tiles(&hand, &additional_visible)
            .map_err(|e| ("invalid_position", format!("可见牌校验失败：{e:?}")))?;
        if let Some(history) = &position.history {
            let end = evidence["event_index"].as_u64().ok_or_else(bad_position)? as usize;
            let mut last = None;
            let mut rivers: [Vec<&str>; 4] = std::array::from_fn(|_| vec![]);
            for event in history {
                if event.player >= 4
                    || event.event_index > end
                    || last.is_some_and(|last| event.event_index <= last)
                {
                    return Err(bad_position());
                }
                last = Some(event.event_index);
                match event.kind.as_str() {
                    "discard" => {
                        let tile = event
                            .tile
                            .as_deref()
                            .filter(|tile| parse_tile(tile).is_some())
                            .ok_or_else(bad_position)?;
                        rivers[event.player as usize].push(tile);
                    }
                    "draw" | "call" | "riichi_declared" | "riichi_accepted"
                        if event.tile.is_none() => {}
                    _ => return Err(bad_position()),
                }
            }
            for (i, river) in rivers.iter().enumerate() {
                if *river
                    != position.players[i]
                        .discards
                        .iter()
                        .map(|d| d.tile.as_str())
                        .collect::<Vec<_>>()
                {
                    return Err(bad_position());
                }
            }
            for (i, player) in position.players.iter().enumerate() {
                let declared = history
                    .iter()
                    .position(|e| e.player as usize == i && e.kind == "riichi_declared");
                let accepted = history
                    .iter()
                    .position(|e| e.player as usize == i && e.kind == "riichi_accepted");
                let consistent = match player.riichi.as_str() {
                    "not_declared" => declared.is_none() && accepted.is_none(),
                    "declared" => declared.is_some() && accepted.is_none(),
                    "accepted" => matches!((declared,accepted),(Some(d),Some(a)) if d<a),
                    _ => false,
                };
                if !consistent {
                    return Err(bad_position());
                }
            }
        }
        Ok(Self {
            player,
            position,
            hand,
            additional_visible,
            unseen,
        })
    }

    pub fn require_discard(&self, evidence: &Value, name: &str) -> Result<Tile, ToolError> {
        if !matches!(
            self.position.phase["kind"].as_str(),
            Some("after_draw" | "after_call")
        ) || self.position.phase["player"] != self.player
        {
            return Err(("unsupported_state", "当前不是所选玩家的切牌时刻。".into()));
        }
        if !evidence["discards"]
            .as_array()
            .is_some_and(|ds| ds.iter().any(|d| d["discard"] == name))
        {
            return Err((
                "candidate_not_provided",
                "请使用当前 discards 已提供的切牌候选。".into(),
            ));
        }
        parse_tile(name)
            .filter(|t| self.hand.concealed().contains(t))
            .ok_or_else(bad_position)
    }
}

pub(super) fn parse_tile(name: &str) -> Option<Tile> {
    (0..=Tile::MAX_VALUE)
        .filter_map(Tile::new)
        .find(|&tile| format_tile(tile) == name)
}

pub(super) fn kind_name(kind: TileKind) -> String {
    format_tile(
        Tile::try_from(kind.as_u8()).unwrap_or_else(|_| unreachable!("牌种必然是合法普通牌")),
    )
}

pub(super) fn parse_tiles(names: &[String]) -> Result<Vec<Tile>, ToolError> {
    names
        .iter()
        .map(|name| parse_tile(name).ok_or_else(bad_position))
        .collect()
}

pub(super) fn parse_meld(meld: &PublicMeld) -> Result<Meld, ToolError> {
    let mut tiles = parse_tiles(&meld.tiles)?;
    tiles.sort_unstable();
    if meld.kind == "ankan" {
        if meld.called.is_some() || meld.from.is_some() {
            return Err(bad_position());
        }
        return Ok(Meld::Ankan {
            tiles: tiles.try_into().map_err(|_| bad_position())?,
        });
    }
    let called = meld
        .called
        .as_deref()
        .and_then(parse_tile)
        .ok_or_else(bad_position)?;
    let from = PlayerIndex::new(meld.from.ok_or_else(bad_position)?).ok_or_else(bad_position)?;
    Ok(match meld.kind.as_str() {
        "chi" => Meld::Chi {
            tiles: tiles.try_into().map_err(|_| bad_position())?,
            called,
            from,
        },
        "pon" => Meld::Pon {
            tiles: tiles.try_into().map_err(|_| bad_position())?,
            called,
            from,
        },
        "daiminkan" => Meld::Daiminkan {
            tiles: tiles.try_into().map_err(|_| bad_position())?,
            called,
            from,
        },
        "kakan" => Meld::Kakan {
            tiles: tiles.try_into().map_err(|_| bad_position())?,
            called,
            from,
        },
        _ => return Err(bad_position()),
    })
}

pub(super) fn bad_position() -> ToolError {
    (
        "invalid_position",
        "局面快照缺少有效的手牌或公开信息。".into(),
    )
}
