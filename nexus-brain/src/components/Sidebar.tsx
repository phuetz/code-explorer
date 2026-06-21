import React, { useMemo } from 'react';
import { Folder, FileText, ChevronRight, ChevronDown, Brain } from 'lucide-react';
import { useVaultStore, VaultEntry } from '../stores/vault-store';

interface TreeItemProps {
  entry: VaultEntry;
  depth: number;
}

const TreeItem: React.FC<TreeItemProps> = ({ entry, depth }) => {
  const selectedNote = useVaultStore((s) => s.selectedNote);
  const setSelectedNote = useVaultStore((s) => s.setSelectedNote);
  const isActive = selectedNote === entry.path;

  const handleClick = () => {
    if (!entry.is_dir) {
      setSelectedNote(entry.path);
    }
  };

  return (
    <div
      onClick={handleClick}
      className={`flex items-center gap-2 py-1.5 px-3 rounded-md cursor-pointer transition-colors ${
        isActive 
          ? 'bg-blue-500/10 text-blue-400 border-l-2 border-blue-500' 
          : 'hover:bg-zinc-800 text-zinc-400'
      }`}
      style={{ marginLeft: `${depth * 12}px` }}
    >
      {entry.is_dir ? (
        <Folder size={14} className="text-zinc-500" />
      ) : (
        <FileText size={14} className={isActive ? 'text-blue-400' : 'text-zinc-500'} />
      )}
      <span className="text-xs font-medium truncate">{entry.name.replace('.md', '')}</span>
    </div>
  );
};

export const Sidebar: React.FC = () => {
  const entries = useVaultStore((s) => s.entries);
  const vaultPath = useVaultStore((s) => s.vaultPath);

  const sortedEntries = useMemo(() => {
    return [...entries].sort((a, b) => {
      if (a.is_dir !== b.is_dir) return b.is_dir ? 1 : -1;
      return a.name.localeCompare(b.name);
    });
  }, [entries]);

  return (
    <aside className="w-64 h-full border-r border-zinc-800 bg-zinc-900/50 flex flex-col">
      <div className="p-4 border-bottom border-zinc-800 flex items-center gap-2">
        <div className="p-1.5 bg-blue-500 rounded-lg shadow-lg shadow-blue-500/20">
          <Brain size={18} className="text-white" />
        </div>
        <h1 className="font-bold text-sm tracking-tight text-zinc-100">NexusBrain</h1>
      </div>

      <div className="flex-1 overflow-y-auto p-2 space-y-0.5">
        <div className="px-3 py-2 text-[10px] uppercase font-bold text-zinc-500 tracking-wider">
          Explorer
        </div>
        {sortedEntries.length === 0 ? (
          <div className="px-3 py-4 text-center">
            <p className="text-[11px] text-zinc-600 italic">No vault opened</p>
          </div>
        ) : (
          sortedEntries.map((entry) => (
            <TreeItem key={entry.path} entry={entry} depth={entry.path.split(/[\\/]/).filter(Boolean).length - 1} />
          ))
        )}
      </div>

      {vaultPath && (
        <div className="p-3 border-t border-zinc-800">
          <div className="text-[10px] text-zinc-500 truncate" title={vaultPath}>
            {vaultPath}
          </div>
        </div>
      )}
    </aside>
  );
};
