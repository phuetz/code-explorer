import { useState, useEffect } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { invoke } from "@tauri-apps/api/core";
import { MdEditor } from "md-editor-rt";
import "md-editor-rt/lib/style.css";
import { Sidebar } from "./components/Sidebar";
import { GraphView } from "./components/GraphView";
import { useVaultStore } from "./stores/vault-store";
import { FolderOpen, Save, Share2, Edit3 } from "lucide-react";

function App() {
  const { vaultPath, setVaultPath, selectedNote, setSelectedNote, viewMode, setViewMode } = useVaultStore();
  const [noteContent, setNoteContent] = useState("");
  const [isSaving, setIsSaving] = useState(false);

  // Load note content when selectedNote changes
  useEffect(() => {
    if (vaultPath && selectedNote) {
      invoke<string>("read_note", { vaultPath, notePath: selectedNote })
        .then((content) => {
          setNoteContent(content);
        })
        .catch((err) => {
          console.error("Failed to read note:", err);
        });
    } else {
      setNoteContent("");
    }
  }, [vaultPath, selectedNote]);

  const handleOpenVault = async () => {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Select Code Explorer Vault",
    });
    if (selected && typeof selected === 'string') {
      setSelectedNote(null); // reset before switching vaults
      setVaultPath(selected);
    }
  };

  const handleSave = async () => {
    if (!vaultPath || !selectedNote) return;
    setIsSaving(true);
    try {
      await invoke("save_note", { vaultPath, notePath: selectedNote, content: noteContent });
      setTimeout(() => setIsSaving(false), 500);
    } catch (err) {
      console.error("Failed to save note:", err);
      setIsSaving(false);
    }
  };

  return (
    <div className="flex h-screen w-screen bg-zinc-950 text-zinc-200 overflow-hidden font-sans">
      <Sidebar />

      <main className="flex-1 flex flex-col min-w-0 bg-zinc-900/20">
        {/* Toolbar */}
        <header className="h-14 border-b border-zinc-800 flex items-center justify-between px-6 bg-zinc-900/40 backdrop-blur-md">
          <div className="flex items-center gap-4">
            <button
              onClick={handleOpenVault}
              className="flex items-center gap-2 px-3 py-1.5 bg-zinc-800 hover:bg-zinc-700 rounded-md text-xs font-medium transition-colors"
            >
              <FolderOpen size={14} />
              Open Vault
            </button>
            <div className="h-4 w-[1px] bg-zinc-800 mx-1"></div>
            <div className="flex bg-zinc-900 rounded-lg p-1 border border-zinc-800">
              <button
                onClick={() => setViewMode('editor')}
                className={`flex items-center gap-2 px-3 py-1 rounded-md text-[11px] font-bold transition-all ${
                  viewMode === 'editor' ? 'bg-zinc-800 text-blue-400 shadow-sm' : 'text-zinc-500 hover:text-zinc-300'
                }`}
              >
                <Edit3 size={12} />
                EDITOR
              </button>
              <button
                onClick={() => setViewMode('graph')}
                className={`flex items-center gap-2 px-3 py-1 rounded-md text-[11px] font-bold transition-all ${
                  viewMode === 'graph' ? 'bg-zinc-800 text-purple-400 shadow-sm' : 'text-zinc-500 hover:text-zinc-300'
                }`}
              >
                <Share2 size={12} />
                GRAPH
              </button>
            </div>
          </div>

          <div className="flex items-center gap-2">
            {selectedNote && viewMode === 'editor' && (
              <button
                onClick={handleSave}
                disabled={isSaving}
                className={`flex items-center gap-2 px-4 py-1.5 rounded-md text-xs font-bold transition-all shadow-lg ${
                  isSaving 
                    ? 'bg-zinc-800 text-zinc-500 cursor-wait' 
                    : 'bg-blue-600 hover:bg-blue-500 text-white shadow-blue-600/20'
                }`}
              >
                <Save size={14} />
                {isSaving ? 'SAVING...' : 'SAVE NOTE'}
              </button>
            )}
          </div>
        </header>

        {/* Content Area */}
        <div className="flex-1 overflow-hidden relative">
          {!vaultPath ? (
            <div className="h-full flex flex-col items-center justify-center p-8 text-center bg-[radial-gradient(circle_at_center,_var(--tw-gradient-stops))] from-blue-500/5 via-transparent to-transparent">
              <div className="w-20 h-20 bg-blue-500/10 rounded-3xl flex items-center justify-center mb-8 border border-blue-500/20 shadow-2xl shadow-blue-500/10 rotate-3">
                <FolderOpen size={40} className="text-blue-500" />
              </div>
              <h2 className="text-2xl font-bold text-zinc-100 mb-3 tracking-tight">NexusBrain</h2>
              <p className="text-zinc-500 text-sm max-w-sm mb-10 leading-relaxed">
                Transform your Code Explorer Markdown vaults into an interactive, visual digital brain.
              </p>
              <button
                onClick={handleOpenVault}
                className="px-8 py-3 bg-blue-600 hover:bg-blue-500 text-white rounded-xl font-bold transition-all shadow-2xl shadow-blue-600/40 hover:scale-105 active:scale-95"
              >
                Select Vault Directory
              </button>
            </div>
          ) : viewMode === 'graph' ? (
            <GraphView />
          ) : !selectedNote ? (
            <div className="h-full flex items-center justify-center text-zinc-600 text-sm italic">
              Select a note from the sidebar or click a node in the graph
            </div>
          ) : (
            <MdEditor
              modelValue={noteContent}
              onChange={setNoteContent}
              theme="dark"
              language="en-US"
              className="h-full !bg-transparent"
              style={{ height: '100%' }}
              toolbars={[
                'bold', 'italic', 'title', 'underline', 'strikeThrough', 'quote', 'orderedList', 'unorderedList', 'link', 'image', 'table', 'mermaid', 'katex', 'code', 'preview', 'save'
              ]}
              onSave={handleSave}
            />
          )}
        </div>
      </main>
    </div>
  );
}

export default App;
