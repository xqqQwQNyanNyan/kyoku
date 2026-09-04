use std::collections::VecDeque;
use std::error::Error;
use std::fmt::{self, Write};

use convlog::Event;

use crate::mahjong::meld::Meld;
use crate::mahjong::player::{Discard, PlayerState, RiichiState};
use crate::mahjong::round::{DrawSource, KanKind, RoundPhase, RoundResult, RoundState, Wind};
use crate::mahjong::tile::Tile;

use super::replayer::{ReplayError, Replayer};

const HISTORY_LIMIT: usize = 6;

/// 重放事件并输出人类可读差异的轻量检查器。
#[derive(Debug, Default)]
pub struct ReplayInspector {
    replayer: Replayer,
    output: String,
    history: VecDeque<String>,
    global_index: usize,
    kyoku_index: Option<usize>,
}

/// 检查器应用事件失败时携带的定位信息。
#[derive(Debug)]
pub struct ReplayInspectionError {
    /// 失败事件在整份牌谱中的索引。
    pub global_index: usize,
    /// 失败事件在当前一局中的索引；局外事件为 `None`。
    pub kyoku_index: Option<usize>,
    /// 格式化后的失败事件。
    pub event: String,
    /// 失败事件之前最近的事件输出。
    pub preceding_events: Vec<String>,
    /// 失败前最后一个有效的完整状态。
    pub last_valid_state: String,
    /// 回放器返回的原始错误。
    pub error: ReplayError,
}

impl ReplayInspector {
    /// 创建一个从空牌局开始的检查器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 应用一个事件，并将事件索引及状态变化追加到输出中。
    pub fn apply(&mut self, event: &Event) -> Result<(), Box<ReplayInspectionError>> {
        let is_start_kyoku = matches!(event, Event::StartKyoku { .. });
        let global_index = self.global_index;
        let kyoku_index = if is_start_kyoku {
            Some(0)
        } else {
            self.kyoku_index
        };
        let event_text = format_event(event);
        let before = self.replayer.state().cloned();
        if let Err(error) = self.replayer.apply(event) {
            return Err(Box::new(ReplayInspectionError {
                global_index,
                kyoku_index,
                event: event_text,
                preceding_events: self.history.iter().cloned().collect(),
                last_valid_state: before
                    .as_ref()
                    .map(format_state)
                    .unwrap_or_else(|| "round=<none>".to_owned()),
                error,
            }));
        }

        let after = self.replayer.state().cloned();
        let prefix = format_index(global_index, kyoku_index);
        if is_start_kyoku && let Some(state) = after.as_ref() {
            self.append_line(&format_header(state, global_index, kyoku_index));
        }

        let changes = format_changes(before.as_ref(), after.as_ref(), event);
        let mut line = format!("{prefix} {event_text}");
        for change in changes.split("; ") {
            let _ = write!(&mut line, "\n  - {change}");
        }
        if let Some(state) = after.as_ref()
            && let Some(summary) = format_settlement_summary(event, state)
        {
            let _ = write!(&mut line, "\n  - {summary}");
        }
        self.append_line(&line);
        self.history.push_back(line);
        while self.history.len() > HISTORY_LIMIT {
            self.history.pop_front();
        }

        self.global_index += 1;
        if is_start_kyoku {
            self.kyoku_index = Some(1);
        } else if let Some(index) = self.kyoku_index.as_mut() {
            *index += 1;
        }
        Ok(())
    }

    /// 返回当前检查器的差异输出。
    pub fn output(&self) -> &str {
        &self.output
    }

    /// 返回当前回放状态。
    pub fn state(&self) -> Option<&RoundState> {
        self.replayer.state()
    }

    /// 返回当前状态的完整快照；尚未开局时返回 `round=<none>`。
    pub fn full_state(&self) -> String {
        self.state()
            .map(format_state)
            .unwrap_or_else(|| "round=<none>".to_owned())
    }

    fn append_line(&mut self, line: &str) {
        self.output.push_str(line);
        self.output.push('\n');
    }
}

