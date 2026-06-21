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
  save: vi.fn(async () => undefined),
  test: vi.fn(async () => ({
    ok: true,
    provider: 'darkstar-ollama',
    model: 'local-code-model:latest',
    elapsedMs: 35,
    responsePreview: 'OK',
  })),
  listModels: vi.fn(async () => [
    'local-code-model:latest',
    'local-general-model:latest',
    'local-fast-model:latest',
  ]),
  tailnetHosts: vi.fn(async () => ({
    available: true,
    hosts: [
      {
        hostName: 'DARKSTAR',
        dnsName: 'darkstar.tail2a752c.ts.net',
        os: 'windows',
        tailscaleIp: '100.73.222.64',
        online: true,
        active: true,
      },
      {
        hostName: 'ministar-linux',
        dnsName: 'ministar-linux.tail2a752c.ts.net',
        os: 'linux',
        tailscaleIp: '100.98.18.76',
        online: true,
        active: true,
      },
    ],
  })),
};

describe('LlmStatus', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('opens detailed LLM settings from the header badge', async () => {
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
    expect(screen.getByRole('button', { name: /choisir ollama local/i })).toBeTruthy();
    expect(await screen.findByRole('button', { name: /choisir darkstar ollama/i })).toBeTruthy();
    expect(screen.getByRole('button', { name: /choisir lm studio local/i })).toBeTruthy();
  });

  it('shows only online tailnet machine cards even when Ollama models are unavailable', async () => {
    const ministarOnlineState = {
      ...readyState,
      tailnetHosts: vi.fn(async () => ({
        available: true,
        hosts: [
          {
            hostName: 'DARKSTAR',
            tailscaleIp: '100.73.222.64',
            online: false,
            active: true,
          },
          {
            hostName: 'ministar-linux',
            tailscaleIp: '100.98.18.76',
            online: true,
            active: true,
          },
        ],
      })),
      listModels: vi.fn(async (update: { baseUrl: string }) => {
        if (update.baseUrl.includes('100.')) {
          throw new Error('timeout');
        }
        return ['local-code-model:latest'];
      }),
    };
    render(<LlmStatus llm={ministarOnlineState} />);

    fireEvent.click(screen.getByRole('button', { name: /configuration llm/i }));

    await waitFor(() => {
      expect(ministarOnlineState.tailnetHosts).toHaveBeenCalled();
    });
    expect(screen.queryByRole('button', { name: /choisir darkstar ollama/i })).toBeNull();
    expect(await screen.findByRole('button', { name: /choisir ministar linux ollama/i })).toBeTruthy();
    expect(screen.getByRole('button', { name: /choisir ollama local/i })).toBeTruthy();
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

  it('saves the selected local provider without sending an empty API key', async () => {
    render(<LlmStatus llm={readyState} />);

    fireEvent.click(screen.getByRole('button', { name: /configuration llm/i }));
    fireEvent.click(screen.getByRole('button', { name: /choisir ollama local/i }));
    await screen.findByRole('combobox', { name: /modèle/i });
    fireEvent.click(screen.getByRole('button', { name: /enregistrer/i }));

    await waitFor(() => {
      expect(readyState.save).toHaveBeenCalledWith({
        provider: 'ollama',
        baseUrl: 'http://localhost:11434/v1',
        model: 'local-code-model:latest',
        apiKey: undefined,
        reasoningEffort: 'none',
        maxTokens: 4096,
      });
    });
  });

  it('loads real model choices for local Ollama providers', async () => {
    render(<LlmStatus llm={readyState} />);

    fireEvent.click(screen.getByRole('button', { name: /configuration llm/i }));
    fireEvent.click(screen.getByRole('button', { name: /choisir ollama local/i }));

    const modelSelect = await screen.findByRole('combobox', { name: /modèle/i });
    expect(readyState.listModels).toHaveBeenCalledWith({
      provider: 'ollama',
      baseUrl: 'http://localhost:11434/v1',
      model: '',
      apiKey: undefined,
      reasoningEffort: 'none',
      maxTokens: 4096,
    });
    const options = Array.from((modelSelect as HTMLSelectElement).options).map(
      (option) => option.value
    );
    expect(options).toEqual([
      'local-code-model:latest',
      'local-general-model:latest',
      'local-fast-model:latest',
    ]);
  });

  it('prefers the smallest detected local chat model by default', async () => {
    const localModelsState = {
      ...readyState,
      listModels: vi.fn(async () => [
        'devstral-small-2:24b-instruct-2512-q4_K_M',
        'qwen3.6:35b-a3b-q4_K_M',
        'qwen3:4b',
        'gemma4:26b',
      ]),
    };
    render(<LlmStatus llm={localModelsState} />);

    fireEvent.click(screen.getByRole('button', { name: /configuration llm/i }));
    fireEvent.click(await screen.findByRole('button', { name: /choisir ministar linux ollama/i }));

    const modelSelect = await screen.findByRole('combobox', { name: /modèle/i });
    expect((modelSelect as HTMLSelectElement).value).toBe('qwen3:4b');
  });

  it('saves the DARKSTAR provider as a tailnet Ollama endpoint', async () => {
    render(<LlmStatus llm={readyState} />);

    fireEvent.click(screen.getByRole('button', { name: /configuration llm/i }));
    fireEvent.click(await screen.findByRole('button', { name: /choisir darkstar ollama/i }));
    await screen.findByRole('combobox', { name: /modèle/i });
    fireEvent.click(screen.getByRole('button', { name: /enregistrer/i }));

    await waitFor(() => {
      expect(readyState.save).toHaveBeenCalledWith({
        provider: 'darkstar-ollama',
        baseUrl: 'http://100.73.222.64:11434/v1',
        model: 'local-code-model:latest',
        apiKey: undefined,
        reasoningEffort: 'none',
        maxTokens: 8192,
      });
    });
  });

  it('tests the selected provider without saving it', async () => {
    render(<LlmStatus llm={readyState} />);

    fireEvent.click(screen.getByRole('button', { name: /configuration llm/i }));
    fireEvent.click(await screen.findByRole('button', { name: /choisir darkstar ollama/i }));
    await screen.findByRole('combobox', { name: /modèle/i });
    fireEvent.click(screen.getByRole('button', { name: /tester/i }));

    await waitFor(() => {
      expect(readyState.test).toHaveBeenCalledWith({
        provider: 'darkstar-ollama',
        baseUrl: 'http://100.73.222.64:11434/v1',
        model: 'local-code-model:latest',
        apiKey: undefined,
        reasoningEffort: 'none',
        maxTokens: 8192,
      });
    });
    expect(await screen.findByText(/test ok en 35 ms/i)).toBeTruthy();
  });

  it('loads real model choices for LM Studio without a hardcoded model', async () => {
    render(<LlmStatus llm={readyState} />);

    fireEvent.click(screen.getByRole('button', { name: /configuration llm/i }));
    fireEvent.click(screen.getByRole('button', { name: /choisir lm studio local/i }));

    const modelSelect = await screen.findByRole('combobox', { name: /modèle/i });
    expect(readyState.listModels).toHaveBeenCalledWith({
      provider: 'lm-studio',
      baseUrl: 'http://localhost:1234/v1',
      model: '',
      apiKey: undefined,
      reasoningEffort: 'none',
      maxTokens: 4096,
    });
    expect((modelSelect as HTMLSelectElement).value).toBe('local-code-model:latest');
  });

  it('explains when LM Studio exposes no chat model', async () => {
    const lmStudioOnlyEmbeddings = {
      ...readyState,
      listModels: vi.fn(async () => []),
    };
    render(<LlmStatus llm={lmStudioOnlyEmbeddings} />);

    fireEvent.click(screen.getByRole('button', { name: /configuration llm/i }));
    fireEvent.click(screen.getByRole('button', { name: /choisir lm studio local/i }));

    expect(await screen.findByText(/aucun modèle chat détecté/i)).toBeTruthy();
    expect((screen.getByRole('button', { name: /enregistrer/i }) as HTMLButtonElement).disabled).toBe(
      true
    );
  });
});
