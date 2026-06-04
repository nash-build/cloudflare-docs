// Screenshot reading: find an image in Google Drive by name and have Claude
// describe it. Used by Paige's `read_screenshot` client tool (via /screenshot).
//
// Required Worker secrets/vars (see wrangler.toml):
//   GDRIVE_CLIENT_ID, GDRIVE_CLIENT_SECRET, GDRIVE_REFRESH_TOKEN  (OAuth)
//   GDRIVE_FOLDER_ID         (optional: restrict to the screenshots folder)
//   ANTHROPIC_API_KEY        (Claude vision)
//   CLAUDE_VISION_MODEL      (optional: defaults to claude-sonnet-4-6)

const SUPPORTED = {
  'image/png': 'image/png',
  'image/jpeg': 'image/jpeg',
  'image/jpg': 'image/jpeg',
  'image/gif': 'image/gif',
  'image/webp': 'image/webp',
};

// Escape a value for a Drive `q` string literal.
const escapeQ = (s) => String(s).replace(/\\/g, '\\\\').replace(/'/g, "\\'");

async function driveAccessToken(env) {
  const res = await fetch('https://oauth2.googleapis.com/token', {
    method: 'POST',
    headers: { 'content-type': 'application/x-www-form-urlencoded' },
    body: new URLSearchParams({
      client_id: env.GDRIVE_CLIENT_ID,
      client_secret: env.GDRIVE_CLIENT_SECRET,
      refresh_token: env.GDRIVE_REFRESH_TOKEN,
      grant_type: 'refresh_token',
    }),
  });
  if (!res.ok) throw new Error('Drive auth failed (' + res.status + ')');
  return (await res.json()).access_token;
}

async function findScreenshot(env, token, query) {
  const clauses = ["mimeType contains 'image/'", 'trashed = false'];
  if (env.GDRIVE_FOLDER_ID) clauses.push(`'${escapeQ(env.GDRIVE_FOLDER_ID)}' in parents`);
  if (query && query.trim()) clauses.push(`name contains '${escapeQ(query.trim())}'`);
  const u = new URL('https://www.googleapis.com/drive/v3/files');
  u.searchParams.set('q', clauses.join(' and '));
  u.searchParams.set('orderBy', 'modifiedTime desc');
  u.searchParams.set('pageSize', '5');
  u.searchParams.set('spaces', 'drive');
  u.searchParams.set('fields', 'files(id,name,mimeType,modifiedTime)');
  const res = await fetch(u, { headers: { authorization: `Bearer ${token}` } });
  if (!res.ok) throw new Error('Drive search failed (' + res.status + ')');
  return ((await res.json()).files || [])[0] || null;
}

async function downloadBase64(token, fileId) {
  const res = await fetch(`https://www.googleapis.com/drive/v3/files/${fileId}?alt=media`, {
    headers: { authorization: `Bearer ${token}` },
  });
  if (!res.ok) throw new Error('Drive download failed (' + res.status + ')');
  const buf = new Uint8Array(await res.arrayBuffer());
  let binary = '';
  const chunk = 0x8000;
  for (let i = 0; i < buf.length; i += chunk) {
    binary += String.fromCharCode.apply(null, buf.subarray(i, i + chunk));
  }
  return btoa(binary);
}

async function describeWithClaude(env, base64, mediaType, name) {
  const model = env.CLAUDE_VISION_MODEL || 'claude-sonnet-4-6';
  const res = await fetch('https://api.anthropic.com/v1/messages', {
    method: 'POST',
    headers: {
      'x-api-key': env.ANTHROPIC_API_KEY,
      'anthropic-version': '2023-06-01',
      'content-type': 'application/json',
    },
    body: JSON.stringify({
      model,
      max_tokens: 300,
      messages: [
        {
          role: 'user',
          content: [
            { type: 'image', source: { type: 'base64', media_type: mediaType, data: base64 } },
            {
              type: 'text',
              text:
                `This is a screenshot named "${name}". In 1-2 short, spoken-style sentences, ` +
                `describe what it shows and any task or action it implies. Be concise and natural, ` +
                `as if telling someone what's on their screen.`,
            },
          ],
        },
      ],
    }),
  });
  if (!res.ok) {
    const t = await res.text().catch(() => '');
    throw new Error('Claude vision failed (' + res.status + ') ' + t.slice(0, 160));
  }
  const data = await res.json();
  return (data.content || [])
    .filter((b) => b.type === 'text')
    .map((b) => b.text)
    .join(' ')
    .trim();
}

// Returns { name, modifiedTime, description } or { error, status }.
export async function readScreenshot(env, query) {
  if (!env.GDRIVE_CLIENT_ID || !env.GDRIVE_CLIENT_SECRET || !env.GDRIVE_REFRESH_TOKEN) {
    return { error: 'Google Drive is not configured on the server.', status: 501 };
  }
  if (!env.ANTHROPIC_API_KEY) {
    return { error: 'Claude vision (ANTHROPIC_API_KEY) is not configured on the server.', status: 501 };
  }
  const token = await driveAccessToken(env);
  const file = await findScreenshot(env, token, query);
  if (!file) {
    return {
      error: query ? `No screenshot matching "${query}".` : 'No screenshots found.',
      status: 404,
    };
  }
  const mediaType = SUPPORTED[file.mimeType];
  if (!mediaType) {
    return { error: `Unsupported image type ${file.mimeType}.`, status: 415, name: file.name };
  }
  const base64 = await downloadBase64(token, file.id);
  const description = await describeWithClaude(env, base64, mediaType, file.name);
  return { name: file.name, modifiedTime: file.modifiedTime, description };
}
