"use strict";

const http = require("node:http");
const { ServiceError } = require("./record.cjs");
const { createConverter } = require("./conversion.cjs");
const { createDownloader } = require("./downloader.cjs");

function createServer(downloader, options = {}) {
  const convert = createConverter(downloader, options);
  const server = http.createServer(async (request, response) => {
    response.setHeader("Content-Type", "application/json; charset=utf-8");
    response.setHeader("Cache-Control", "no-store");
    response.setHeader("X-Content-Type-Options", "nosniff");
    const reply = (status, value) => {
      response.writeHead(status);
      response.end(typeof value === "string" ? value : JSON.stringify(value));
    };
    try {
      if (request.method !== "GET")
        throw new ServiceError("method_not_allowed", 405);
      const url = new URL(request.url, "http://localhost");
      if (url.pathname === "/healthz") return reply(200, { status: "ok" });
      if (url.pathname !== "/convert") throw new ServiceError("not_found", 404);
      if (
        url.searchParams.getAll("id").length !== 1 ||
        [...url.searchParams.keys()].some((k) => k !== "id")
      ) {
        throw new ServiceError("invalid_id", 400);
      }
      reply(200, await convert(url.searchParams.get("id")));
    } catch (error) {
      const known = error instanceof ServiceError;
      const status = known ? error.status : 502;
      if (status === 429 || status === 503)
        response.setHeader("Retry-After", "5");
      reply(status, {
        error: { code: known ? error.code : "upstream_failed" },
      });
    }
  });
  server.requestTimeout = 10000;
  server.headersTimeout = 10000;
  server.maxConnections = 64;
  return server;
}

if (require.main === module) {
  if (
    !(
      process.env.ACCESS_TOKEN?.trim() ||
      (process.env.MJS_USERNAME?.trim() && process.env.MJS_PASSWORD)
    ) ||
    !process.env.MJS_BASE?.trim()
  ) {
    console.error(
      "请先在 services/majsoul/.env 配置 MJS_BASE 及专用账号的用户名、密码或 ACCESS_TOKEN。",
    );
    process.exitCode = 1;
  } else {
    const downloader = createDownloader();
    const server = createServer(downloader);
    server.listen(
      Number(process.env.PORT || 2563),
      process.env.HOST || "127.0.0.1",
      () => {
        console.log(
          "雀魂牌谱服务已启动；/healthz 仅表示 HTTP 服务存活，首次导入时验证雀魂连接。",
        );
      },
    );
    server.on("error", () => {
      console.error("无法启动牌谱服务，请检查监听地址与端口。");
      process.exitCode = 1;
    });
    const close = () => {
      downloader.close();
      server.close();
      server.closeAllConnections();
    };
    process.on("SIGINT", close);
    process.on("SIGTERM", close);
  }
}
module.exports = { createServer };
