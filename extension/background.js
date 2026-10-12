const NATIVE_HOST = 'com.nununuuuu.conest';

const API_CANDIDATES = [];
for (let port = 17345; port <= 17349; port += 1) {
  API_CANDIDATES.push(`http://127.0.0.1:${port}`);
  API_CANDIDATES.push(`http://localhost:${port}`);
}

let nativePort = null;
let nativeSeq = 0;
const nativePending = new Map();
let localApi = null;
let nativeRepairInFlight = null;

function newClientRequestId() {
  if (globalThis.crypto?.randomUUID) return globalThis.crypto.randomUUID();
  return `${Date.now()}-${Math.random().toString(36).slice(2)}-${++nativeSeq}`;
}

function disconnectNative(error = 'Native Messaging disconnected') {
  const port = nativePort;
  nativePort = null;
  if (port) {
    try { port.disconnect(); } catch (_) {}
  }
  for (const { reject, timer } of nativePending.values()) {
    clearTimeout(timer);
    reject(new Error(error));
  }
  nativePending.clear();
}

function ensureNativePort() {
  if (nativePort) return nativePort;

  const port = chrome.runtime.connectNative(NATIVE_HOST);
  nativePort = port;

  port.onMessage.addListener((message) => {
    const requestId = message?.requestId;
    if (requestId == null) return;
    const pending = nativePending.get(String(requestId));
    if (!pending) return;
    nativePending.delete(String(requestId));
    clearTimeout(pending.timer);
    pending.resolve(message);
  });

  port.onDisconnect.addListener(() => {
    const detail = chrome.runtime.lastError?.message || 'Native Messaging host disconnected';
    if (nativePort === port) disconnectNative(detail);
  });

  return port;
}

function nativeRequest(action, payload = {}) {
  return new Promise((resolve, reject) => {
    let port;
    try {
      port = ensureNativePort();
    } catch (error) {
      reject(error);
      return;
    }

    const requestId = String(++nativeSeq);
    const timeoutMs = action === 'health' || action === 'tasks' ? 3000 : 45000;
    const timer = setTimeout(() => {
      nativePending.delete(requestId);
      reject(new Error(`Native Messaging timeout: ${action}`));
    }, timeoutMs);

    nativePending.set(requestId, { resolve, reject, timer });

    try {
      port.postMessage({ requestId, action, payload });
    } catch (error) {
      clearTimeout(timer);
      nativePending.delete(requestId);
      disconnectNative(String(error));
      reject(error);
    }
  });
}

async function getBrowserBilibiliCookie() {
  try {
    const cookies = await chrome.cookies.getAll({ domain: '.bilibili.com' });
    return cookies
      .filter(cookie => !cookie.expirationDate || cookie.expirationDate * 1000 > Date.now())
      .map(cookie => `${cookie.name}=${cookie.value}`)
      .join('; ');
  } catch (_) {
    return '';
  }
}

// Legacy localhost bridge is intentionally fallback-only. Browser code no longer
// depends on localhost when Native Messaging is registered correctly.
async function probeLocalApi(api) {
  try {
    const response = await fetch(`${api}/health?client=conest-extension`, {
      headers: { 'X-CONest-Extension': '1' },
      cache: 'no-store'
    });
    const data = await response.json().catch(() => ({}));
    return response.ok && data?.ok === true && data?.app === 'CONest';
  } catch (_) {
    return false;
  }
}

async function findLocalApi(force = false) {
  if (!force && localApi && await probeLocalApi(localApi)) return localApi;
  localApi = null;

  for (const api of API_CANDIDATES) {
    if (await probeLocalApi(api)) {
      localApi = api;
      return api;
    }
  }
  return null;
}

async function localRequest(path, options = {}) {
  let api = await findLocalApi();
  if (!api) {
    return {
      ok: false,
      status: 0,
      error: 'CONest local bridge unavailable',
      transport: 'localhost'
    };
  }

  const request = async (base) => {
    const headers = new Headers(options.headers || {});
    headers.set('X-CONest-Extension', '1');

    const sep = path.includes('?') ? '&' : '?';
    const response = await fetch(`${base}${path}${sep}client=conest-extension`, {
      ...options,
      headers,
      cache: 'no-store'
    });

    const data = await response.json().catch(() => ({}));
    return { ok: response.ok, status: response.status, data, api: base, transport: 'localhost' };
  };

  try {
    return await request(api);
  } catch (_) {
    localApi = null;
    api = await findLocalApi(true);
    if (!api) {
      return {
        ok: false,
        status: 0,
        error: 'CONest local bridge unavailable',
        transport: 'localhost'
      };
    }

    try {
      return await request(api);
    } catch (error) {
      localApi = null;
      return {
        ok: false,
        status: 0,
        error: String(error),
        transport: 'localhost'
      };
    }
  }
}

function nativeHostMissing(error) {
  return /native messaging host not found|specified native messaging host not found/i
    .test(String(error || ''));
}

