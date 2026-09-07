"use strict";

const { fork } = require("node:child_process");
const { ServiceError } = require("./record.cjs");

// 用独立进程隔离协议异常，超时可终止整个连接并在下一次请求重建。
function createDownloader({
  workerPath = require.resolve("./worker.cjs"),
  timeoutMs = 45000,
} = {}) {
  let worker;
  return {
    close() {
      worker?.kill();
      worker = undefined;
    },
    download(uuid) {
      worker ??= fork(workerPath, [], {
        stdio: ["ignore", "ignore", "ignore", "ipc"],
        execArgv: [],
      });
      const current = worker;
      return new Promise((resolve, reject) => {
        let settled = false;
        const finish = (error, log) => {
          if (settled) return;
          settled = true;
          clearTimeout(timer);
          current.removeAllListeners("message");
          current.removeAllListeners("exit");
          current.removeAllListeners("error");
          if (error) {
            current.kill();
            worker = undefined;
            reject(error);
          } else {
            // 空闲期间连接意外退出时，下次请求重新登录。
            current.once("exit", () => {
              if (worker === current) worker = undefined;
            });
            resolve(log);
          }
        };
        const timer = setTimeout(
          () => finish(new ServiceError("upstream_timeout", 504)),
          timeoutMs,
        );
        current.removeAllListeners("exit");
        current.once("exit", () =>
          finish(new ServiceError("upstream_unavailable", 503)),
        );
        current.once("error", () =>
          finish(new ServiceError("upstream_unavailable", 503)),
        );
        current.once("message", (message) => {
          if (message.error) {
            const status =
              message.error === "log_too_large"
                ? 413
                : message.error.startsWith("unsupported_")
                  ? 422
                  : 502;
            finish(new ServiceError(message.error, status));
          } else {
            finish(null, message.log);
          }
        });
        current.send({ uuid }, (error) => {
          if (error) finish(new ServiceError("upstream_unavailable", 503));
        });
      });
    },
  };
}
module.exports = { createDownloader };
