"use strict";

const { connect, download } = require("./client.cjs");
let connection;

// 上游诊断输出由父进程丢弃；只通过 IPC 返回白名单错误码。
process.on("message", async ({ uuid }) => {
  try {
    connection ??= connect({
      base: process.env.MJS_BASE,
      gateway: process.env.MJS_GATEWAY,
      routeId: process.env.MJS_ROUTE_ID,
      resourceVersion: process.env.MJS_RESOURCE_VERSION,
      token: process.env.ACCESS_TOKEN,
      username: process.env.MJS_USERNAME,
      password: process.env.MJS_PASSWORD,
      loginType: Number(process.env.MJS_LOGIN_TYPE || 10),
    });
    process.send({ log: await download(await connection, uuid) });
  } catch (error) {
    const code = [
      "unsupported_rules",
      "unsupported_record",
      "conversion_failed",
      "log_too_large",
    ].includes(error.code)
      ? error.code
      : "upstream_failed";
    process.send({ error: code });
  }
});
process.on("disconnect", () => process.exit(0));
