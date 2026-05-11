import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { LlmStatus } from './LlmStatus';
import { copyTextToClipboard } from '../../utils/clipboard';
import type { LlmConfigState } from '../../hooks/use-llm-config';

vi.mock('../../utils/clipboard', () => ({
  copyTextToClipboard: vi.fn(async () => true),
}));

const readyState: LlmConfigState = {
  status: 'ready',
  config: {
    configured: true,
    provider: 'chatgpt',
    model: 'gpt-5.5',
    reasoningEffort: 'high',
    maxTokens: 8192,
  },
  message: 'chatgpt / gpt-5.5',
  refresh: vi.fn(async () => undefined),
};

describe('LlmStatus', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('opens detailed LLM settings from the header badge', () => {
    render(<LlmStatus llm={readyState} />);

    fireEvent.click(screen.getByRole('button', { name: /configuration llm/i }));

    const dialog = screen.getByRole('dialog', { name: /détails de la configuration llm/i });
    expect(dialog).toBeTruthy();
    expect(dialog.className).toContain('fixed');
    expect(dialog.className).toContain('z-[100]');
    expect(screen.getAllByText('chatgpt').length).toBeGreaterThan(0);
    expect(screen.getAllByText('gpt-5.5').length).toBeGreaterThan(0);
    expect(screen.getAllByText('high').length).toBeGreaterThan(0);
    expect(screen.getByText(/config-chatgpt\.cmd -Model gpt-5\.5 -Reasoning xhigh/)).toBeTruthy();
    expect(screen.getByRole('button', { name: /configuration llm en low/i })).toBeTruthy();
    expect(screen.getByRole('button', { name: /configuration llm en medium/i })).toBeTruthy();
    expect(screen.getByRole('button', { name: /configuration llm en high/i })).toBeTruthy();
    expect(screen.getByRole('button', { name: /configuration llm en xhigh/i })).toBeTruthy();
  });

  it('copies the selected reasoning preset command', async () => {
    render(<LlmStatus llm={readyState} />);

    fireEvent.click(screen.getByRole('button', { name: /configuration llm/i }));
    fireEvent.click(screen.getByRole('button', { name: /configuration llm en xhigh/i }));

    await waitFor(() => {
      expect(copyTextToClipboard).toHaveBeenCalledWith(
        '.\\config-chatgpt.cmd -Model gpt-5.5 -Reasoning xhigh -MaxTokens 8192'
      );
    });
  });

  it('copies high without forcing xhigh', async () => {
    render(<LlmStatus llm={readyState} />);

    fireEvent.click(screen.getByRole('button', { name: /configuration llm/i }));
    fireEvent.click(screen.getByRole('button', { name: /configuration llm en high/i }));

    await waitFor(() => {
      expect(copyTextToClipboard).toHaveBeenCalledWith(
        '.\\config-chatgpt.cmd -Model gpt-5.5 -Reasoning high -MaxTokens 8192'
      );
    });
  });
});
