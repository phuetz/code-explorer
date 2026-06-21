import { afterEach, describe, expect, it, vi } from 'vitest';
import { MCPClient } from './mcp-client';

describe('MCPClient errors', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it('adds a backend hint for HTTP 5xx responses', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => new Response('Bad Gateway', { status: 502, statusText: 'Bad Gateway' }))
    );

    await expect(new MCPClient('http://127.0.0.1:3010').listRepos()).rejects.toThrow(
      /list_repos: HTTP 502 Bad Gateway.*http:\/\/127\.0\.0\.1:3010.*code-explorer\.cmd doctor.*Bad Gateway/
    );
  });

  it('redacts provider secrets from HTTP error bodies', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(
        async () =>
          new Response(
            [
              'provider failed',
              'Authorization: Bearer sk-proj-1234567890abcdef',
              'api_key=AIzaSyD9exampleKeySecret1234567890',
              'refresh_token=ya29.refreshTokenSecret1234567890',
            ].join('\n'),
            { status: 500, statusText: 'Internal Server Error' }
          )
      )
    );

    let message = '';
    try {
      await new MCPClient('http://127.0.0.1:3010').listRepos();
    } catch (error) {
      message = error instanceof Error ? error.message : String(error);
    }

    expect(message).toContain('Bearer [redacted]');
    expect(message).toContain('[redacted-google-key]');
    expect(message).toContain('refresh_token=[redacted-google-token]');
    expect(message).not.toContain('sk-proj-1234567890abcdef');
    expect(message).not.toContain('AIzaSyD9exampleKeySecret1234567890');
    expect(message).not.toContain('ya29.refreshTokenSecret1234567890');
  });

  it('turns fetch failures into actionable backend messages', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => {
        throw new TypeError('Failed to fetch');
      })
    );

    await expect(new MCPClient('').llmConfig()).rejects.toThrow(
      /llm_config: serveur Code Explorer injoignable via le proxy Vite courant.*VITE_MCP_URL.*code-explorer\.cmd doctor.*Failed to fetch/
    );
  });

  it('preserves AbortError so chat cancellation stays distinct', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => {
        throw new DOMException('The operation was aborted.', 'AbortError');
      })
    );

    await expect(new MCPClient('').llmConfig()).rejects.toMatchObject({
      name: 'AbortError',
    });
  });

  it('loads safe runtime diagnostics from the backend', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(
        async () =>
          new Response(
            JSON.stringify({
              service: 'code-explorer',
              version: '0.1.0',
              generatedAtUnixMs: 1778000000000,
              httpAuthRequired: false,
              repoPathsExposed: false,
              repos: {
                count: 1,
                names: [
                  {
                    id: 'repo_alise',
                    name: 'sample-app',
                    indexedAt: '2026-05-06T05:00:00Z',
                    pathExposed: false,
                  },
                ],
              },
              llm: {
                configured: true,
                provider: 'chatgpt',
                model: 'gpt-5.5',
                reasoningEffort: 'high',
                maxTokens: 8192,
              },
              auth: {
                chatgptOAuth: {
                  loggedIn: true,
                  status: 'logged_in',
                  tokenFilePresent: true,
                  tokenFileReadable: true,
                  refreshTokenPresent: true,
                  lastRefresh: '2026-05-06T20:00:00Z',
                  storage: 'dpapi',
                },
              },
            }),
            { status: 200, headers: { 'content-type': 'application/json' } }
          )
      )
    );

    const diagnostics = await new MCPClient('').diagnostics();

    expect(diagnostics.service).toBe('code-explorer');
    expect(diagnostics.repoPathsExposed).toBe(false);
    expect(diagnostics.repos.count).toBe(1);
    expect(diagnostics.llm.provider).toBe('chatgpt');
    expect(diagnostics.llm.model).toBe('gpt-5.5');
    expect(diagnostics.auth?.chatgptOAuth?.loggedIn).toBe(true);
  });

  it('saves LLM provider configuration without serializing an empty API key', async () => {
    const fetchMock = vi.fn(async (...args: Parameters<typeof fetch>) => {
      void args;
      return new Response(
        JSON.stringify({
          configured: true,
          provider: 'ollama',
          model: 'qwen2.5-coder:7b',
          reasoningEffort: 'none',
          maxTokens: 4096,
        }),
        { status: 200, headers: { 'content-type': 'application/json' } }
      );
    });
    vi.stubGlobal('fetch', fetchMock);

    const result = await new MCPClient('http://127.0.0.1:3010').saveLlmConfig({
      provider: 'ollama',
      baseUrl: 'http://localhost:11434/v1',
      model: 'qwen2.5-coder:7b',
      apiKey: undefined,
      reasoningEffort: 'none',
      maxTokens: 4096,
    });

    const init = fetchMock.mock.calls[0][1] as RequestInit;
    const body = JSON.parse(String(init.body));
    expect(String(fetchMock.mock.calls[0][0])).toBe('http://127.0.0.1:3010/api/llm-config');
    expect(init.method).toBe('POST');
    expect(body).not.toHaveProperty('apiKey');
    expect(body.provider).toBe('ollama');
    expect(result.model).toBe('qwen2.5-coder:7b');
  });

  it('tests LLM provider configuration without persisting it', async () => {
    const fetchMock = vi.fn(async (...args: Parameters<typeof fetch>) => {
      void args;
      return new Response(
        JSON.stringify({
          ok: true,
          provider: 'darkstar-ollama',
          model: 'qwen2.5-coder:32b',
          elapsedMs: 35,
          responsePreview: 'OK',
        }),
        { status: 200, headers: { 'content-type': 'application/json' } }
      );
    });
    vi.stubGlobal('fetch', fetchMock);

    const result = await new MCPClient('http://127.0.0.1:3010').testLlmConfig({
      provider: 'darkstar-ollama',
      baseUrl: 'http://100.73.222.64:11434/v1',
      model: 'qwen2.5-coder:32b',
      reasoningEffort: 'none',
      maxTokens: 8192,
    });

    const init = fetchMock.mock.calls[0][1] as RequestInit;
    const body = JSON.parse(String(init.body));
    expect(String(fetchMock.mock.calls[0][0])).toBe('http://127.0.0.1:3010/api/llm-config/test');
    expect(init.method).toBe('POST');
    expect(body).not.toHaveProperty('apiKey');
    expect(body.provider).toBe('darkstar-ollama');
    expect(result.responsePreview).toBe('OK');
  });

  it('lists available models without serializing an empty API key', async () => {
    const fetchMock = vi.fn(async (...args: Parameters<typeof fetch>) => {
      void args;
      return new Response(
        JSON.stringify({
          provider: 'ollama',
          baseUrl: 'http://localhost:11434/v1',
          models: ['gemma4:e2b', 'qwen2.5-coder:7b'],
        }),
        { status: 200, headers: { 'content-type': 'application/json' } }
      );
    });
    vi.stubGlobal('fetch', fetchMock);

    const result = await new MCPClient('http://127.0.0.1:3010').listLlmModels({
      provider: 'ollama',
      baseUrl: 'http://localhost:11434/v1',
      model: 'qwen2.5-coder:7b',
      reasoningEffort: 'none',
      maxTokens: 4096,
    });

    const init = fetchMock.mock.calls[0][1] as RequestInit;
    const body = JSON.parse(String(init.body));
    expect(String(fetchMock.mock.calls[0][0])).toBe('http://127.0.0.1:3010/api/llm-config/models');
    expect(init.method).toBe('POST');
    expect(body).not.toHaveProperty('apiKey');
    expect(body.provider).toBe('ollama');
    expect(result.models).toContain('qwen2.5-coder:7b');
  });

  it('loads Tailscale host availability for network LLM cards', async () => {
    const fetchMock = vi.fn(async (...args: Parameters<typeof fetch>) => {
      void args;
      return new Response(
        JSON.stringify({
          available: true,
          hosts: [
            {
              hostName: 'ministar-linux',
              dnsName: 'ministar-linux.tail.example',
              tailscaleIp: '100.98.18.76',
              online: true,
              active: true,
            },
          ],
          error: null,
        }),
        { status: 200, headers: { 'content-type': 'application/json' } }
      );
    });
    vi.stubGlobal('fetch', fetchMock);

    const result = await new MCPClient('http://127.0.0.1:3010').tailnetHosts();

    expect(String(fetchMock.mock.calls[0][0])).toBe(
      'http://127.0.0.1:3010/api/llm-config/tailnet-hosts'
    );
    expect(result.hosts[0].hostName).toBe('ministar-linux');
    expect(result.hosts[0].online).toBe(true);
  });

  it('persists work-document snapshots through the backend state endpoint', async () => {
    const fetchMock = vi.fn(async (...args: Parameters<typeof fetch>) => {
      const init = args[1] as RequestInit;
      const body = JSON.parse(String(init.body));
      return new Response(
        JSON.stringify({
          id: body.document.id,
          savedAtUnixMs: 1778000000000,
          document: body.document,
        }),
        { status: 200, headers: { 'content-type': 'application/json' } }
      );
    });
    vi.stubGlobal('fetch', fetchMock);

    const document = {
      id: 'doc-1',
      filename: 'Questions Sample.docx',
      importedAt: 1774500000000,
      repo: 'repo_alise',
      repoName: 'sample-app',
      sourceBytes: 1200,
      markdownChars: 4200,
      questions: [],
    };
    const result = await new MCPClient('http://127.0.0.1:3010').saveWorkDocumentState(document);

    const init = fetchMock.mock.calls[0][1] as RequestInit;
    expect(String(fetchMock.mock.calls[0][0])).toBe('http://127.0.0.1:3010/api/workdocs/state');
    expect(init.method).toBe('PUT');
    expect(JSON.parse(String(init.body))).toEqual({ document });
    expect(result.document.filename).toBe('Questions Sample.docx');
  });

  it('loads source and graph exploration payloads from typed REST endpoints', async () => {
    const fetchMock = vi.fn(async (url: string | URL | Request) => {
      const href = String(url);
      if (href.includes('/source?')) {
        return new Response(
          JSON.stringify({
            path: 'src/App.tsx',
            content: 'export function App() {}',
            language: 'typescript',
            totalLines: 1,
            startLine: 1,
            endLine: 1,
            truncated: false,
          }),
          { status: 200, headers: { 'content-type': 'application/json' } }
        );
      }
      if (href.includes('/symbols?')) {
        return new Response(
          JSON.stringify({
            symbols: [
              {
                nodeId: 'Method:src/App.tsx:App',
                name: 'App',
                label: 'Function',
                filePath: 'src/App.tsx',
                score: 12,
              },
            ],
          }),
          { status: 200, headers: { 'content-type': 'application/json' } }
        );
      }
      if (href.includes('/graph/neighborhood?')) {
        return new Response(
          JSON.stringify({
            nodes: [{ id: 'n1', name: 'App', label: 'Function', filePath: 'src/App.tsx' }],
            edges: [],
            stats: { nodeCount: 1, edgeCount: 0, truncated: false },
          }),
          { status: 200, headers: { 'content-type': 'application/json' } }
        );
      }
      return new Response(JSON.stringify({ files: [] }), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      });
    });
    vi.stubGlobal('fetch', fetchMock);

    const client = new MCPClient('http://127.0.0.1:3010');
    const source = await client.source('repo alise', 'src/App.tsx', { start: 1, end: 5 });
    const symbols = await client.symbols('repo alise', 'App');
    const graph = await client.graphNeighborhood('repo alise', 'Method:src/App.tsx:App', 2);

    expect(source.language).toBe('typescript');
    expect(symbols[0].nodeId).toBe('Method:src/App.tsx:App');
    expect(graph.stats.nodeCount).toBe(1);
    expect(String(fetchMock.mock.calls[0][0])).toContain('/api/repos/repo%20alise/source?');
    expect(String(fetchMock.mock.calls[0][0])).toContain('path=src%2FApp.tsx');
    expect(String(fetchMock.mock.calls[2][0])).toContain('node_id=Method%3Asrc%2FApp.tsx%3AApp');
  });
});
