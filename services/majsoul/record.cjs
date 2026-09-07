"use strict";

const { decodeLogID } = require("./vendor/tensoul/deobfuse.js");
const { toTenhou } = require("./vendor/tensoul/convert.js");

const NORMAL_ID = /^(?:\d{6}-)?[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/;
const SHARE_ID =
  /^(?:[0-9a-z]{6}-)?[0-9a-z]{8}(?:-[0-9a-z]{4}){3}-[0-9a-z]{12}(?:_a?\d{1,12})?(?:_2)?$/;
const RANKED_MODES = new Set([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 15, 16]);
const RECORD_TYPES = new Set([
  "RecordNewRound",
  "RecordDealTile",
  "RecordDiscardTile",
  "RecordChiPengGang",
  "RecordAnGangAddGang",
  "RecordHule",
  "RecordNoTile",
  "RecordLiuJu",
]);

class ServiceError extends Error {
  constructor(code, status) {
    super(code);
    this.code = code;
    this.status = status;
  }
}

function parseId(input) {
  if (typeof input !== "string" || !SHARE_ID.test(input)) {
    throw new ServiceError("invalid_id", 400);
  }
  const parts = input.split("_");
  // 匿名标记是第三段，不能把普通玩家编号 2 当成匿名标记。
  const anonymous = parts.length === 3 && parts[2] === "2";
  if (parts.length === 3 && !/^a?\d{1,12}$/.test(parts[1])) {
    throw new ServiceError("invalid_id", 400);
  }
  const uuid = anonymous ? decodeLogID(parts[0]) : parts[0];
  if (!NORMAL_ID.test(uuid)) throw new ServiceError("invalid_id", 400);
  return { uuid, anonymous };
}

function convert(record) {
  const mode = record.head?.config;
  if (
    record.head?.result?.players?.length !== 4 ||
    !RANKED_MODES.has(mode?.meta?.mode_id) ||
    ![1, 2].includes(mode?.mode?.mode)
  ) {
    throw new ServiceError("unsupported_rules", 422);
  }
  if (
    !Array.isArray(record.data) ||
    record.data.length === 0 ||
    record.data.some((item) => !RECORD_TYPES.has(item.constructor.name))
  ) {
    throw new ServiceError("unsupported_record", 422);
  }
  for (const event of record.data) {
    if (
      (event.constructor.name === "RecordChiPengGang" &&
        ![0, 1, 2].includes(event.type)) ||
      (event.constructor.name === "RecordAnGangAddGang" &&
        ![2, 3].includes(event.type)) ||
      (event.constructor.name === "RecordLiuJu" &&
        ![1, 2, 3, 4].includes(event.type))
    ) {
      throw new ServiceError("unsupported_record", 422);
    }
    // 上游转换器只读 is_liqi，双立直也必须保留立直宣言。
    if (event.constructor.name === "RecordDiscardTile" && event.is_wliqi)
      event.is_liqi = true;
  }
  const log = toTenhou(record);
  if (!log.log?.length || log.name?.length !== 4 || log.ratingc !== "PF4") {
    throw new ServiceError("conversion_failed", 502);
  }
  // 只返回本地回放需要的字段，不附带账号资料或原始服务响应。
  return {
    ver: log.ver,
    name: log.name,
    rule: log.rule,
    ratingc: log.ratingc,
    log: log.log,
  };
}

module.exports = { ServiceError, parseId, convert };
