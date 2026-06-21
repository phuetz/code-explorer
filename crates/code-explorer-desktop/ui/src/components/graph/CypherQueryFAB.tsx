/**
 * Floating Action Button for running ad-hoc Cypher queries against the knowledge graph.
 */

import { useState, useRef, useEffect } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { Terminal, Play, X, ChevronRight, Save, FolderOpen, Trash2 } from "lucide-react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import { commands } from "../../lib/tauri-commands";
import { useI18n } from "../../hooks/use-i18n";
import { confirm } from "../../lib/confirm";
import { useAppStore } from "../../stores/app-store";
import type { SavedQuery } from "../../lib/tauri-commands";

const EXAMPLES = [
  {
    i18nKey: "cypher.preset.allFunctions",
    query: "MATCH (n:Function) RETURN n.name, n.filePath LIMIT 20",
  },
  {
    i18nKey: "cypher.preset.callGraph",
    query: "MATCH (n)-[:CALLS]->(m) RETURN n.name, m.name LIMIT 30",
  },
  {
    i18nKey: "cypher.preset.controllers",
    query:
      "MATCH (n:Controller)-[:DEFINES]->(a:ControllerAction) RETURN n.name, a.name LIMIT 30",
  },
  {
    i18nKey: "cypher.preset.deadCode",
    query: "MATCH (n:Method) WHERE n.isDeadCandidate = true RETURN n.name, n.filePath LIMIT 20",
  },
  {
    i18nKey: "cypher.preset.topCallers",
    query: "MATCH (n)-[:CALLS]->(m) RETURN m.name, count(n) ORDER BY count(n) LIMIT 10",
  },
  {
    i18nKey: "cypher.preset.services",
    query: "MATCH (n:Service) RETURN n.name, n.filePath LIMIT 20",
  },
  {
    i18nKey: "cypher.preset.communities",
    query: "MATCH (n:Community) RETURN n.name LIMIT 20",
  },
];

