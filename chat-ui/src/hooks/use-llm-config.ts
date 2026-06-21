import { useCallback, useEffect, useState } from 'react';
import {
  mcpClient,
  type LlmConfigInfo,
  type LlmConfigTestResult,
  type LlmConfigUpdate,
  type TailnetHostsInfo,
} from '../api/mcp-client';

export type LlmConfigStatus = 'checking' | 'ready' | 'missing' | 'error';

export interface LlmConfigState {
  status: LlmConfigStatus;
  config: LlmConfigInfo | null;
  message: string;
  refresh: () => Promise<void>;
  save: (config: LlmConfigUpdate) => Promise<void>;
  test: (config: LlmConfigUpdate) => Promise<LlmConfigTestResult>;
  listModels: (config: LlmConfigUpdate) => Promise<string[]>;
  tailnetHosts: () => Promise<TailnetHostsInfo>;
}

export function useLlmConfig(): LlmConfigState {
  const [status, setStatus] = useState<LlmConfigStatus>('checking');
  const [config, setConfig] = useState<LlmConfigInfo | null>(null);
  const [message, setMessage] = useState('Lecture de la configuration LLM...');

  const refresh = useCallback(async () => {
    setStatus('checking');
    setMessage('Lecture de la configuration LLM...');
    try {
      const next = await mcpClient.llmConfig();
      setConfig(next);
      if (next.configured) {
        setStatus('ready');
        setMessage(`${next.provider ?? 'llm'} / ${next.model ?? 'modèle inconnu'}`);
      } else {
        setStatus('missing');
        setMessage('Aucun LLM configuré');
      }
    } catch (e) {
      setStatus('error');
      setConfig(null);
      setMessage(e instanceof Error ? e.message : String(e));
    }
  }, []);

  const save = useCallback(async (update: LlmConfigUpdate) => {
    setStatus('checking');
    setMessage('Enregistrement de la configuration LLM...');
    try {
      const next = await mcpClient.saveLlmConfig(update);
      setConfig(next);
      if (next.configured) {
        setStatus('ready');
        setMessage(`${next.provider ?? 'llm'} / ${next.model ?? 'modèle inconnu'}`);
      } else {
        setStatus('missing');
        setMessage('Aucun LLM configuré');
      }
    } catch (e) {
      setStatus('error');
      setMessage(e instanceof Error ? e.message : String(e));
      throw e;
    }
  }, []);

  const test = useCallback(async (update: LlmConfigUpdate) => {
    return mcpClient.testLlmConfig(update);
  }, []);

  const listModels = useCallback(async (update: LlmConfigUpdate) => {
    const result = await mcpClient.listLlmModels(update);
    return result.models;
  }, []);

  const tailnetHosts = useCallback(async () => {
    return mcpClient.tailnetHosts();
  }, []);

  useEffect(() => {
    const timer = setTimeout(() => void refresh(), 250);
    return () => clearTimeout(timer);
  }, [refresh]);

  return { status, config, message, refresh, save, test, listModels, tailnetHosts };
}
