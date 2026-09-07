"use strict";
const { createConverter } = require("./conversion.cjs");

// 不读取环境中的账号、网关或代理服务地址；个人账号只用于已实测的官方国际中文服。
function createSession(client) {
  let convert;
  let loginAttempted = false;
  return async (request) => {
    if (request?.action === "login") {
      if (loginAttempted) return { error: "already_attempted" };
      if (request.accept_risk !== true) return { error: "risk_not_accepted" };
      if (
        typeof request.username !== "string" ||
        !request.username.trim() ||
        Buffer.byteLength(request.username) > 256 ||
        typeof request.password !== "string" ||
        !request.password ||
        Buffer.byteLength(request.password) > 1024
      )
        return { error: "invalid_credentials" };
      loginAttempted = true;
      try {
        const connection = await client.connect({
          base: "https://game.maj-soul.com/1",
          username: request.username.trim(),
          password: request.password,
        });
        convert = createConverter({
          download: (uuid) => client.download(connection, uuid),
        });
        return { logged_in: true };
      } catch (error) {
        return {
          error: error.error?.code === 151 ? "client_outdated" : "login_failed",
        };
      } finally {
        // 不保留密码用于自动重登；断线后需要用户重新登录。
        request.password = "";
        request.username = "";
      }
    }
    if (request?.action !== "download") return { error: "invalid_request" };
    if (!convert) return { error: "login_required" };
    try {
      return { log: JSON.parse(await convert(request.id)) };
    } catch (error) {
      const code = [
        "invalid_id",
        "busy",
        "rate_limited",
        "unsupported_rules",
        "unsupported_record",
        "conversion_failed",
        "log_too_large",
      ].includes(error.code)
        ? error.code
        : "download_failed";
      return { error: code };
    }
  };
}

module.exports = { createSession };
