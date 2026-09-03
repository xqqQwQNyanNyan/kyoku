# 局状态设计

`RoundId`、`RoundState` 和相关枚举定义在 `src/mahjong/round.rs`。这些类型只描述
某个事件处理完毕后的静态快照；事件合法性校验和状态转移不属于本模块。

## 局标识

`RoundId` 由场风和当前场风内的局数组成。当前模型只支持四人麻将，局数范围为
`1..=4`，并约定玩家 `0` 是东一局庄家，因此庄家索引为 `number - 1`。

```rust
pub enum Wind {
    East,
    South,
    West,
    North,
}

pub struct RoundId { /* 私有字段 */ }

pub const RoundId::MIN_NUMBER: u8 = 1;
pub const RoundId::MAX_NUMBER: u8 = 4;
pub const fn RoundId::new(wind: Wind, number: u8) -> Option<RoundId>
```

`Wind::East`、`South`、`West` 和 `North` 分别表示东、南、西、北风。
`MIN_NUMBER` 和 `MAX_NUMBER` 公开当前模型的局数边界。`new` 校验局数并构造
标识，无效时返回 `None`。

```rust
pub const fn RoundId::wind(self) -> Wind
```

返回场风。

```rust
pub const fn RoundId::number(self) -> u8
```

返回当前场风内从一开始的局数。

```rust
pub const fn RoundId::dealer(self) -> PlayerIndex
```

按照 `number - 1` 返回庄家。牌谱导入层仍需校验来源事件中的庄家是否与该结果
一致。

## 局面阶段

`RoundPhase` 表示当前快照之后允许处理哪一类动作：

```rust
pub enum RoundPhase {
    Initial,
    AfterDraw { player: PlayerIndex },
    AfterDiscard { player: PlayerIndex },
    AfterCall { player: PlayerIndex },
    AfterKanDeclaration { player: PlayerIndex, kind: KanKind },
    Ended,
}
```

- `Initial` 只在庄家第一次摸牌前出现；庄家通过 `RoundId::dealer` 取得。
- `AfterDraw` 表示 `player` 摸牌后，等待其自摸、打牌或杠。
- `AfterDiscard` 表示 `player` 打牌后，等待其他玩家响应；被打出的牌是该玩家牌河
  的最后一张。
- `AfterCall` 表示 `player` 吃或碰后，等待其强制打牌。
- `AfterKanDeclaration` 保存宣告者和杠类型，等待抢杠响应或后续杠处理。
- `Ended` 表示本局已经结束。

```rust
pub enum KanKind {
    Daiminkan,
    Ankan,
    Kakan,
}
```

`Daiminkan`、`Ankan` 和 `Kakan` 分别表示大明杠、暗杠和加杠。

```rust
pub const fn RoundPhase::player(self) -> Option<PlayerIndex>
```

返回阶段自身记录的玩家。`Initial` 的庄家通过 `RoundId::dealer` 获得，因此它和
`Ended` 都返回 `None`。

## 局状态

`RoundState` 保存四名玩家、局标识、本场数、场上立直棒、宝牌指示牌、剩余可摸牌
数量和当前阶段。

```rust
pub fn RoundState::new(
    players: [PlayerState; 4],
    round: RoundId,
    honba: u8,
    riichi_sticks: u8,
    dora_indicators: Vec<Tile>,
    remaining_draws: u8,
    phase: RoundPhase,
) -> RoundState
```

构造一个静态局面快照。它不校验各字段能否由同一段合法事件序列产生，这项工作由
后续状态转移层负责。

```rust
pub const fn RoundState::players(&self) -> &[PlayerState; 4]
```

按玩家索引顺序返回全部玩家状态。

```rust
pub fn RoundState::player(&self, player: PlayerIndex) -> &PlayerState
```

返回指定玩家的状态。

```rust
pub const fn RoundState::round(&self) -> RoundId
```

返回当前局标识。

```rust
pub const fn RoundState::honba(&self) -> u8
```

返回当前本场数。

```rust
pub const fn RoundState::riichi_sticks(&self) -> u8
```

返回当前留在场上的立直棒数量。

```rust
pub fn RoundState::dora_indicators(&self) -> &[Tile]
```

按翻开顺序返回宝牌指示牌。

```rust
pub const fn RoundState::remaining_draws(&self) -> u8
```

返回从当前时点起还能发生的摸牌次数。它不是不可见牌数量；开局通常为 `70`，
普通摸牌和岭上摸牌都会使其减一。

```rust
pub const fn RoundState::phase(&self) -> RoundPhase
```

返回当前局面阶段。

## 测试

测试及覆盖说明见 [`round-tests.md`](round-tests.md)。