impl fmt::Display for ReplayInspectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            formatter,
            "replay failed at {}: {}",
            format_index(self.global_index, self.kyoku_index),
            self.event
        )?;
        writeln!(formatter, "preceding events:")?;
        for event in &self.preceding_events {
            writeln!(formatter, "  {event}")?;
        }
        writeln!(formatter, "last valid state:")?;
        writeln!(formatter, "{}", self.last_valid_state)?;
        write!(formatter, "error: {}", self.error)
    }
}

impl Error for ReplayInspectionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.error)
    }
}

/// 将领域牌格式化为麻将记法。
pub fn format_tile(tile: Tile) -> String {
    match tile.as_u8() {
        0..=8 => format!("{}m", tile.as_u8() + 1),
        9..=17 => format!("{}p", tile.as_u8() - 8),
        18..=26 => format!("{}s", tile.as_u8() - 17),
        27 => "E".to_owned(),
        28 => "S".to_owned(),
        29 => "W".to_owned(),
        30 => "N".to_owned(),
        31 => "P".to_owned(),
        32 => "F".to_owned(),
        33 => "C".to_owned(),
        34 => "5mr".to_owned(),
        35 => "5pr".to_owned(),
        36 => "5sr".to_owned(),
        value => format!("?{value}"),
    }
}

/// 将一个 MJAI 事件格式化为紧凑的可读文本。
pub fn format_event(event: &Event) -> String {
    match event {
        Event::None => "None".to_owned(),
        Event::StartGame {
            names,
            kyoku_first,
            aka_flag,
        } => format!("StartGame(names={names:?}, kyoku_first={kyoku_first}, aka={aka_flag})"),
        Event::StartKyoku {
            bakaze,
            dora_marker,
            kyoku,
            honba,
            kyotaku,
            scores,
            ..
        } => format!(
            "StartKyoku(round={}{} honba={} kyotaku={} scores={} dora={})",
            format_convlog_wind(*bakaze),
            kyoku,
            honba,
            kyotaku,
            format_scores(scores),
            format_convlog_tile(*dora_marker),
        ),
        Event::Tsumo { actor, pai } => {
            format!(
                "Tsumo({} {})",
                format_player(*actor),
                format_convlog_tile(*pai)
            )
        }
        Event::Dahai {
            actor,
            pai,
            tsumogiri,
        } => format!(
            "Dahai({} {})",
            format_player(*actor),
            format_convlog_discard(*pai, *tsumogiri, false, false),
        ),
        Event::Chi {
            actor,
            target,
            pai,
            consumed,
        } => format!(
            "Chi({} from {} called={} consumed={})",
            format_player(*actor),
            format_player(*target),
            format_convlog_tile(*pai),
            format_convlog_tiles(consumed),
        ),
        Event::Pon {
            actor,
            target,
            pai,
            consumed,
        } => format!(
            "Pon({} from {} called={} consumed={})",
            format_player(*actor),
            format_player(*target),
            format_convlog_tile(*pai),
            format_convlog_tiles(consumed),
        ),
        Event::Daiminkan {
            actor,
            target,
            pai,
            consumed,
        } => format!(
            "Daiminkan({} from {} called={} consumed={})",
            format_player(*actor),
            format_player(*target),
            format_convlog_tile(*pai),
            format_convlog_tiles(consumed),
        ),
        Event::Ankan { actor, consumed } => format!(
            "Ankan({} consumed={})",
            format_player(*actor),
            format_convlog_tiles(consumed),
        ),
        Event::Kakan {
            actor,
            pai,
            consumed,
        } => format!(
            "Kakan({} added={} consumed={})",
            format_player(*actor),
            format_convlog_tile(*pai),
            format_convlog_tiles(consumed),
        ),
        Event::Dora { dora_marker } => {
            format!("Dora({})", format_convlog_tile(*dora_marker))
        }
        Event::Reach { actor } => format!("Reach({})", format_player(*actor)),
        Event::ReachAccepted { actor } => format!("ReachAccepted({})", format_player(*actor)),
        Event::Hora {
            actor,
            target,
            deltas,
            ura_markers,
        } => format!(
            "Hora({} target={} deltas={} ura={})",
            format_player(*actor),
            format_player(*target),
            format_optional_deltas(*deltas),
            ura_markers
                .as_deref()
                .map(format_convlog_slice)
                .unwrap_or_else(|| "<none>".to_owned()),
        ),
        Event::Ryukyoku { deltas } => {
            format!("Ryukyoku(deltas={})", format_optional_deltas(*deltas))
        }
        Event::EndKyoku => "EndKyoku".to_owned(),
        Event::EndGame => "EndGame".to_owned(),
    }
}

