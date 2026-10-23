/** Fetch a fresh per-process token so an open tab survives server restarts.
 * @param {string} base
 * @param {typeof fetch} transport
 */
export async function writeHeaders(base = '', transport = fetch) {
  const response = await transport(base + '/api/security/token', { cache: 'no-store' });
  const body = await response.json();
  if (!response.ok || typeof body.token !== 'string' || !body.token)
    throw new Error(body.error || 'Unable to authorize dashboard write');
  return { 'X-Codex-Write-Token': body.token };
}
