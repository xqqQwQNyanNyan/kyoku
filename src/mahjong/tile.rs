use std::error::Error;
use std::fmt;

/// 一张麻将牌。
///
/// `0..=33` 表示普通牌，`34..=36` 依次表示赤五万、赤五饼和赤五索。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Tile(u8);

/// 忽略赤牌差异后的牌种类。
///
/// 有效值为 `0..=33`。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TileKind(u8);

/// `Tile` 编码超出 `0..=36`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidTile(u8);

/// `TileKind` 编码超出 `0..=33`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidTileKind(u8);

impl Tile {
    pub const MAX_VALUE: u8 = 36;

    /// 从内部编码构造牌；编码无效时返回 `None`。
    pub const fn new(value: u8) -> Option<Self> {
        if value <= Self::MAX_VALUE {
            Some(Self(value))
        } else {
            None
        }
    }

    /// 返回牌的内部编码。
    pub const fn as_u8(self) -> u8 {
        self.0
    }

    /// 返回忽略赤牌差异后的牌种类。
    pub const fn kind(self) -> TileKind {
        TileKind(match self.0 {
            34 => 4,
            35 => 13,
            36 => 22,
            value => value,
        })
    }

    /// 这张牌是否为赤五。
    pub const fn is_aka(self) -> bool {
        self.0 >= 34
    }
}

impl PartialOrd for Tile {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Tile {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.kind()
            .cmp(&other.kind())
            .then_with(|| other.is_aka().cmp(&self.is_aka()))
    }
}

impl TileKind {
    pub const MAX_VALUE: u8 = 33;

    /// 从内部编码构造牌种类；编码无效时返回 `None`。
    pub const fn new(value: u8) -> Option<Self> {
        if value <= Self::MAX_VALUE {
            Some(Self(value))
        } else {
            None
        }
    }

    /// 返回牌种类的内部编码。
    pub const fn as_u8(self) -> u8 {
        self.0
    }
}

impl From<Tile> for TileKind {
    fn from(tile: Tile) -> Self {
        tile.kind()
    }
}

impl TryFrom<u8> for Tile {
    type Error = InvalidTile;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value).ok_or(InvalidTile(value))
    }
}

impl TryFrom<u8> for TileKind {
    type Error = InvalidTileKind;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value).ok_or(InvalidTileKind(value))
    }
}

impl InvalidTile {
    pub const fn value(self) -> u8 {
        self.0
    }
}

impl fmt::Display for InvalidTile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid tile value {}; expected 0..=36", self.0)
    }
}

impl Error for InvalidTile {}

impl InvalidTileKind {
    pub const fn value(self) -> u8 {
        self.0
    }
}

impl fmt::Display for InvalidTileKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid tile kind value {}; expected 0..=33",
            self.0
        )
    }
}

impl Error for InvalidTileKind {}
