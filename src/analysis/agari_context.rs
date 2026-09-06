use crate::mahjong::round::Wind;
use crate::mahjong::tile::TileKind;

/// 和牌方式及和牌张来源。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WinMethod {
    /// 自摸。
    Tsumo(TsumoSource),
    /// 荣和。
    Ron(RonSource),
}

/// 自摸和牌张的来源及所处时机。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TsumoSource {
    /// 通常牌山摸牌，不是最后一张或未中断的首次摸牌。
    Wall,
    /// 通常牌山的最后一张，不包含岭上摸牌。
    LastWall,
    /// 杠后的岭上摸牌。
    Rinshan,
    /// 和牌者首次摸牌，且此前无人吃、碰或杠（包括暗杠）。
    ///
    /// 自风为东时表示庄家的起手和牌，用于天和；其余自风用于地和。
    FirstDraw,
}

/// 荣和的取牌来源及所处时机。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RonSource {
    /// 通常弃牌，不是牌山耗尽后的最后一张弃牌。
    Discard,
    /// 牌山耗尽后的最后一张弃牌，包括最后一次岭上摸牌后的弃牌。
    LastDiscard,
    /// 抢他家的加杠牌。
    Kakan,
    /// 抢他家的暗杠牌；当前役种检测会返回不支持错误。
    Ankan,
}

/// 和牌时已经成立的立直及一发状态。
///
/// 与领域层表示宣言／受理流程的 `RiichiState` 不同，这里保存供役种判断使用的
/// 历史结论。两立直要求首张弃牌宣言前无人吃、碰或杠；一发是否仍有效由事件层维护。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RiichiStatus {
    /// 未成立立直，包括只宣言但尚未受理。
    None,
    /// 普通立直已经成立。
    Riichi {
        /// 是否仍在未被鸣牌或杠打断的一发期间。
        ippatsu: bool,
    },
    /// 两立直已经成立。
    DoubleRiichi {
        /// 是否仍在未被鸣牌或杠打断的一发期间。
        ippatsu: bool,
    },
}

/// 与牌型拆分无关的和牌条件。
///
/// 和牌张只记录牌种，不区分赤牌。门前状态、等待型和和牌张归属应由牌型
/// 与这些条件共同确定，不在此重复保存。构造上下文不代表手牌已经合法和牌，
/// 也不校验和牌张能否归属于某个牌型或事件历史是否真实。
/// 役种检测会检查可直接识别的条件冲突，事件历史仍由调用方保证。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AgariContext {
    /// 完成手牌的和牌张牌种，自摸与荣和均需指定。
    pub winning_tile: TileKind,
    /// 自摸或荣和，以及和牌张的来源与时机。
    pub win_method: WinMethod,
    /// 当前局的场风。
    pub round_wind: Wind,
    /// 和牌者的自风，可以与场风相同。
    pub seat_wind: Wind,
    /// 已成立的立直类型及一发状态。
    pub riichi: RiichiStatus,
}
