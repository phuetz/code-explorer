import { useMemo, useState } from "react";
import { Plus, MessageSquare, Trash2, Pencil, Pin, GitBranch, Search, Filter } from "lucide-react";
import { useChatSessionStore, type ChatSession } from "../../stores/chat-session-store";
import { useAppStore } from "../../stores/app-store";
import { useI18n } from "../../hooks/use-i18n";

/**
 * TreeNode = a session plus any children that forked from it. We render
 * the forest as a tree so "Fork from here" has a visible hierarchy.
 */
interface TreeNode {
  session: ChatSession;
  children: TreeNode[];
  depth: number;
}

/**
 * Build the session forest for a given repo. Orphans whose `parentId`
 * isn't in the set become roots (keeps us resilient to deletion of an
 * ancestor).
 */
function buildTree(sessions: ChatSession[]): TreeNode[] {
  const byId = new Map<string, ChatSession>();
  for (const s of sessions) byId.set(s.id, s);
  const childrenMap = new Map<string, ChatSession[]>();
  const roots: ChatSession[] = [];

  for (const s of sessions) {
    if (s.parentId && byId.has(s.parentId)) {
      const list = childrenMap.get(s.parentId) ?? [];
      list.push(s);
      childrenMap.set(s.parentId, list);
    } else {
      roots.push(s);
    }
  }

  const build = (s: ChatSession, depth: number): TreeNode => ({
    session: s,
    depth,
    children: (childrenMap.get(s.id) ?? [])
      .sort((a, b) => b.updatedAt - a.updatedAt)
      .map((c) => build(c, depth + 1)),
  });

  return roots
    .sort((a, b) => b.updatedAt - a.updatedAt)
    .map((r) => build(r, 0));
}

function flatten(tree: TreeNode[]): TreeNode[] {
  const out: TreeNode[] = [];
  const walk = (node: TreeNode) => {
    out.push(node);
    for (const c of node.children) walk(c);
  };
  for (const r of tree) walk(r);
  return out;
}

