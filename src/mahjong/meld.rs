use super::player_index::PlayerIndex;
use super::tile::Tile;

/// 玩家已经组成的面子。
///
/// 所有变体的 `tiles` 均约定按 [`Tile`] 的领域顺序排列。公开的牌局状态变更应
/// 通过 [`super::hand::Hand`] 完成，由它校验吃碰牌形并维护手牌不变量。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Meld {
    Chi {
        tiles: [Tile; 3],
        called: Tile,
        from: PlayerIndex,
    },
    Pon {
        tiles: [Tile; 3],
        called: Tile,
        from: PlayerIndex,
    },
    Daiminkan {
        tiles: [Tile; 4],
        called: Tile,
        from: PlayerIndex,
    },
    Ankan {
        tiles: [Tile; 4],
    },
    Kakan {
        tiles: [Tile; 4],
        /// 原碰牌时从其他玩家处取得的牌。
        called: Tile,
        /// 原碰牌的来源玩家。
        from: PlayerIndex,
    },
}

impl Meld {
    /// 返回组成面子的全部牌。
    pub fn tiles(&self) -> &[Tile] {
        match self {
            Self::Chi { tiles, .. } | Self::Pon { tiles, .. } => tiles,
            Self::Daiminkan { tiles, .. } | Self::Ankan { tiles } | Self::Kakan { tiles, .. } => {
                tiles
            }
        }
    }

    /// 返回鸣牌时从其他玩家处取得的牌。暗杠返回 `None`。
    ///
    /// 对加杠而言，这是原碰牌时取得的牌，而不是后来用于加杠的牌。
    pub const fn called(&self) -> Option<Tile> {
        match self {
            Self::Chi { called, .. }
            | Self::Pon { called, .. }
            | Self::Daiminkan { called, .. }
            | Self::Kakan { called, .. } => Some(*called),
            Self::Ankan { .. } => None,
        }
    }

    /// 返回鸣牌的来源玩家。暗杠返回 `None`。
    ///
    /// 对加杠而言，这是原碰牌的来源玩家。
    pub const fn from(&self) -> Option<PlayerIndex> {
        match self {
            Self::Chi { from, .. }
            | Self::Pon { from, .. }
            | Self::Daiminkan { from, .. }
            | Self::Kakan { from, .. } => Some(*from),
            Self::Ankan { .. } => None,
        }
    }

    /// 该面子是否为明面子。
    pub const fn is_open(&self) -> bool {
        !matches!(self, Self::Ankan { .. })
    }

    /// 该面子是否为杠子。
    pub const fn is_kan(&self) -> bool {
        matches!(
            self,
            Self::Daiminkan { .. } | Self::Ankan { .. } | Self::Kakan { .. }
        )
    }
}
