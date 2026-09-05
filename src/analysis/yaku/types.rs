use std::error::Error;
use std::fmt;

use crate::mahjong::tile::TileKind;

/// 通行四人日麻的役种，包含常见双倍役满形及流局满贯。
///
/// 不包含地方役、宝牌或累计役满；定义役种不代表已支持其向听计算或和牌判断。
/// 双倍役满形是否按双倍计分由规则决定。
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
    /// 断幺九。
    Tanyao,
    /// 混一色。
    Honitsu,
    /// 混老头。
    Honroutou,
    /// 混全带幺九。
    Chanta,
    /// 纯全带幺九。
    Junchan,
    /// 三色同顺。
    SanshokuDoujun,
    /// 三色同刻。
    SanshokuDoukou,
    /// 二杯口。
    Ryanpeikou,
    /// 小三元。
    Shousangen,
    /// 大三元。
    Daisangen,
    /// 小四喜。
    Shousuushi,
    /// 大四喜。
    Daisuushi,
    /// 字一色。
    Tsuuiisou,
    /// 清老头。
    Chinroutou,
    /// 绿一色。
    Ryuuiisou,
    /// 立直。
    Riichi,
    /// 两立直。
    DoubleRiichi,
    /// 一发。
    Ippatsu,
    /// 门前清自摸和。
    MenzenTsumo,
    /// 平和。
    Pinfu,
    /// 役牌：白。
    Haku,
    /// 役牌：发。
    Hatsu,
    /// 役牌：中。
    Chun,
    /// 役牌：场风，与自风重合时分别计役。
    Bakaze,
    /// 役牌：自风，与场风重合时分别计役。
    Jikaze,
    /// 海底摸月。
    Haitei,
    /// 河底捞鱼。
    Houtei,
    /// 岭上开花。
    RinshanKaihou,
    /// 抢杠。
    Chankan,
    /// 三暗刻。
    Sanankou,
    /// 三杠子。
    Sankantsu,
    /// 四暗刻。
    Suuankou,
    /// 四暗刻单骑。
    SuuankouTanki,
    /// 四杠子。
    Suukantsu,
    /// 九莲宝灯。
    ChuurenPoutou,
    /// 纯正九莲宝灯。
    JunseiChuurenPoutou,
    /// 国士无双十三面。
    KokushiJuusanmen,
    /// 天和。
    Tenhou,
    /// 地和。
    Chiihou,
    /// 流局满贯，属于流局时的特殊计分条件。
    NagashiMangan,
}

/// 役种距离计算无法执行的原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum YakuDistanceError {
    /// 尚未支持指定役种的距离计算，与手牌是否可达无关。
    UnsupportedYaku(Yaku),
}

impl fmt::Display for YakuDistanceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedYaku(yaku) => {
                write!(
                    formatter,
                    "distance calculation is not supported for {yaku:?}"
                )
            }
        }
    }
}

impl Error for YakuDistanceError {}

/// 役种检测无法执行的原因。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum YakuDetectionError {
    /// 上下文的和牌张与生成解释时使用的和牌张不一致。
    ContextWinningTileMismatch {
        interpretation_tile: TileKind,
        context_tile: TileKind,
    },
    /// 副露手不能成立立直。
    RiichiRequiresClosedHand,
    /// 首次摸牌和牌前不能已有副露或杠。
    FirstDrawRequiresInitialHand,
    /// 首次摸牌和牌时不可能已经立直。
    FirstDrawWithRiichi,
    /// 岭上和牌要求和牌者手中已有杠。
    RinshanRequiresKan,
    /// 自己开杠后，一发已经中断。
    IppatsuWithRinshan,
    /// 当前规则口径不支持抢暗杠。
    RobbingAnkanUnsupported,
    /// 他家碰子已经占三张，抢加杠者不能原先还持有同种牌。
    RobbedKanTileAlreadyHeld { winning_tile: TileKind },
}

impl fmt::Display for YakuDetectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ContextWinningTileMismatch {
                interpretation_tile,
                context_tile,
            } => write!(
                formatter,
                "context winning tile {} does not match interpretation winning tile {}",
                context_tile.as_u8(),
                interpretation_tile.as_u8()
            ),
            Self::RiichiRequiresClosedHand => formatter.write_str("riichi requires a closed hand"),
            Self::FirstDrawRequiresInitialHand => {
                formatter.write_str("first draw cannot follow a call or kan")
            }
            Self::FirstDrawWithRiichi => {
                formatter.write_str("riichi cannot precede the first draw")
            }
            Self::RinshanRequiresKan => {
                formatter.write_str("rinshan requires a kan in the winning hand")
            }
            Self::IppatsuWithRinshan => formatter.write_str("ippatsu cannot combine with rinshan"),
            Self::RobbingAnkanUnsupported => {
                formatter.write_str("robbing a concealed kan is not supported")
            }
            Self::RobbedKanTileAlreadyHeld { winning_tile } => write!(
                formatter,
                "winning hand already held robbed kan tile {}",
                winning_tile.as_u8()
            ),
        }
    }
}

impl Error for YakuDetectionError {}
