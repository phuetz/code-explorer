import { useCallback, useEffect, useState } from 'react';
import {
  mcpClient,
  type McpToolResult,
  type SfdDraftWrittenMeta,
  type SfdPagesMeta,
  type SfdValidateMeta,
  type SfdValidationReport,
} from '../api/mcp-client';

/**
 * SFD doc-authoring helper hook. Reads the active repo from the prop, calls
 * the three MCP tools (`list_sfd_pages`, `write_sfd_draft`, `validate_sfd`),
 * and exposes the typed `_meta` payloads.
 *
 * The hook is unopinionated about UI: it just owns network state.
 */
export function useSfd(repo: string | null) {
  const [pages, setPages] = useState<string[]>([]);
  const [drafts, setDrafts] = useState<string[]>([]);
  const [docsDir, setDocsDir] = useState<string>('');
  const [missing, setMissing] = useState<boolean>(false);
  const [report, setReport] = useState<SfdValidationReport | null>(null);
  const [reportStatus, setReportStatus] = useState<'green' | 'yellow' | 'red' | null>(null);
  const [busy, setBusy] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);

  const refreshPages = useCallback(async () => {
    if (!repo) return;
    setBusy(true);
    setError(null);
    try {
      const result = await mcpClient.callTool<McpToolResult>('list_sfd_pages', { repo });
      const meta = result._meta as SfdPagesMeta | undefined;
      if (meta) {
        setPages(meta.pages);
        setDrafts(meta.drafts);
        setDocsDir(meta.docsDir);
        setMissing(meta.missing);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }, [repo]);

  const writeDraft = useCallback(
    async (page: string, content: string): Promise<SfdDraftWrittenMeta | null> => {
      if (!repo) return null;
      setBusy(true);
      setError(null);
      try {
        const result = await mcpClient.callTool<McpToolResult>('write_sfd_draft', {
          repo,
          page,
          content,
        });
        const meta = result._meta as SfdDraftWrittenMeta | undefined;
        await refreshPages();
        return meta ?? null;
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
        return null;
      } finally {
        setBusy(false);
      }
    },
    [repo, refreshPages]
  );

  const validate = useCallback(
    async (subPath: string = ''): Promise<SfdValidateMeta | null> => {
      if (!repo) return null;
      setBusy(true);
      setError(null);
      try {
        const result = await mcpClient.callTool<McpToolResult>('validate_sfd', {
          repo,
          path: subPath,
        });
        const meta = result._meta as SfdValidateMeta | undefined;
        if (meta) {
          setReport(meta.report);
          setReportStatus(meta.status);
        }
        return meta ?? null;
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
        return null;
      } finally {
        setBusy(false);
      }
    },
    [repo]
  );

  useEffect(() => {
    queueMicrotask(() => {
      if (repo) {
        void refreshPages();
      } else {
        setPages([]);
        setDrafts([]);
        setReport(null);
        setReportStatus(null);
      }
    });
  }, [repo, refreshPages]);

  return {
    pages,
    drafts,
    docsDir,
    missing,
    report,
    reportStatus,
    busy,
    error,
    refreshPages,
    writeDraft,
    validate,
  };
}
