import { useMemo, useState } from 'react';
import clsx from 'clsx';
import { Plus, MessageSquare, Trash2, Library, Search, Pencil } from 'lucide-react';
import { useChatStore } from '../../stores/chat-store';
import { formatMessageTimestamp } from '../../utils/dates';

export function ChatSidebar() {
  const sessions = useChatStore((s) => s.sessions);
  const currentSessionId = useChatStore((s) => s.currentSessionId);
  const createSession = useChatStore((s) => s.createSession);
  const selectSession = useChatStore((s) => s.selectSession);
  const deleteSession = useChatStore((s) => s.deleteSession);
  const renameSession = useChatStore((s) => s.renameSession);
  const [query, setQuery] = useState('');
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editingTitle, setEditingTitle] = useState('');
  const queryTokens = useMemo(() => searchTokens(query), [query]);
  const filteredSessions = useMemo(() => {
    if (queryTokens.length === 0) return sessions;
    return sessions.filter((session) => {
      const haystack = normalizeSearchText([
        session.title,
        ...session.messages.map((message) => message.content),
      ].join('\n'));
      return queryTokens.every((token) => haystack.includes(token));
    });
  }, [queryTokens, sessions]);
  const sessionCountLabel = queryTokens.length > 0
    ? `${filteredSessions.length}/${sessions.length}`
    : String(sessions.length);

  const startRename = (sessionId: string, title: string) => {
    setEditingId(sessionId);
    setEditingTitle(title);
  };

  const commitRename = () => {
    if (!editingId) return;
    const title = editingTitle.trim();
    if (title) {
      renameSession(editingId, title);
    }
    setEditingId(null);
    setEditingTitle('');
  };

  const cancelRename = () => {
    setEditingId(null);
    setEditingTitle('');
  };

  return (
    <aside className="app-sidebar flex h-full w-72 shrink-0 flex-col border-r">
      <div className="border-b border-[var(--border)] p-3">
        <div className="mb-3 flex items-center gap-2 px-1">
          <div className="brand-tile flex h-7 w-7 items-center justify-center rounded-md">
            <Library size={14} aria-hidden="true" />
          </div>
          <div className="min-w-0">
            <div className="text-sm font-medium text-neutral-200">Conversations</div>
            <div className="text-xs text-neutral-600">
              {sessionCountLabel} session{sessions.length > 1 ? 's' : ''}
            </div>
          </div>
        </div>
        <button
          type="button"
          onClick={() => createSession()}
          className="primary-action flex w-full items-center justify-center gap-2 rounded-md border px-3 py-2 text-sm transition"
        >
          <Plus size={14} aria-hidden="true" />
          Nouvelle conversation
        </button>
        <label className="field-shell mt-2 flex items-center gap-2 rounded-md border px-2 py-1.5 text-xs">
          <Search size={13} aria-hidden="true" />
          <input
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Rechercher..."
            aria-label="Rechercher une conversation"
            className="min-w-0 flex-1 bg-transparent text-[var(--text-primary)] outline-none placeholder:text-[var(--text-faint)]"
          />
        </label>
      </div>

      <div className="flex-1 overflow-y-auto p-2">
        {sessions.length === 0 ? (
          <p className="px-3 py-4 text-xs text-neutral-600">
            Aucune conversation. Crée-en une pour démarrer.
          </p>
        ) : filteredSessions.length === 0 ? (
          <p className="px-3 py-4 text-xs text-neutral-600">
            Aucune conversation ne correspond à cette recherche.
          </p>
        ) : (
          filteredSessions.map((session) => {
            const updatedAt = formatMessageTimestamp(session.updatedAt);
            const messageCount = session.messages.length;
            const isEditing = editingId === session.id;
            const activityLabel = [
              `${messageCount} message${messageCount > 1 ? 's' : ''}`,
              updatedAt,
            ].filter(Boolean).join(' · ');

            return (
              <div
                key={session.id}
                onClick={() => selectSession(session.id)}
                className={clsx(
                  'group flex cursor-pointer items-start gap-2 rounded-md border px-3 py-2 text-sm transition',
                  session.id === currentSessionId
                    ? 'session-row session-row-active'
                    : 'session-row text-neutral-400'
                )}
              >
                <MessageSquare size={14} className="mt-0.5 shrink-0 opacity-60" aria-hidden="true" />
                <div className="min-w-0 flex-1">
                  {isEditing ? (
                    <input
                      value={editingTitle}
                      onChange={(event) => setEditingTitle(event.target.value)}
                      onClick={(event) => event.stopPropagation()}
                      onBlur={commitRename}
                      onKeyDown={(event) => {
                        if (event.key === 'Enter') {
                          event.preventDefault();
                          commitRename();
                        }
                        if (event.key === 'Escape') {
                          event.preventDefault();
                          cancelRename();
                        }
                      }}
                      aria-label="Nouveau titre de conversation"
                      autoFocus
                      className="field-shell block w-full rounded border px-1.5 py-0.5 text-xs outline-none"
                    />
                  ) : (
                    <span className="block truncate">{session.title}</span>
                  )}
                  <span className="mt-0.5 block truncate text-[11px] text-neutral-600 group-hover:text-neutral-500">
                    {activityLabel}
                  </span>
                </div>
                {!isEditing && (
                  <div className="mt-0.5 flex items-center gap-1 opacity-0 transition group-hover:opacity-100 focus-within:opacity-100">
                    <button
                      type="button"
                      onClick={(e) => {
                        e.stopPropagation();
                        startRename(session.id, session.title);
                      }}
                      aria-label={`Renommer la conversation "${session.title}"`}
                      className="hover:text-neutral-100"
                    >
                      <Pencil size={13} aria-hidden="true" />
                    </button>
                    <button
                      type="button"
                      onClick={(e) => {
                        e.stopPropagation();
                        deleteSession(session.id);
                      }}
                      aria-label={`Supprimer la conversation "${session.title}"`}
                      className="hover:text-red-400"
                    >
                      <Trash2 size={14} aria-hidden="true" />
                    </button>
                  </div>
                )}
              </div>
            );
          })
        )}
      </div>

      <div className="border-t border-[var(--border)] p-3 text-[11px] text-neutral-600">
        <div className="font-medium text-neutral-500">GitNexus Chat</div>
        <div className="mt-0.5">Interface web MCP</div>
      </div>
    </aside>
  );
}

function normalizeSearchText(value: string): string {
  return value
    .normalize('NFD')
    .replace(/[\u0300-\u036f]/g, '')
    .toLowerCase();
}

function searchTokens(value: string): string[] {
  return normalizeSearchText(value)
    .trim()
    .split(/\s+/)
    .filter(Boolean);
}