/// 将完整局面格式化为紧凑快照。
pub fn format_state(state: &RoundState) -> String {
    let mut output = format!(
        "round={}{} honba={} kyotaku={} phase={} dora={} remaining_draws={}\n",
        format_wind(state.round().wind()),
        state.round().number(),
        state.honba(),
        state.riichi_sticks(),
        format_phase(state.phase()),
        format_tiles(state.dora_indicators()),
        state.remaining_draws(),
    );
    for (index, player) in state.players().iter().enumerate() {
        let _ = writeln!(
            &mut output,
            "{} score={} riichi={} hand={} melds={} discards={}",
            format_player(index as u8),
            player.score(),
            format_riichi(player.riichi()),
            format_tiles(player.hand().concealed()),
            format_melds(player.hand().melds()),
            format_discards(player.discards()),
        );
    }
    output
}

/// 将当前阶段格式化为紧凑文本。
pub fn format_phase(phase: RoundPhase) -> String {
    match phase {
        RoundPhase::Initial => "Initial".to_owned(),
        RoundPhase::AfterDraw { player, source } => {
            format!(
                "AfterDraw({}, {})",
                format_player(player.get_id()),
                format_draw_source(source)
            )
        }
        RoundPhase::AfterDiscard { player } => {
            format!("AfterDiscard({})", format_player(player.get_id()))
        }
        RoundPhase::AfterCall { player } => {
            format!("AfterCall({})", format_player(player.get_id()))
        }
        RoundPhase::AfterKanDeclaration { player, kind } => format!(
            "AfterKanDeclaration({}, {})",
            format_player(player.get_id()),
            format_kan_kind(kind)
        ),
        RoundPhase::AwaitingEnd(result) => format!("AwaitingEnd({})", format_round_result(result)),
        RoundPhase::Ended(result) => format!("Ended({})", format_round_result(result)),
    }
}

fn format_header(state: &RoundState, global_index: usize, kyoku_index: Option<usize>) -> String {
    format!(
        "========== KYOKU {}{} {} | honba={} dealer={} kyotaku={} scores={} dora={} ==========",
        format_wind(state.round().wind()),
        state.round().number(),
        format_index(global_index, kyoku_index),
        state.honba(),
        format_player(state.round().dealer().get_id()),
        state.riichi_sticks(),
        format_scores(&state_scores(state)),
        format_tiles(state.dora_indicators()),
    )
}

fn format_index(global_index: usize, kyoku_index: Option<usize>) -> String {
    match kyoku_index {
        Some(kyoku_index) => format!("[G{global_index:03} K{kyoku_index:03}]"),
        None => format!("[G{global_index:03} K---]"),
    }
}

fn format_changes(
    before: Option<&RoundState>,
    after: Option<&RoundState>,
    event: &Event,
) -> String {
    if matches!(event, Event::StartGame { .. } | Event::EndGame) {
        return "no state change".to_owned();
    }
    if matches!(event, Event::StartKyoku { .. }) {
        return "round started".to_owned();
    }
    let (Some(before), Some(after)) = (before, after) else {
        return if after.is_some() {
            "round started".to_owned()
        } else {
            "no state change".to_owned()
        };
    };

    let mut changes = Vec::new();
    if before.phase() != after.phase() {
        changes.push(format!(
            "phase {} -> {}",
            format_phase(before.phase()),
            format_phase(after.phase())
        ));
    }
    if before.remaining_draws() != after.remaining_draws() {
        changes.push(format!(
            "draws {} -> {}",
            before.remaining_draws(),
            after.remaining_draws()
        ));
    }
    if before.dora_indicators() != after.dora_indicators() {
        changes.push(format!("dora {}", format_tiles(after.dora_indicators())));
    }
    if before.honba() != after.honba() {
        changes.push(format!("honba {} -> {}", before.honba(), after.honba()));
    }
    if before.riichi_sticks() != after.riichi_sticks() {
        changes.push(format!(
            "kyotaku {} -> {}",
            before.riichi_sticks(),
            after.riichi_sticks()
        ));
    }

    for (index, (before_player, after_player)) in
        before.players().iter().zip(after.players()).enumerate()
    {
        let player = format_player(index as u8);
        append_player_changes(&mut changes, &player, before_player, after_player);
    }

    if changes.is_empty() {
        "no state change".to_owned()
    } else {
        changes.join("; ")
    }
}

