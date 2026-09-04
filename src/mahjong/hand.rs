use std::error::Error;
use std::fmt;

use super::meld::Meld;
use super::player_index::PlayerIndex;
use super::tile::Tile;

/// 一名玩家当前持有的手牌。
///
/// 暗牌始终按 [`Tile`] 的顺序排列。手牌的等效张数必须为 13 或 14；每个
/// 副露按三张计算，因此杠的第四张牌不增加等效张数。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hand {
    concealed: Vec<Tile>,
    melds: Vec<Meld>,
}

/// 手牌的等效张数不是 13 或 14。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InvalidHandSize {
    concealed_count: usize,
    meld_count: usize,
}

/// 修改手牌时无法保持手牌不变量。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandMutationError {
    /// 修改后的等效张数无效。
    InvalidSize(InvalidHandSize),
    /// 暗牌中不存在要移除的牌。
    TileNotFound { tile: Tile },
    /// 三张牌不能组成合法的顺子。
    InvalidChi { tiles: [Tile; 3] },
    /// 三张牌不能组成合法的刻子。
    InvalidPon { tiles: [Tile; 3] },
    /// 四张牌不能组成合法的大明杠。
    InvalidDaiminkan { tiles: [Tile; 4] },
    /// 四张牌不能组成合法的暗杠。
    InvalidAnkan { tiles: [Tile; 4] },
    /// 新增牌和原碰子不能组成合法的加杠。
    InvalidKakan { tiles: [Tile; 4] },
    /// 加杠事件指定的原碰子不存在。
    PonNotFound { tiles: [Tile; 3] },
}

impl Hand {
    pub const MIN_TILE_COUNT: usize = 13;
    pub const MAX_TILE_COUNT: usize = 14;
    pub const TILES_PER_MELD: usize = 3;

    /// 构造手牌，并按领域顺序整理暗牌。
    ///
    /// 除等效张数外，不校验牌的副本数、面子内容或其他规则合法性。
    pub fn new(mut concealed: Vec<Tile>, melds: Vec<Meld>) -> Result<Self, InvalidHandSize> {
        validate_size(concealed.len(), melds.len())?;

        concealed.sort_unstable();
        Ok(Self { concealed, melds })
    }

    /// 将摸到的牌加入暗牌，并保持领域顺序。
    pub fn draw(&mut self, tile: Tile) -> Result<(), HandMutationError> {
        validate_size(self.concealed.len() + 1, self.melds.len())
            .map_err(HandMutationError::InvalidSize)?;
        self.concealed.push(tile);
        self.concealed.sort_unstable();
        Ok(())
    }

    /// 从暗牌中打出一张牌。
    pub fn discard(&mut self, tile: Tile) -> Result<(), HandMutationError> {
        validate_size(self.concealed.len() - 1, self.melds.len())
            .map_err(HandMutationError::InvalidSize)?;
        let position = self
            .concealed
            .iter()
            .position(|held| *held == tile)
            .ok_or(HandMutationError::TileNotFound { tile })?;
        self.concealed.remove(position);
        Ok(())
    }

    /// 使用两张暗牌和一张他家弃牌组成顺子。
    pub fn chi(
        &mut self,
        called: Tile,
        from: PlayerIndex,
        consumed: [Tile; 2],
    ) -> Result<(), HandMutationError> {
        let mut tiles = [called, consumed[0], consumed[1]];
        tiles.sort_unstable();
        if !is_chi(tiles) {
            return Err(HandMutationError::InvalidChi { tiles });
        }

        self.add_open_meld(
            consumed,
            Meld::Chi {
                tiles,
                called,
                from,
            },
        )
    }

