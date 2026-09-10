import React, { useRef } from 'react';
import ForceGraph2D from 'react-force-graph-2d';
import { useVaultStore } from '../stores/vault-store';

const GROUP_COLORS = {
  module: '#3b82f6',  // blue-500
  process: '#8b5cf6', // purple-500
  symbol: '#10b981',  // emerald-500
  file: '#6b7280',    // zinc-500
};

export const GraphView: React.FC = () => {
  const graphData = useVaultStore((s) => s.graphData);
  const setSelectedNote = useVaultStore((s) => s.setSelectedNote);
  const fgRef = useRef<any>();

  if (!graphData) return null;

  return (
    <div className="w-full h-full bg-zinc-950 relative">
      <ForceGraph2D
        ref={fgRef}
        graphData={graphData}
        nodeLabel="label"
        nodeColor={(n: any) => GROUP_COLORS[n.group as keyof typeof GROUP_COLORS] || '#ffffff'}
        nodeRelSize={6}
        linkDirectionalArrowLength={3}
        linkDirectionalArrowRelPos={1}
        linkColor={() => 'rgba(255, 255, 255, 0.1)'}
        onNodeClick={(node: any) => {
          // In this simple version, we assume ID is the filename
          // In a real app, we would map the ID back to the full relative path
          // For now, let's just trigger selection if we find a match
          setSelectedNote(node.id + '.md');
        }}
        cooldownTicks={100}
        onEngineStop={() => fgRef.current?.zoomToFit(400)}
      />
      <div className="absolute bottom-6 left-6 flex gap-4 bg-zinc-900/80 backdrop-blur-md p-3 rounded-lg border border-zinc-800 text-[10px] uppercase font-bold tracking-wider">
        <div className="flex items-center gap-2">
          <div className="w-2 h-2 rounded-full" style={{ background: GROUP_COLORS.module }}></div>
          <span>Modules</span>
        </div>
        <div className="flex items-center gap-2">
          <div className="w-2 h-2 rounded-full" style={{ background: GROUP_COLORS.process }}></div>
          <span>Processus</span>
        </div>
        <div className="flex items-center gap-2">
          <div className="w-2 h-2 rounded-full" style={{ background: GROUP_COLORS.symbol }}></div>
          <span>Symboles</span>
        </div>
      </div>
    </div>
  );
};