export function ChatHistorySidebar() {
  const { t } = useI18n();
  const activeRepo = useAppStore((s) => s.activeRepo) || "global";
  const {
    activeSessionId,
    createSession,
    deleteSession,
    setActiveSession,
    renameSession,
    getSessionsForRepo,
  } = useChatSessionStore();
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editValue, setEditValue] = useState("");
  const [filterText, setFilterText] = useState("");
  const [pinnedOnly, setPinnedOnly] = useState(false);

  const repoSessions = getSessionsForRepo(activeRepo);

  const tree = useMemo(() => buildTree(repoSessions), [repoSessions]);
  const flat = useMemo(() => flatten(tree), [tree]);

  const filtered = useMemo(() => {
    const needle = filterText.trim().toLowerCase();
    return flat.filter((node) => {
      if (needle && !node.session.title.toLowerCase().includes(needle)) return false;
      if (pinnedOnly) {
        const hasPinned = node.session.messages.some((m) => m.pinned);
        if (!hasPinned) return false;
      }
      return true;
    });
  }, [flat, filterText, pinnedOnly]);

  const handleNewChat = () => {
    createSession(activeRepo, t("chat.newChat"));
  };

  return (
    <div
      className="flex flex-col h-full w-full"
      style={{
        background: "var(--bg-0)",
        borderRight: "1px solid var(--surface-border)",
      }}
    >
      <div
        className="shrink-0 flex items-center justify-between px-4 py-3"
        style={{ borderBottom: "1px solid var(--surface-border)" }}
      >
        <h3
          style={{
            fontFamily: "var(--font-display)",
            fontSize: 14,
            fontWeight: 600,
            color: "var(--text-1)",
          }}
        >
          {t("chat.recentChats")}
        </h3>
        <button
          onClick={handleNewChat}
          className="p-1 rounded-lg transition-colors flex items-center justify-center"
          style={{ background: "var(--accent)", color: "white", width: 24, height: 24 }}
          title={t("chat.newChat")}
          aria-label="Create new chat"
        >
          <Plus size={14} />
        </button>
      </div>

      {/* Filter row */}
      <div
        className="shrink-0 px-3 py-2"
        style={{ borderBottom: "1px solid var(--surface-border)", display: "flex", flexDirection: "column", gap: 6 }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
          <Search size={12} style={{ color: "var(--text-3)" }} />
          <input
            value={filterText}
            onChange={(e) => setFilterText(e.target.value)}
            placeholder={t("chat.sidebar.filterPlaceholder") || "Filter sessions…"}
            style={{
              flex: 1,
              background: "transparent",
              outline: "none",
              border: "none",
              color: "var(--text-1)",
              fontSize: 12,
            }}
          />
        </div>
        <button
          onClick={() => setPinnedOnly((p) => !p)}
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: 4,
            fontSize: 11,
            padding: "2px 6px",
            borderRadius: 4,
            border: "1px solid var(--surface-border)",
            background: pinnedOnly ? "var(--accent-subtle)" : "transparent",
            color: pinnedOnly ? "var(--accent)" : "var(--text-3)",
            cursor: "pointer",
            alignSelf: "flex-start",
          }}
          title="Show only sessions with pinned messages"
        >
          {pinnedOnly ? <Pin size={10} /> : <Filter size={10} />}
          {t("chat.sidebar.pinnedOnly") || "Pinned only"}
        </button>
      </div>

      <div className="flex-1 overflow-y-auto py-2 px-2" style={{ gap: 4, display: "flex", flexDirection: "column" }}>
        {filtered.length === 0 ? (
          <div className="text-center px-4 py-8" style={{ color: "var(--text-3)", fontSize: 12 }}>
            {repoSessions.length === 0 ? t("chat.noRecentChats") : (t("chat.sidebar.noMatches") || "No matching sessions.")}
          </div>
        ) : (
          filtered.map((node) => {
            const { session, depth } = node;
            const isActive = session.id === activeSessionId;
            const hasPinned = session.messages.some((m) => m.pinned);
            return (
              <div
                key={session.id}
                className="group relative flex items-center rounded-lg px-3 py-2 cursor-pointer transition-colors"
                style={{
                  background: isActive ? "var(--bg-2)" : "transparent",
                  color: isActive ? "var(--text-0)" : "var(--text-2)",
                  marginLeft: depth * 12,
                  borderLeft: depth > 0 ? "2px solid var(--surface-border)" : "none",
                }}
                onClick={() => setActiveSession(session.id)}
              >
                {depth > 0 ? (
                  <GitBranch size={12} className="shrink-0 mr-2" style={{ color: "var(--text-3)" }} />
                ) : (
                  <MessageSquare size={14} className="shrink-0 mr-3" />
                )}
                {editingId === session.id ? (
                  <input
                    autoFocus
                    className="flex-1 text-[13px] bg-transparent outline-none border-b"
                    style={{ color: "var(--text-0)", borderColor: "var(--accent)" }}
                    value={editValue}
                    onChange={(e) => setEditValue(e.target.value)}
                    onBlur={() => {
                      if (editValue.trim()) renameSession(session.id, editValue.trim());
                      setEditingId(null);
                    }}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") { e.currentTarget.blur(); }
                      if (e.key === "Escape") { setEditingId(null); }
                    }}
                    onClick={(e) => e.stopPropagation()}
                  />
                ) : (
                  <div
                    className="flex-1 truncate text-[13px] flex items-center gap-1"
                    style={{ fontWeight: isActive ? 500 : 400 }}
                    title={session.title}
                    onDoubleClick={(e) => {
                      e.stopPropagation();
                      setEditingId(session.id);
                      setEditValue(session.title);
                    }}
                  >
                    <span className="truncate">{session.title}</span>
                    {hasPinned && (
                      <Pin size={10} style={{ color: "var(--accent)", flexShrink: 0 }} aria-label="Has pinned messages" />
                    )}
                  </div>
                )}
                <div className="shrink-0 flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity">
                  <button
                    style={{ color: "var(--text-3)" }}
                    className="p-1"
                    onClick={(e) => {
                      e.stopPropagation();
                      setEditingId(session.id);
                      setEditValue(session.title);
                    }}
                    title={t("chat.renameChat")}
                  >
                    <Pencil size={11} />
                  </button>
                  <button
                    style={{ color: "var(--red-400)" }}
                    className="p-1"
                    onClick={(e) => {
                      e.stopPropagation();
                      deleteSession(session.id);
                    }}
                    title={t("chat.deleteChat")}
                  >
                    <Trash2 size={12} />
                  </button>
                </div>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}
