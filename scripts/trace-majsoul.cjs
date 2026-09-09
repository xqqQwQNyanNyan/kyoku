"use strict";
// 仅用于诊断子进程：保留原入口和消息协议，旁路记录白名单事件。
const fs = require('node:fs');
const path = require('node:path');
const { safeError } = require('./diagnose-majsoul.cjs');
const service = path.dirname(process.argv[1]);
let stage = 'component_start';
function report(event, details = {}) {
  try { fs.writeSync(2, JSON.stringify({ event, stage, ...details }) + '\n'); }
  catch { /* 父进程退出时不再写诊断。 */ }
}
process.on('uncaughtExceptionMonitor', error => report('EXCEPTION', safeError(error)));
process.on('unhandledRejection', error => report('EXCEPTION', safeError(error)));
process.on('exit', exitCode => report('COMPONENT_EXIT', { exitCode }));
const originalFetch = global.fetch;
global.fetch = async (url, options) => {
  const pathname = new URL(url).pathname;
  stage = pathname.endsWith('/version.json') ? 'https_version'
    : pathname.includes('/resversion') ? 'https_resources'
    : pathname.endsWith('/liqi.json') ? 'https_protocol'
    : pathname.endsWith('/config.json') ? 'https_config'
    : pathname.endsWith('/routes') ? 'https_routes' : 'record_body';
  try {
    const response = await originalFetch(url, options);
    report('HTTP', { status: response.status });
    return response;
  } catch (error) {
    report('HTTP_ERROR', safeError(error));
    throw error;
  }
};
const MJSoul = require(path.join(service, 'node_modules/mjsoul'));
const originalOpen = MJSoul.prototype.open;
MJSoul.prototype.open = function (...args) {
  stage = 'websocket';
  const result = originalOpen.apply(this, args);
  this.ws.prependListener('open', () => report('WS_OPEN'));
  this.ws.prependListener('error', error => report('WS_ERROR', safeError(error)));
  this.ws.prependListener('close', closeCode => report('WS_CLOSED', { closeCode }));
  return result;
};
const originalEmit = MJSoul.prototype.emit;
MJSoul.prototype.emit = function (...args) {
  if (args[0] === 'NotifyAccountLogout') report('ACCOUNT_LOGOUT');
  return originalEmit.apply(this, args);
};
const originalSend = MJSoul.prototype.send;
MJSoul.prototype.send = function (name, data, callback) {
  const phases = {
    requestConnection: 'route_handshake', login: 'account_login', loginSuccess: 'login_success',
    fetchGameRecord: 'fetch_record', heartbeat: 'heartbeat',
  };
  stage = phases[name] || 'other_rpc';
  const requestStage = stage;
  report('RPC_START');
  return originalSend.call(this, name, data, response => {
    report(response?.error ? 'RPC_FAILED' : 'RPC_OK', {
      stage: requestStage, ...(response?.error ? safeError(response) : {}),
    });
    callback(response);
  });
};
const record = require(path.join(service, 'record.cjs'));
const originalConvert = record.convert;
record.convert = function (...args) {
  stage = 'conversion';
  report('CONVERSION_START');
  const result = originalConvert.apply(this, args);
  report('CONVERSION_OK');
  return result;
};
const client = require(path.join(service, 'client.cjs'));
const originalDownload = client.download;
client.download = async function (...args) {
  try { return await originalDownload.apply(this, args); }
  catch (error) { report('DOWNLOAD_FAILED', safeError(error)); throw error; }
};