    /// 使用两张暗牌和一张他家弃牌组成刻子。
    pub fn pon(
        &mut self,
        called: Tile,
        from: PlayerIndex,
        consumed: [Tile; 2],
    ) -> Result<(), HandMutationError> {
        let mut tiles = [called, consumed[0], consumed[1]];
        tiles.sort_unstable();
        if !tiles.iter().all(|tile| tile.kind() == called.kind()) {
            return Err(HandMutationError::InvalidPon { tiles });
        }

        self.add_open_meld(
            consumed,
            Meld::Pon {
                tiles,
                called,
                from,
            },
        )
    }

    /// 使用三张暗牌和一张他家弃牌组成大明杠。
    pub fn daiminkan(
        &mut self,
        called: Tile,
        from: PlayerIndex,
        consumed: [Tile; 3],
    ) -> Result<(), HandMutationError> {
        let mut tiles = [called, consumed[0], consumed[1], consumed[2]];
        tiles.sort_unstable();
        if !tiles.iter().all(|tile| tile.kind() == called.kind()) {
            return Err(HandMutationError::InvalidDaiminkan { tiles });
        }

        self.add_meld(
            consumed,
            Meld::Daiminkan {
                tiles,
                called,
                from,
            },
        )
    }

    /// 使用四张暗牌组成暗杠。
    pub fn ankan(&mut self, consumed: [Tile; 4]) -> Result<(), HandMutationError> {
        let mut tiles = consumed;
        tiles.sort_unstable();
        if !tiles.iter().all(|tile| tile.kind() == tiles[0].kind()) {
            return Err(HandMutationError::InvalidAnkan { tiles });
        }

        self.add_meld(consumed, Meld::Ankan { tiles })
    }

    /// 用一张暗牌将已有碰子原地升级为加杠。
    pub fn kakan(&mut self, added: Tile, mut consumed: [Tile; 3]) -> Result<(), HandMutationError> {
        consumed.sort_unstable();
        let mut tiles = [added, consumed[0], consumed[1], consumed[2]];
        tiles.sort_unstable();
        if !tiles.iter().all(|tile| tile.kind() == added.kind()) {
            return Err(HandMutationError::InvalidKakan { tiles });
        }

        let meld_position = self
            .melds
            .iter()
            .position(|meld| matches!(meld, Meld::Pon { tiles, .. } if *tiles == consumed))
            .ok_or(HandMutationError::PonNotFound { tiles: consumed })?;
        let Meld::Pon { called, from, .. } = self.melds[meld_position] else {
            unreachable!("the matched meld is a pon")
        };

        let mut concealed = self.concealed.clone();
        remove_tiles(&mut concealed, [added])?;
        validate_size(concealed.len(), self.melds.len()).map_err(HandMutationError::InvalidSize)?;

        self.concealed = concealed;
        self.melds[meld_position] = Meld::Kakan {
            tiles,
            called,
            from,
        };
        Ok(())
    }

    /// 返回排好序的暗牌。
    pub fn concealed(&self) -> &[Tile] {
        &self.concealed
    }

    /// 返回已有面子，并保留构造时的顺序。
    pub fn melds(&self) -> &[Meld] {
        &self.melds
    }

    /// 返回手牌的等效张数；每个面子按三张计算。
    pub fn effective_tile_count(&self) -> usize {
        self.concealed.len() + self.melds.len() * Self::TILES_PER_MELD
    }

    fn add_open_meld(&mut self, consumed: [Tile; 2], meld: Meld) -> Result<(), HandMutationError> {
        self.add_meld(consumed, meld)
    }

    fn add_meld<const N: usize>(
        &mut self,
        consumed: [Tile; N],
        meld: Meld,
    ) -> Result<(), HandMutationError> {
        let mut concealed = self.concealed.clone();
        remove_tiles(&mut concealed, consumed)?;
        validate_size(concealed.len(), self.melds.len() + 1)
            .map_err(HandMutationError::InvalidSize)?;

        self.concealed = concealed;
        self.melds.push(meld);
        Ok(())
    }
}

impl InvalidHandSize {
    /// 返回暗牌张数。
    pub const fn concealed_count(self) -> usize {
        self.concealed_count
    }