async function repairNativeMessaging(nativeError, fallback) {
  if (!fallback?.ok || !nativeHostMissing(nativeError)) return;
  if (nativeRepairInFlight) return nativeRepairInFlight;

  nativeRepairInFlight = (async () => {
    const result = await localRequest('/api/native-register', { method: 'POST' });
    if (result?.ok) {
      disconnectNative('Native Messaging registration refreshed');
    }
    return result;
  })().finally(() => {
    nativeRepairInFlight = null;
  });

  return nativeRepairInFlight;
}

async function bridgeRequest(action, payload = {}) {
  let nativeError = null;

  try {
    const nativeReply = await nativeRequest(action, payload);

    // Application-level HTTP errors are real CONest responses and should be
    // preserved. status=0 means the native transport/local bridge failed.
    // Older native hosts can also reply "unsupported action" after an update;
    // in both cases use the existing localhost bridge as the compatibility path.
    const unsupportedAction =
      /unsupported action/i.test(String(nativeReply?.error || ''));
    const transportFailure =
      nativeReply?.ok === false &&
      Number(nativeReply?.status || 0) === 0 &&
      !nativeReply?.offline;

    if (nativeReply?.ok || (!unsupportedAction && !transportFailure)) {
      return nativeReply;
    }

    nativeError = nativeReply?.error || `Native Messaging failed: ${action}`;
    disconnectNative(String(nativeError));
  } catch (error) {
    nativeError = error;
  }

  // Legacy localhost is fallback-only for cases where Native Messaging itself
  // cannot be opened (old install / registry problem).
  let fallback;
  switch (action) {
    case 'health':
      fallback = await localRequest('/health');
      break;
    case 'tasks':
      fallback = await localRequest('/api/tasks');
      break;
    case 'download':
      fallback = await localRequest('/api/download', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload)
      });
      break;
    case 'start-task':
      fallback = await localRequest(`/api/start?id=${encodeURIComponent(payload.id || '')}`, {
        method: 'POST'
      });
      break;
    case 'clear-completed':
      fallback = await localRequest('/api/clear-completed', { method: 'POST' });
      break;
    case 'remove-task':
      fallback = await localRequest(`/api/remove?id=${encodeURIComponent(payload.id || '')}`, {
        method: 'POST'
      });
      break;
    case 'configure-task':
      fallback = await localRequest('/api/configure', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload)
      });
      break;
    case 'task-settings':
      fallback = await localRequest('/api/task-settings', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload)
      });
      break;
    default:
      return { ok: false, status: 0, error: `Unsupported action: ${action}` };
  }

  if (fallback?.ok && nativeError) {
    void repairNativeMessaging(nativeError, fallback);
  }

  if (!fallback?.ok && !fallback?.status && nativeError) {
    fallback.error = `Native Messaging: ${String(nativeError)}; localhost fallback: ${fallback.error || 'unavailable'}`;
  }
  return fallback;
}

chrome.runtime.onMessage.addListener((message, sender, sendResponse) => {
  if (message?.type === 'conest-health') {
    bridgeRequest('health')
      .then(sendResponse)
      .catch(err => sendResponse({ ok: false, error: String(err) }));
    return true;
  }

  if (message?.type === 'conest-tasks') {
    bridgeRequest('tasks')
      .then(sendResponse)
      .catch(err => sendResponse({ ok: false, error: String(err) }));
    return true;
  }

  if (message?.type === 'conest-download') {
    (async () => {
      const browserCookie = await getBrowserBilibiliCookie();
      const payload = {
        ...message.payload,
        request_id: newClientRequestId(),
        tab_id: Number.isInteger(sender?.tab?.id) ? sender.tab.id : null,
        ...(browserCookie ? { browser_cookie: browserCookie } : {})
      };
      return bridgeRequest('download', payload);
    })()
      .then(sendResponse)
      .catch(err => sendResponse({ ok: false, error: String(err) }));
    return true;
  }

  if (message?.type === 'conest-start-task') {
    bridgeRequest('start-task', { id: message.id })
      .then(sendResponse)
      .catch(err => sendResponse({ ok: false, error: String(err) }));
    return true;
  }

  if (message?.type === 'conest-clear-completed') {
    bridgeRequest('clear-completed')
      .then(sendResponse)
      .catch(err => sendResponse({ ok: false, error: String(err) }));
    return true;
  }

  if (message?.type === 'conest-remove-task') {
    bridgeRequest('remove-task', { id: message.id })
      .then(sendResponse)
      .catch(err => sendResponse({ ok: false, error: String(err) }));
    return true;
  }

  if (message?.type === 'conest-configure-task') {
    (async () => {
      const browserCookie = await getBrowserBilibiliCookie();
      return bridgeRequest('configure-task', {
        id: message.id,
        ...(browserCookie ? { browser_cookie: browserCookie } : {})
      });
    })()
      .then(sendResponse)
      .catch(err => sendResponse({ ok: false, error: String(err) }));
    return true;
  }
});


chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (message?.type !== 'conest-task-settings') return false;
  bridgeRequest('task-settings', {
    id: message.id,
    select: message.select
  })
    .then(sendResponse)
    .catch(err => sendResponse({ ok: false, error: String(err) }));
  return true;
});
