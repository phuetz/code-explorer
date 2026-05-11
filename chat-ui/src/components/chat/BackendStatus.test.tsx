import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { BackendStatus } from './BackendStatus';

vi.mock('../../hooks/use-backend-status', () => ({
  useBackendStatus: () => ({
    lastAttemptAt: Date.now(),
    lastSuccessAt: 0,
    message: 'Connecté à gitnexus@0.1.0',
    nextProbeAt: Date.now() + 10_000,
    service: 'gitnexus',
    status: 'online',
    version: '0.1.0',
  }),
}));

describe('BackendStatus', () => {
  it('opens its details as a fixed portal popover', () => {
    render(<BackendStatus />);

    fireEvent.click(screen.getByRole('button', { name: /serveur connecté/i }));

    const dialog = screen.getByRole('dialog', {
      name: /détails de la connexion au serveur mcp/i,
    });
    expect(dialog.className).toContain('fixed');
    expect(dialog.className).toContain('z-[100]');
    expect(screen.getByText('Serveur connecté')).toBeTruthy();
  });
});
