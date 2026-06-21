import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it } from 'vitest';
import { ChatSidebar } from './ChatSidebar';
import { useChatStore } from '../../stores/chat-store';

describe('ChatSidebar', () => {
  beforeEach(() => {
    localStorage.clear();
    useChatStore.setState({
      sessions: [
        {
          id: 's1',
          title: 'Flux courrier',
          createdAt: 1778000000000,
          updatedAt: 1778000000000,
          messages: [
            {
              id: 'm1',
              role: 'assistant',
              content: 'Trace la création de courrier en masse',
              createdAt: 1778000000000,
            },
          ],
        },
        {
          id: 's2',
          title: 'Architecture MVC',
          createdAt: 1778000000000,
          updatedAt: 1778000000000,
          messages: [
            {
              id: 'm2',
              role: 'assistant',
              content: 'Vue globale contrôleurs services repository',
              createdAt: 1778000000000,
            },
          ],
        },
      ],
      currentSessionId: 's1',
      isStreaming: false,
      selectedRepo: null,
      selectedRepoName: null,
      inputDraft: '',
      isSfdPanelOpen: false,
    });
  });

  it('filters conversations by title or message content', () => {
    render(<ChatSidebar />);

    expect(screen.getByText('Flux courrier')).toBeTruthy();
    expect(screen.getByText('Architecture MVC')).toBeTruthy();
    expect(screen.getAllByText(/1 message/)).toHaveLength(2);

    fireEvent.change(screen.getByRole('searchbox', { name: /rechercher une conversation/i }), {
      target: { value: 'repository' },
    });

    expect(screen.queryByText('Flux courrier')).toBeNull();
    expect(screen.getByText('Architecture MVC')).toBeTruthy();
    expect(screen.getByText('1/2 sessions')).toBeTruthy();
  });

  it('filters conversations with multiple accent-insensitive tokens', () => {
    render(<ChatSidebar />);

    fireEvent.change(screen.getByRole('searchbox', { name: /rechercher une conversation/i }), {
      target: { value: 'controleurs services' },
    });

    expect(screen.queryByText('Flux courrier')).toBeNull();
    expect(screen.getByText('Architecture MVC')).toBeTruthy();
    expect(screen.getByText('1/2 sessions')).toBeTruthy();
  });

  it('shows an empty search state', () => {
    render(<ChatSidebar />);

    fireEvent.change(screen.getByRole('searchbox', { name: /rechercher une conversation/i }), {
      target: { value: 'introuvable' },
    });

    expect(screen.getByText('Aucune conversation ne correspond à cette recherche.')).toBeTruthy();
  });

  it('renames a conversation inline', () => {
    render(<ChatSidebar />);

    fireEvent.click(screen.getByLabelText('Renommer la conversation "Flux courrier"'));
    fireEvent.change(screen.getByLabelText('Nouveau titre de conversation'), {
      target: { value: 'Courriers en masse' },
    });
    fireEvent.keyDown(screen.getByLabelText('Nouveau titre de conversation'), { key: 'Enter' });

    expect(screen.queryByText('Flux courrier')).toBeNull();
    expect(screen.getByText('Courriers en masse')).toBeTruthy();
    expect(useChatStore.getState().sessions[0].title).toBe('Courriers en masse');
  });
});
