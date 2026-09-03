use std::error::Error;
use std::fmt;

/// 玩家在一局中的索引。
///
/// 有效值为 `0..=3`。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PlayerIndex(u8);

/// `PlayerIndex` 编码超出 `0..=3`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidPlayerIndex(u8);

impl PlayerIndex {
    const MAX_VALUE: u8 = 3;

    /// 从整数构造玩家索引；值无效时返回 `None`。
    pub const fn new(value: u8) -> Option<Self> {
        if value <= Self::MAX_VALUE {
            Some(Self(value))
        } else {
            None
        }
    }

    /// 返回玩家索引的整数值。
    pub const fn get_id(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for PlayerIndex {
    type Error = InvalidPlayerIndex;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value).ok_or(InvalidPlayerIndex(value))
    }
}

impl InvalidPlayerIndex {
    pub const fn value(self) -> u8 {
        self.0
    }
}

impl fmt::Display for InvalidPlayerIndex {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid player index {}; expected 0..=3", self.0)
    }
}

impl Error for InvalidPlayerIndex {}
