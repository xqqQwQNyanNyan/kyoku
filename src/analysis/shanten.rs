const TILE_KIND_COUNT: usize = 34;
const MAX_COPIES: usize = 4;
const MAX_MELDS: usize = 4;
const UNREACHABLE: u8 = u8::MAX;
const KOKUSHI_TILES: [usize; 13] = [0, 8, 9, 17, 18, 26, 27, 28, 29, 30, 31, 32, 33];

/// 计算普通型“四面子一雀头”的向听数。
///
/// `counts` 保存 34 种牌各自的数量。赤五必须事先折叠到对应的普通五中，
/// 每种牌的数量必须在 `0..=4` 范围内。
///
/// 本函数仅处理无副露手牌的普通型，不处理七对子和国士无双。
pub fn standard_shanten(counts: &[u8; TILE_KIND_COUNT]) -> i8 {
    debug_assert!(counts.iter().all(|&count| count <= MAX_COPIES as u8));

    // dp[i][a][b][p][m]：处理完前 i 种牌后，在此前顺子仍需要当前牌 a 张、
    // 下一种牌 b 张，且已选择 p 个雀头、m 个面子的情况下，最少还缺多少张牌。
    let mut dp = [[[[[UNREACHABLE; MAX_MELDS + 1]; 2]; MAX_COPIES + 1]; MAX_COPIES + 1];
        TILE_KIND_COUNT + 1];
    dp[0][0][0][0][0] = 0;

    for tile in 0..TILE_KIND_COUNT {
        for a in 0..=MAX_COPIES {
            for b in 0..=MAX_COPIES {
                for pairs in 0..=1 {
                    for melds in 0..=MAX_MELDS {
                        let missing_so_far = dp[tile][a][b][pairs][melds];
                        if missing_so_far == UNREACHABLE {
                            continue;
                        }

                        for pair in 0..=1 {
                            if pairs + pair > 1 {
                                continue;
                            }

                            for triplet in 0..=1 {
                                for sequence in 0..=max_sequences_starting_at(tile) {
                                    let next_melds = melds + triplet + sequence;
                                    let required = a + 2 * pair + 3 * triplet + sequence;

                                    if next_melds > MAX_MELDS
                                        || required > MAX_COPIES
                                        || b + sequence > MAX_COPIES
                                    {
                                        continue;
                                    }

                                    let missing_here =
                                        required.saturating_sub(counts[tile] as usize) as u8;
                                    let candidate = missing_so_far + missing_here;
                                    let next = &mut dp[tile + 1][b + sequence][sequence]
                                        [pairs + pair][next_melds];
                                    *next = (*next).min(candidate);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    dp[TILE_KIND_COUNT][0][0][1][MAX_MELDS] as i8 - 1
}

/// 计算无副露手牌的七对子向听数。
///
/// 四张相同的牌仍然只算一种对子，因为七对子必须由七种不同牌组成。
pub fn chiitoitsu_shanten(counts: &[u8; TILE_KIND_COUNT]) -> i8 {
    debug_assert!(counts.iter().all(|&count| count <= MAX_COPIES as u8));

    let pairs = counts.iter().filter(|&&count| count >= 2).count();
    let unique = counts.iter().filter(|&&count| count > 0).count();

    6 - pairs as i8 + 7usize.saturating_sub(unique) as i8
}

/// 计算无副露手牌的国士无双向听数。
pub fn kokushi_shanten(counts: &[u8; TILE_KIND_COUNT]) -> i8 {
    debug_assert!(counts.iter().all(|&count| count <= MAX_COPIES as u8));

    let unique = KOKUSHI_TILES
        .iter()
        .filter(|&&tile| counts[tile] > 0)
        .count();
    let has_pair = KOKUSHI_TILES.iter().any(|&tile| counts[tile] >= 2);

    13 - unique as i8 - i8::from(has_pair)
}

/// 返回无副露手牌在普通型、七对子和国士无双中的最小向听数。
pub fn shanten(counts: &[u8; TILE_KIND_COUNT]) -> i8 {
    standard_shanten(counts)
        .min(chiitoitsu_shanten(counts))
        .min(kokushi_shanten(counts))
}

/// 返回可以从该牌种开始的顺子数量上限。
/// 只有数牌每门花色的 1 到 7 可以作为顺子的起点。
fn max_sequences_starting_at(tile: usize) -> usize {
    if tile < 27 && tile % 9 <= 6 {
        MAX_COPIES
    } else {
        0
    }
}
