import { render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { mcpClient } from '../../api/mcp-client';
import { useChatStore } from '../../stores/chat-store';
import { SfdDraftsPanel } from './SfdDraftsPanel';

describe('SfdDraftsPanel', () => {
  beforeEach(() => {
    localStorage.clear();
    useChatStore.setState({
      selectedRepo: 'repo_alise',
      selectedRepoName: 'sample-app',
      isSfdPanelOpen: false,
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('does not fetch SFD pages while the panel is closed', async () => {
    const callTool = vi.spyOn(mcpClient, 'callTool').mockResolvedValue({
      content: [],
      _meta: { pages: [], drafts: [], docsDir: '.codeexplorer/docs', missing: false },
    });

    render(<SfdDraftsPanel />);
    await Promise.resolve();

    expect(screen.queryByLabelText('Brouillons SFD')).toBeNull();
    expect(callTool).not.toHaveBeenCalled();
  });

  it('loads SFD pages when the panel is opened', async () => {
    const callTool = vi.spyOn(mcpClient, 'callTool').mockResolvedValue({
      content: [],
      _meta: {
        pages: ['modules/courrier.md'],
        drafts: ['_drafts/courrier.md'],
        docsDir: '.codeexplorer/docs',
        missing: false,
      },
    });
    useChatStore.setState({ isSfdPanelOpen: true });

    render(<SfdDraftsPanel />);

    await waitFor(() => {
      expect(callTool).toHaveBeenCalledWith('list_sfd_pages', { repo: 'repo_alise' });
      expect(screen.getByText('modules/courrier.md')).toBeTruthy();
      expect(screen.getByText('_drafts/courrier.md')).toBeTruthy();
    });
  });
});
