import { describe, expect, it } from 'vitest';
import { formatBackendOfflineMessage } from './use-backend-status';

describe('formatBackendOfflineMessage', () => {
  it('points users to the chat launcher when the backend was never reached', () => {
    const message = formatBackendOfflineMessage('Failed to fetch', null);

    expect(message).toContain('.\\code-explorer.cmd chat -RestartBackend');
    expect(message).toContain('.\\code-explorer.cmd doctor');
    expect(message).toContain('Failed to fetch');
  });

  it('explains 502 proxy failures without losing the previous success age', () => {
    const message = formatBackendOfflineMessage('HTTP 502 Bad Gateway', 65_000);

    expect(message).toContain('Serveur injoignable depuis 1 min');
    expect(message).toContain('proxy Vite');
    expect(message).toContain('mauvais port');
    expect(message).toContain('HTTP 502 Bad Gateway');
  });
});
