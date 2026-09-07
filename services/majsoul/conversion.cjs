"use strict";
const { ServiceError, parseId } = require("./record.cjs");

// HTTP 服务和桌面会话共用下载限制；缓存只属于当前下载账号。
function createConverter(
  downloader,
  {
    cooldownMs = 2000,
    cacheBytes = 32 * 1024 * 1024,
    cacheTtlMs = 3600000,
  } = {},
) {
  const cache = new Map();
  let usedBytes = 0;
  let busy = false;
  let nextDownload = 0;
  return async (id) => {
    const { uuid, anonymous } = parseId(id);
    const key = `${uuid}:${anonymous}`;
    const cached = cache.get(key);
    if (cached && cached.expires > Date.now()) return cached.json;
    if (busy) throw new ServiceError("busy", 503);
    if (Date.now() < nextDownload) throw new ServiceError("rate_limited", 429);
    busy = true;
    try {
      const log = await downloader.download(uuid);
      if (anonymous) log.name = ["玩家 1", "玩家 2", "玩家 3", "玩家 4"];
      const json = JSON.stringify(log);
      const bytes = Buffer.byteLength(json);
      if (bytes > 16 * 1024 * 1024)
        throw new ServiceError("log_too_large", 413);
      for (const [oldKey, entry] of cache) {
        if (
          oldKey === key ||
          entry.expires <= Date.now() ||
          usedBytes + bytes > cacheBytes
        ) {
          cache.delete(oldKey);
          usedBytes -= entry.bytes;
        }
      }
      if (bytes <= cacheBytes) {
        cache.set(key, { json, bytes, expires: Date.now() + cacheTtlMs });
        usedBytes += bytes;
      }
      return json;
    } finally {
      busy = false;
      nextDownload = Date.now() + cooldownMs;
    }
  };
}

module.exports = { createConverter };