fn append_player_changes(
    changes: &mut Vec<String>,
    player: &str,
    before: &PlayerState,
    after: &PlayerState,
) {
    if before.score() != after.score() {
        changes.push(format!(
            "{player} score {} -> {}",
            before.score(),
            after.score()
        ));
    }
    if before.riichi() != after.riichi() {
        changes.push(format!(
            "{player} riichi {} -> {}",
            format_riichi(before.riichi()),
            format_riichi(after.riichi())
        ));
    }
    if before.hand().concealed() != after.hand().concealed() {
        let (added, removed) = tile_delta(before.hand().concealed(), after.hand().concealed());
        if !added.is_empty() {
            changes.push(format!("{player} hand +{}", format_tiles(&added)));
        }
        if !removed.is_empty() {
            changes.push(format!("{player} hand -{}", format_tiles(&removed)));
        }
    }
    if before.hand().melds() != after.hand().melds() {
        if after.hand().melds().len() > before.hand().melds().len() {
            let added = &after.hand().melds()[before.hand().melds().len()..];
            changes.push(format!("{player} meld +{}", format_melds(added)));
        } else {
            changes.push(format!(
                "{player} melds {} -> {}",
                format_melds(before.hand().melds()),
                format_melds(after.hand().melds())
            ));
        }
    }
    if before.discards().len() < after.discards().len() {
        let added = &after.discards()[before.discards().len()..];
        changes.push(format!("{player} discard +{}", format_discards(added)));
    }
    for (before_discard, after_discard) in before.discards().iter().zip(after.discards()) {
        if !before_discard.is_called() && after_discard.is_called() {
            changes.push(format!(
                "{player} discard {}",
                format_discard(*after_discard)
            ));
        }
    }
}

fn format_settlement_summary(event: &Event, state: &RoundState) -> Option<String> {
    let result = match event {
        Event::Hora { deltas, .. } => Some(("Hora", *deltas)),
        Event::Ryukyoku { deltas } => Some(("Ryuukyoku", *deltas)),
        Event::EndKyoku => match state.phase() {
            RoundPhase::Ended(result) => Some((format_round_result(result), None)),
            _ => None,
        },
        _ => None,
    }?;
    let result_name = result.0.to_owned();
    let deltas = result
        .1
        .map(|deltas| format!(" deltas={}", format_scores(&deltas)))
        .unwrap_or_default();
    Some(format!(
        "result={result_name}{deltas} scores={}",
        format_scores(&state_scores(state))
    ))
}

fn tile_delta(before: &[Tile], after: &[Tile]) -> (Vec<Tile>, Vec<Tile>) {
    let mut remaining_after = after.to_vec();
    let mut removed = Vec::new();
    for tile in before {
        if let Some(index) = remaining_after
            .iter()
            .position(|candidate| candidate == tile)
        {
            remaining_after.remove(index);
        } else {
            removed.push(*tile);
        }
    }
    (remaining_after, removed)
}

fn format_tiles(tiles: &[Tile]) -> String {
    let mut output = String::from("[");
    for (index, tile) in tiles.iter().enumerate() {
        if index > 0 {
            output.push(' ');
        }
        output.push_str(&format_tile(*tile));
    }
    output.push(']');
    output
}

fn format_melds(melds: &[Meld]) -> String {
    let mut output = String::from("[");
    for (index, meld) in melds.iter().enumerate() {
        if index > 0 {
            output.push(' ');
        }
        output.push_str(&format_meld(*meld));
    }
    output.push(']');
    output
}

fn format_meld(meld: Meld) -> String {
    let kind = match meld {
        Meld::Chi { .. } => "chi",
        Meld::Pon { .. } => "pon",
        Meld::Daiminkan { .. } => "daiminkan",
        Meld::Ankan { .. } => "ankan",
        Meld::Kakan { .. } => "kakan",
    };
    let mut output = format!("{kind}{}", format_tiles(meld.tiles()));
    if let Some(called) = meld.called() {
        let _ = write!(&mut output, "(called={})", format_tile(called));
    }
    if let Some(from) = meld.from() {
        let _ = write!(&mut output, "<{}", format_player(from.get_id()));
    }
    output
}

