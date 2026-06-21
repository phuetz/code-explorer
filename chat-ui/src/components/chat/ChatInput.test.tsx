import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';
import { useChatStore } from '../../stores/chat-store';
import { ChatInput } from './ChatInput';

describe('ChatInput', () => {
  beforeEach(() => {
    localStorage.clear();
    useChatStore.setState({
      sessions: [
        {
          id: 's1',
          title: 'Flux courrier',
          createdAt: 1774507049000,
          updatedAt: 1774507079000,
          messages: [
            {
              id: 'm1',
              role: 'user',
              content: 'Trace le flux courrier',
              createdAt: 1774507049000,
            },
            {
              id: 'm2',
              role: 'assistant',
              content: 'Réponse 1',
              createdAt: 1774507059000,
            },
            {
              id: 'm3',
              role: 'user',
              content: 'Liste les endpoints HTTP',
              createdAt: 1774507069000,
            },
          ],
        },
      ],
      currentSessionId: 's1',
      isStreaming: false,
      selectedRepo: 'repo_alise',
      selectedRepoName: 'sample-app',
      inputDraft: '',
      isSfdPanelOpen: false,
      isWorkspaceOpen: false,
      workspaceTab: 'sources',
      verifiedMode: false,
    });
  });

  it('navigates previous prompts with arrow keys', () => {
    render(<ChatInput />);

    const textarea = screen.getByLabelText(/message à envoyer/i) as HTMLTextAreaElement;

    fireEvent.keyDown(textarea, { key: 'ArrowUp' });
    expect(textarea.value).toBe('Liste les endpoints HTTP');

    fireEvent.keyDown(textarea, { key: 'ArrowUp' });
    expect(textarea.value).toBe('Trace le flux courrier');

    fireEvent.keyDown(textarea, { key: 'ArrowDown' });
    expect(textarea.value).toBe('Liste les endpoints HTTP');

    fireEvent.keyDown(textarea, { key: 'ArrowDown' });
    expect(textarea.value).toBe('');
  });

  it('keeps a typed draft unless history browsing has started', () => {
    render(<ChatInput />);

    const textarea = screen.getByLabelText(/message à envoyer/i) as HTMLTextAreaElement;
    fireEvent.change(textarea, { target: { value: 'question en cours' } });
    fireEvent.keyDown(textarea, { key: 'ArrowUp' });

    expect(textarea.value).toBe('question en cours');
  });

  it('toggles verified answer mode from the composer', () => {
    render(<ChatInput />);

    const toggle = screen.getByRole('button', { name: /vérifié/i });
    expect(toggle.getAttribute('aria-pressed')).toBe('false');

    fireEvent.click(toggle);

    expect(toggle.getAttribute('aria-pressed')).toBe('true');
    expect(useChatStore.getState().verifiedMode).toBe(true);
  });

  it('reformulates the current draft into a guided prompt', () => {
    render(<ChatInput />);

    const textarea = screen.getByLabelText(/message à envoyer/i) as HTMLTextAreaElement;
    fireEvent.change(textarea, { target: { value: 'explique StackLogger' } });

    fireEvent.click(screen.getByRole('button', { name: /reformuler le prompt/i }));

    expect(textarea.value).toContain('Question à traiter : explique StackLogger');
    expect(textarea.value).toContain('dans le dépôt sample-app');
    expect(textarea.value).toContain('section Sources');
    expect(textarea.value).toContain("n'invente aucun fichier");
  });
});