    /// 返回面子数量。
    pub const fn meld_count(self) -> usize {
        self.meld_count
    }

    /// 返回手牌的等效张数；每个面子按三张计算。
    pub const fn effective_tile_count(self) -> usize {
        self.concealed_count + self.meld_count * Hand::TILES_PER_MELD
    }
}

impl fmt::Display for InvalidHandSize {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid hand size {}; expected 13 or 14 effective tiles ({} concealed, {} melds)",
            self.effective_tile_count(),
            self.concealed_count,
            self.meld_count
        )
    }
}

impl Error for InvalidHandSize {}

impl fmt::Display for HandMutationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSize(error) => error.fmt(formatter),
            Self::TileNotFound { tile } => {
                write!(
                    formatter,
                    "tile {} is not in the concealed hand",
                    tile.as_u8()
                )
            }
            Self::InvalidChi { tiles } => write!(
                formatter,
                "tiles {}, {}, {} do not form a chi",
                tiles[0].as_u8(),
                tiles[1].as_u8(),
                tiles[2].as_u8()
            ),
            Self::InvalidPon { tiles } => write!(
                formatter,
                "tiles {}, {}, {} do not form a pon",
                tiles[0].as_u8(),
                tiles[1].as_u8(),
                tiles[2].as_u8()
            ),
            Self::InvalidDaiminkan { tiles } => write!(
                formatter,
                "tiles {}, {}, {}, {} do not form a daiminkan",
                tiles[0].as_u8(),
                tiles[1].as_u8(),
                tiles[2].as_u8(),
                tiles[3].as_u8()
            ),
            Self::InvalidAnkan { tiles } => write!(
                formatter,
                "tiles {}, {}, {}, {} do not form an ankan",
                tiles[0].as_u8(),
                tiles[1].as_u8(),
                tiles[2].as_u8(),
                tiles[3].as_u8()
            ),
            Self::InvalidKakan { tiles } => write!(
                formatter,
                "tiles {}, {}, {}, {} do not form a kakan",
                tiles[0].as_u8(),
                tiles[1].as_u8(),
                tiles[2].as_u8(),
                tiles[3].as_u8()
            ),
            Self::PonNotFound { tiles } => write!(
                formatter,
                "pon {}, {}, {} was not found for kakan",
                tiles[0].as_u8(),
                tiles[1].as_u8(),
                tiles[2].as_u8()
            ),
        }
    }
}

impl Error for HandMutationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidSize(error) => Some(error),
            Self::TileNotFound { .. }
            | Self::InvalidChi { .. }
            | Self::InvalidPon { .. }
            | Self::InvalidDaiminkan { .. }
            | Self::InvalidAnkan { .. }
            | Self::InvalidKakan { .. }
            | Self::PonNotFound { .. } => None,
        }
    }
}

fn remove_tiles<const N: usize>(
    concealed: &mut Vec<Tile>,
    tiles: [Tile; N],
) -> Result<(), HandMutationError> {
    for tile in tiles {
        let position = concealed
            .iter()
            .position(|held| *held == tile)
            .ok_or(HandMutationError::TileNotFound { tile })?;
        concealed.remove(position);
    }
    Ok(())
}

fn is_chi(tiles: [Tile; 3]) -> bool {
    let [first, second, third] = tiles.map(|tile| tile.kind().as_u8());
    first < 27 && first / 9 == third / 9 && second == first + 1 && third == second + 1
}

fn validate_size(concealed_count: usize, meld_count: usize) -> Result<(), InvalidHandSize> {
    let effective_tile_count = concealed_count + meld_count * Hand::TILES_PER_MELD;
    if (Hand::MIN_TILE_COUNT..=Hand::MAX_TILE_COUNT).contains(&effective_tile_count) {
        Ok(())
    } else {
        Err(InvalidHandSize {
            concealed_count,
            meld_count,
        })
    }
}
