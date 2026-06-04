// Paige Checklist sync backend — Cloudflare Worker + KV.
//
// One shared checklist, guarded by a bearer token, acting as the source of
// truth so the Mac overlay, the mobile PWA, and any future device (the 01 /
// ring) all stay in sync. It also serves the mobile web app at "/".
//
//   GET  /list           -> { version, items }            (auth)
//   PUT  /list   {items}  -> replace whole list           (auth)
//   POST /add    {text}   -> append one item (quick capture) (auth)
//   POST /complete {id|text,done} -> toggle an item        (auth)
//   GET  /  /manifest.webmanifest /sw.js  -> mobile PWA    (public)

import { MOBILE_HTML, MANIFEST_JSON, SERVICE_WORKER_JS } from './mobile.js';
import { readScreenshot } from './screenshot.js';

const KEY = 'checklist';

const cors = () => ({
  'access-control-allow-origin': '*',
  'access-control-allow-methods': 'GET,POST,PUT,DELETE,OPTIONS',
  'access-control-allow-headers': 'authorization,content-type',
});

const json = (data, status = 200) =>
  new Response(JSON.stringify(data), {
    status,
    headers: { 'content-type': 'application/json; charset=utf-8', ...cors() },
  });

const uid = () => Date.now().toString(36) + Math.random().toString(36).slice(2, 6);

async function load(env) {
  const raw = await env.PAIGE_KV.get(KEY);
  if (!raw) return { version: 0, items: [] };
  try {
    const s = JSON.parse(raw);
    return { version: s.version || 0, items: Array.isArray(s.items) ? s.items : [] };
  } catch {
    return { version: 0, items: [] };
  }
}

async function save(env, state) {
  await env.PAIGE_KV.put(KEY, JSON.stringify(state));
  return state;
}

function authed(request, env) {
  const token = (request.headers.get('authorization') || '').replace(/^Bearer\s+/i, '');
  return env.PAIGE_TOKEN && token === env.PAIGE_TOKEN;
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    const path = url.pathname.replace(/\/+$/, '') || '/';

    if (request.method === 'OPTIONS') return new Response(null, { headers: cors() });

    // --- Public: serve the mobile PWA ---------------------------------------
    if (request.method === 'GET' && (path === '/' || path === '/index.html')) {
      return new Response(MOBILE_HTML, {
        headers: { 'content-type': 'text/html; charset=utf-8' },
      });
    }
    if (request.method === 'GET' && path === '/manifest.webmanifest') {
      return new Response(MANIFEST_JSON, {
        headers: { 'content-type': 'application/manifest+json' },
      });
    }
    if (request.method === 'GET' && path === '/sw.js') {
      return new Response(SERVICE_WORKER_JS, {
        headers: { 'content-type': 'text/javascript' },
      });
    }

    // --- API: everything below requires the token ---------------------------
    if (!authed(request, env)) return json({ error: 'unauthorized' }, 401);

    if (path === '/list' && request.method === 'GET') {
      return json(await load(env));
    }

    if (path === '/list' && request.method === 'PUT') {
      const body = await request.json().catch(() => ({}));
      const items = Array.isArray(body.items) ? body.items : [];
      return json(await save(env, { version: Date.now(), items }));
    }

    if (path === '/add' && request.method === 'POST') {
      const body = await request.json().catch(() => ({}));
      const text = (body.text || '').trim();
      if (!text) return json({ error: 'text required' }, 400);
      const state = await load(env);
      const item = {
        id: uid(),
        text,
        done: false,
        remindAt: body.remindAt || null,
        notified: false,
      };
      state.items.push(item);
      state.version = Date.now();
      await save(env, state);
      return json({ ok: true, item, version: state.version });
    }

    if (path === '/complete' && request.method === 'POST') {
      const body = await request.json().catch(() => ({}));
      const q = (body.text || '').trim().toLowerCase();
      const state = await load(env);
      const item =
        state.items.find((i) => i.id === body.id) ||
        (q && state.items.find((i) => i.text.toLowerCase().includes(q)));
      if (!item) return json({ error: 'not found' }, 404);
      item.done = body.done !== false;
      state.version = Date.now();
      await save(env, state);
      return json({ ok: true, item, version: state.version });
    }

    // Read a screenshot from Google Drive and describe it (Claude vision).
    if (path === '/screenshot' && request.method === 'GET') {
      const query = url.searchParams.get('query') || url.searchParams.get('name') || '';
      try {
        const result = await readScreenshot(env, query);
        return json(result, result.error ? result.status || 500 : 200);
      } catch (err) {
        return json({ error: String((err && err.message) || err) }, 502);
      }
    }

    return json({ error: 'not found' }, 404);
  },
};
