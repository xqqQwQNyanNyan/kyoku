"use strict";
// stdout 只允许协议响应，第三方库的诊断输出不能包含账号或破坏消息边界。
console.log =
  console.info =
  console.warn =
  console.error =
  console.debug =
    () => {};
const { createInterface } = require("node:readline");
const { createSession } = require("./desktop-session.cjs");
const session = createSession(require("./client.cjs"));
const input = createInterface({ input: process.stdin, terminal: false });

// 父进程退出或崩溃时，管道关闭立即结束登录会话。
process.stdin.on("end", () => process.exit(0));
process.on("uncaughtException", () => process.exit(1));
process.on("unhandledRejection", () => process.exit(1));

(async () => {
  for await (const line of input) {
    let result;
    try {
      result =
        Buffer.byteLength(line) <= 8192
          ? await session(JSON.parse(line))
          : { error: "invalid_request" };
    } catch {
      result = { error: "invalid_request" };
    }
    process.stdout.write(JSON.stringify(result) + "\n");
  }
})();
