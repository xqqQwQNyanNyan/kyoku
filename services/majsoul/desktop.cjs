"use strict";
// stdout 只允许协议响应，第三方库的诊断输出不能包含账号或破坏消息边界。
console.log =
  console.info =
  console.warn =
  console.error =
  console.debug =
    () => {};
const { createInterface } = require("node:readline");
let stopping = false;
function stop(code) {
  if (stopping) return;
  stopping = true;
  // 先写完白名单错误再退出，否则父进程只能看到管道断开。
  process.stdout.write(JSON.stringify({ error: code }) + "\n", () =>
    process.exit(1),
  );
}
process.stdout.on("error", () => process.exit(1));
process.on("uncaughtException", () => stop("component_failed"));
process.on("unhandledRejection", () => stop("component_failed"));

let session;
try {
  const { createSession } = require("./desktop-session.cjs");
  const client = require("./client.cjs");
  session = createSession({
    connect: (config) => client.connect(config, stop),
    download: client.download,
  });
} catch {
  stop("component_start_failed");
}
const input = createInterface({ input: process.stdin, terminal: false });

// 父进程退出或崩溃时，管道关闭立即结束登录会话。
process.stdin.on("end", () => process.exit(0));

(async () => {
  for await (const line of input) {
    if (stopping) break;
    let result;
    try {
      result =
        Buffer.byteLength(line) <= 8192
          ? await session(JSON.parse(line))
          : { error: "invalid_request" };
    } catch {
      result = { error: "invalid_request" };
    }
    if (!stopping) process.stdout.write(JSON.stringify(result) + "\n");
  }
})();