fn format_discards(discards: &[Discard]) -> String {
    let mut output = String::from("[");
    for (index, discard) in discards.iter().enumerate() {
        if index > 0 {
            output.push(' ');
        }
        output.push_str(&format_discard(*discard));
    }
    output.push(']');
    output
}

fn format_discard(discard: Discard) -> String {
    let mut annotations = Vec::new();
    if discard.is_tsumogiri() {
        annotations.push("tsumogiri");
    }
    if discard.is_riichi() {
        annotations.push("riichi");
    }
    if discard.is_called() {
        annotations.push("called");
    }
    if annotations.is_empty() {
        format_tile(discard.tile())
    } else {
        format_tile(discard.tile()) + &format!("({})", annotations.join(","))
    }
}

fn format_convlog_discard(
    tile: convlog::Tile,
    tsumogiri: bool,
    riichi: bool,
    called: bool,
) -> String {
    let mut annotations = Vec::new();
    if tsumogiri {
        annotations.push("tsumogiri");
    }
    if riichi {
        annotations.push("riichi");
    }
    if called {
        annotations.push("called");
    }
    let tile = format_convlog_tile(tile);
    if annotations.is_empty() {
        tile
    } else {
        tile + &format!("({})", annotations.join(","))
    }
}

fn format_convlog_tile(tile: convlog::Tile) -> String {
    Tile::new(tile.as_u8())
        .map(format_tile)
        .unwrap_or_else(|| format!("?{}", tile.as_u8()))
}

fn format_convlog_tiles<const N: usize>(tiles: &[convlog::Tile; N]) -> String {
    format_convlog_slice(tiles)
}

fn format_convlog_slice(tiles: &[convlog::Tile]) -> String {
    let mut output = String::from("[");
    for (index, tile) in tiles.iter().enumerate() {
        if index > 0 {
            output.push(' ');
        }
        output.push_str(&format_convlog_tile(*tile));
    }
    output.push(']');
    output
}

fn state_scores(state: &RoundState) -> [i32; 4] {
    [
        state.players()[0].score(),
        state.players()[1].score(),
        state.players()[2].score(),
        state.players()[3].score(),
    ]
}

fn format_convlog_wind(tile: convlog::Tile) -> String {
    match tile.as_u8() {
        27 => "E".to_owned(),
        28 => "S".to_owned(),
        29 => "W".to_owned(),
        30 => "N".to_owned(),
        value => format!("?{value}"),
    }
}

fn format_scores(scores: &[i32; 4]) -> String {
    format!("[{} {} {} {}]", scores[0], scores[1], scores[2], scores[3])
}

fn format_optional_deltas(deltas: Option<[i32; 4]>) -> String {
    deltas
        .map(|deltas| format_scores(&deltas))
        .unwrap_or_else(|| "<none>".to_owned())
}

fn format_player(player: u8) -> String {
    format!("P{player}")
}

fn format_wind(wind: Wind) -> &'static str {
    match wind {
        Wind::East => "E",
        Wind::South => "S",
        Wind::West => "W",
        Wind::North => "N",
    }
}

fn format_draw_source(source: DrawSource) -> &'static str {
    match source {
        DrawSource::Wall => "Wall",
        DrawSource::Rinshan => "Rinshan",
    }
}

fn format_kan_kind(kind: KanKind) -> &'static str {
    match kind {
        KanKind::Daiminkan => "Daiminkan",
        KanKind::Ankan => "Ankan",
        KanKind::Kakan => "Kakan",
    }
}

fn format_round_result(result: RoundResult) -> &'static str {
    match result {
        RoundResult::Hora { .. } => "Hora",
        RoundResult::Ryukyoku => "Ryuukyoku",
    }
}

fn format_riichi(state: RiichiState) -> &'static str {
    match state {
        RiichiState::NotDeclared => "NotDeclared",
        RiichiState::Declared => "Declared",
        RiichiState::Accepted => "Accepted",
    }
}