export function CypherQueryFAB() {
  const { t } = useI18n();
  const queryClient = useQueryClient();
  const activeRepo = useAppStore((s) => s.activeRepo);
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<unknown[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const [showLibrary, setShowLibrary] = useState(false);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const mountedRef = useRef(true);

  const { data: savedQueries = [] } = useQuery({
    queryKey: ["saved-queries", activeRepo],
    queryFn: () => commands.savedQueriesList(),
    enabled: !!activeRepo && open,
    staleTime: 30_000,
  });

  const saveMut = useMutation({
    mutationFn: (q: SavedQuery) => commands.savedQueriesSave(q),
    onSuccess: (next) => {
      queryClient.setQueryData(["saved-queries", activeRepo], next);
      toast.success("Query saved");
    },
    onError: (e) => toast.error(`Save failed: ${(e as Error).message}`),
  });

  const deleteMut = useMutation({
    mutationFn: (id: string) => commands.savedQueriesDelete(id),
    onSuccess: (next) =>
      queryClient.setQueryData(["saved-queries", activeRepo], next),
  });

  const handleSaveCurrent = () => {
    if (!query.trim()) {
      toast.error(t("cypher.emptyQuery"));
      return;
    }
    const name = window.prompt("Name for this saved query:");
    if (!name) return;
    saveMut.mutate({
      id: `q_${Date.now()}`,
      name: name.trim(),
      query: query.trim(),
      tags: [],
      updatedAt: Date.now(),
    });
  };

  useEffect(() => {
    mountedRef.current = true;
    return () => { mountedRef.current = false; };
  }, []);

  useEffect(() => {
    if (open && inputRef.current) {
      inputRef.current.focus();
    }
  }, [open]);

  async function runQuery() {
    if (!query.trim()) return;
    setRunning(true);
    setError(null);
    setResults(null);
    try {
      const res = await commands.executeCypher(query.trim());
      if (mountedRef.current) setResults(res);
    } catch (e: unknown) {
      if (mountedRef.current) setError(e instanceof Error ? e.message : String(e));
    } finally {
      if (mountedRef.current) setRunning(false);
    }
  }

  function handleKeyDown(e: React.KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key === "Enter") {
      e.preventDefault();
      runQuery();
    }
    if (e.key === "Escape") {
      setOpen(false);
    }
  }

  return (
    <>
      {/* FAB button */}
      <motion.button
        onClick={() => setOpen(!open)}
        whileHover={{ scale: 1.1 }}
        whileTap={{ scale: 0.95 }}
        style={{
          position: "absolute",
          bottom: 52,
          right: 16,
          zIndex: 20,
          width: 44,
          height: 44,
          borderRadius: 12,
          border: "1px solid var(--border)",
          background: open ? "var(--accent)" : "var(--bg-1)",
          color: open ? "#fff" : "var(--text-1)",
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          cursor: "pointer",
          boxShadow: "0 4px 16px rgba(0,0,0,0.3)",
        }}
        title={`${t("cypher.title")} (${t("cypher.hint")})`}
        aria-label={open ? "Close Cypher query" : t("cypher.title")}
        aria-expanded={open}
      >
        {open ? <X size={18} /> : <Terminal size={18} />}
      </motion.button>

      {/* Query panel */}
      <AnimatePresence>
        {open && (
          <motion.div
            initial={{ opacity: 0, y: 20, scale: 0.95 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: 20, scale: 0.95 }}
            transition={{ duration: 0.15 }}
            style={{
              position: "absolute",
              bottom: 104,
              right: 16,
              zIndex: 20,
              width: 420,
              maxHeight: 480,
              borderRadius: 12,
              border: "1px solid var(--border)",
              background: "var(--bg-0)",
              boxShadow: "0 8px 32px rgba(0,0,0,0.4)",
              display: "flex",
              flexDirection: "column",
              overflow: "hidden",
            }}
          >
            {/* Header */}
            <div
              style={{
                padding: "10px 14px",
                borderBottom: "1px solid var(--border)",
                fontSize: 12,
                fontWeight: 600,
                color: "var(--text-1)",
                display: "flex",
                alignItems: "center",
                gap: 6,
              }}
            >
              <Terminal size={14} />
              {t("cypher.title")}
              <div
                style={{
                  marginLeft: "auto",
                  display: "flex",
                  alignItems: "center",
                  gap: 6,
                }}
              >
                <button
                  onClick={handleSaveCurrent}
                  title="Save the current query"
                  aria-label="Save query"
                  style={{
                    padding: 6,
                    background: "transparent",
                    border: "1px solid var(--border)",
                    borderRadius: 6,
                    color: "var(--text-2)",
                    cursor: "pointer",
                  }}
                >
                  <Save size={14} />
                </button>
                <button
                  onClick={() => setShowLibrary((v) => !v)}
                  title={`Saved queries (${savedQueries.length})`}
                  aria-label="Open saved queries library"
                  style={{
                    padding: 6,
                    background: showLibrary ? "var(--accent)" : "transparent",
                    border: "1px solid var(--border)",
                    borderRadius: 6,
                    color: showLibrary ? "#fff" : "var(--text-2)",
                    cursor: "pointer",
                    display: "inline-flex",
                    alignItems: "center",
                    gap: 4,
                    fontSize: 11,
                  }}
                >
                  <FolderOpen size={11} />
                  {savedQueries.length}
                </button>
                <span
                  style={{
                    fontSize: 10,
                    color: "var(--text-3)",
                  }}
                >
                  {t("cypher.hint")}
                </span>
              </div>
            </div>

            {/* Saved queries library (toggleable) */}
            {showLibrary && (
              <div
                style={{
                  maxHeight: 180,
                  overflow: "auto",
                  borderBottom: "1px solid var(--border)",
                  background: "var(--bg-1)",
                }}
              >
                {savedQueries.length === 0 ? (
                  <div style={{ padding: 12, fontSize: 11, color: "var(--text-3)" }}>
                    No saved queries. Click the save icon to keep one for later.
                  </div>
                ) : (
                  savedQueries.map((q) => (
                    <div
                      key={q.id}
                      className="transition-colors"
                      style={{
                        display: "flex",
                        alignItems: "center",
                        padding: "6px 12px",
                        borderBottom: "1px solid var(--surface-border)",
                      }}
                      onMouseEnter={(e) => (e.currentTarget.style.background = "var(--bg-3)")}
                      onMouseLeave={(e) => (e.currentTarget.style.background = "transparent")}
                    >
                      <button
                        onClick={() => {
                          setQuery(q.query);
                          setShowLibrary(false);
                          inputRef.current?.focus();
                        }}
                        style={{
                          flex: 1,
                          background: "transparent",
                          border: "none",
                          textAlign: "left",
                          cursor: "pointer",
                          color: "var(--text-1)",
                          fontFamily: "inherit",
                          padding: 0,
                        }}
                      >
                        <div style={{ fontSize: 11, fontWeight: 600 }}>{q.name}</div>
                        <div
                          title={q.query}
                          style={{
                            fontSize: 10,
                            color: "var(--text-3)",
                            fontFamily: "var(--font-mono)",
                            overflow: "hidden",
                            textOverflow: "ellipsis",
                            whiteSpace: "nowrap",
                            maxWidth: 320,
                          }}
                        >
                          {q.query}
                        </div>
                      </button>
                      <button
                        onClick={async () => {
                          const ok = await confirm({
                            title: t("confirm.deleteTitle"),
                            message: t("cypher.confirmDelete"),
                            confirmLabel: t("confirm.delete"),
                            danger: true,
                          });
                          if (ok) deleteMut.mutate(q.id);
                        }}
                        title={t("cypher.deleteSaved")}
                        aria-label={t("cypher.deleteSaved")}
                        style={{
                          padding: 4,
                          background: "transparent",
                          border: "none",
                          color: "var(--text-3)",
                          cursor: "pointer",
                        }}
                      >
                        <Trash2 size={11} />
                      </button>
                    </div>
                  ))
                )}
              </div>
            )}

            {/* Query input */}
            <textarea
              ref={inputRef}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="MATCH (n:Function) RETURN n.name LIMIT 10"
              rows={3}
              style={{
                width: "100%",
                padding: "10px 14px",
                background: "var(--bg-1)",
                border: "none",
                borderBottom: "1px solid var(--border)",
                color: "var(--text-0)",
                fontFamily: "var(--font-mono)",
                fontSize: 12,
                resize: "none",
              }}
            />

            {/* Example chips */}
            <div
              style={{
                padding: "8px 14px",
                display: "flex",
                flexWrap: "wrap",
                gap: 6,
                borderBottom: "1px solid var(--border)",
              }}
            >
              {EXAMPLES.map((ex) => (
                <button
                  key={ex.i18nKey}
                  onClick={() => {
                    setQuery(ex.query);
                    setResults(null);
                    setError(null);
                  }}
                  style={{
                    padding: "3px 8px",
                    borderRadius: 6,
                    border: "1px solid var(--border)",
                    background: "var(--bg-2)",
                    color: "var(--text-2)",
                    fontSize: 10,
                    cursor: "pointer",
                    display: "flex",
                    alignItems: "center",
                    gap: 3,
                  }}
                >
                  <ChevronRight size={10} />
                  {t(ex.i18nKey)}
                </button>
              ))}
            </div>

            {/* Run button */}
            <div style={{ padding: "8px 14px" }}>
              <button
                onClick={runQuery}
                disabled={running || !query.trim()}
                style={{
                  width: "100%",
                  padding: "6px 12px",
                  borderRadius: 6,
                  border: "none",
                  background: running ? "var(--bg-2)" : "var(--accent)",
                  color: "#fff",
                  fontSize: 12,
                  fontWeight: 600,
                  cursor: running ? "wait" : "pointer",
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "center",
                  gap: 6,
                  opacity: !query.trim() ? 0.5 : 1,
                }}
              >
                <Play size={12} />
                {running ? t("cypher.running") : t("cypher.run")}
              </button>
            </div>

            {/* Error */}
            {error && (
              <div
                style={{
                  padding: "8px 14px",
                  color: "var(--rose)",
                  fontSize: 11,
                  borderTop: "1px solid var(--border)",
                }}
              >
                {error}
              </div>
            )}

            {/* Results */}
            {results && (
              <div
                style={{
                  maxHeight: 200,
                  overflow: "auto",
                  borderTop: "1px solid var(--border)",
                  padding: "8px 14px",
                }}
              >
                <div
                  style={{
                    fontSize: 10,
                    color: "var(--text-3)",
                    marginBottom: 6,
                  }}
                >
                  {results.length} {results.length !== 1 ? t("cypher.results") : t("cypher.result")}
                </div>
                <ResultsView data={results} />
              </div>
            )}
          </motion.div>
        )}
      </AnimatePresence>
    </>
  );
}

