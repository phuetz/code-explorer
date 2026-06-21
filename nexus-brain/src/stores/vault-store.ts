import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';

export interface VaultEntry {
  name: string;
  path: string;
  is_dir: boolean;
}

export interface GraphNode {
  id: string;
  label: string;
  group: 'module' | 'process' | 'symbol' | 'file';
}

export interface GraphEdge {
  source: string;
  target: string;
}

export interface VaultGraph {
  nodes: GraphNode[];
  links: GraphEdge[];
}

interface VaultState {
  vaultPath: string | null;
  entries: VaultEntry[];
  selectedNote: string | null;
  viewMode: 'editor' | 'graph';
  graphData: VaultGraph | null;
  
  setVaultPath: (path: string) => Promise<void>;
  setSelectedNote: (path: string | null) => void;
  setViewMode: (mode: 'editor' | 'graph') => void;
  refreshVault: () => Promise<void>;
  loadGraph: () => Promise<void>;
}

export const useVaultStore = create<VaultState>((set, get) => ({
  vaultPath: null,
  entries: [],
  selectedNote: null,
  viewMode: 'editor',
  graphData: null,

  setVaultPath: async (path: string) => {
    set({ vaultPath: path });
    await get().refreshVault();
    await get().loadGraph();
  },

  setSelectedNote: (path: string | null) => {
    set({ selectedNote: path, viewMode: 'editor' });
  },

  setViewMode: (mode: 'editor' | 'graph') => {
    set({ viewMode: mode });
  },

  refreshVault: async () => {
    const { vaultPath } = get();
    if (!vaultPath) return;
    try {
      const entries = await invoke<VaultEntry[]>('list_vault', { path: vaultPath });
      set({ entries });
    } catch (err) {
      console.error("Failed to list vault:", err);
    }
  },

  loadGraph: async () => {
    const { vaultPath } = get();
    if (!vaultPath) return;
    try {
      const data = await invoke<any>('get_vault_graph', { vaultPath });
      // Rename 'edges' to 'links' for react-force-graph compatibility
      set({ graphData: { nodes: data.nodes, links: data.edges } });
    } catch (err) {
      console.error("Failed to load graph:", err);
    }
  }
}));