/** Renders query results as a table if possible, otherwise as JSON */
function ResultsView({ data }: { data: unknown[] }) {
  // Try to render as table if results are objects with consistent keys
  if (data.length > 0 && typeof data[0] === "object" && data[0] !== null) {
    const keys = Object.keys(data[0] as Record<string, unknown>);
    if (keys.length > 0 && keys.length <= 8) {
      return (
        <div style={{ overflow: "auto" }}>
          <table style={{ width: "100%", borderCollapse: "collapse", fontSize: 11 }}>
            <thead>
              <tr>
                {keys.map((k) => (
                  <th
                    key={k}
                    style={{
                      padding: "4px 8px",
                      textAlign: "left",
                      color: "var(--text-3)",
                      fontWeight: 500,
                      borderBottom: "1px solid var(--surface-border)",
                      fontFamily: "var(--font-mono)",
                      fontSize: 10,
                      whiteSpace: "nowrap",
                    }}
                  >
                    {k}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {data.map((row, i) => (
                <tr key={i}>
                  {keys.map((k) => (
                    <td
                      key={k}
                      style={{
                        padding: "3px 8px",
                        color: "var(--text-1)",
                        fontFamily: "var(--font-mono)",
                        fontSize: 10,
                        maxWidth: 200,
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                        whiteSpace: "nowrap",
                        borderBottom: "1px solid var(--surface-border)",
                      }}
                    >
                      {String((row as Record<string, unknown>)[k] ?? "")}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      );
    }
  }

  // Fallback: raw JSON
  return (
    <pre
      style={{
        fontSize: 11,
        color: "var(--text-1)",
        fontFamily: "var(--font-mono)",
        whiteSpace: "pre-wrap",
        wordBreak: "break-all",
        lineHeight: 1.5,
      }}
    >
      {JSON.stringify(data, null, 2)}
    </pre>
  );
}
