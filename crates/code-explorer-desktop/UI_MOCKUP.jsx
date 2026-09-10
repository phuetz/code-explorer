import { useState } from "react";
import { Search, GitBranch, FolderTree, Network, Zap, FileText, Settings, PanelLeftClose, ChevronRight, ChevronDown, Maximize2, Circle, ArrowRight, ExternalLink, Folder, File } from "lucide-react";

// ─── Design Tokens ───
const T = {
  bg0: "#0a0c12", bg1: "#0e1119", bg2: "#141821", bg3: "#1b2030", bg4: "#242a3a",
  surface: "#12161f", surfaceHover: "#1a1f2e", surfaceBorder: "rgba(148,163,194,0.08)",
  surfaceBorderHover: "rgba(148,163,194,0.16)", surfaceElevated: "#161b28",
  text0: "#eaeff7", text1: "#c8d1e0", text2: "#8e99b0", text3: "#5c677d", text4: "#3d4558",
  accent: "#6aa1f8", accentHover: "#83b3fa", accentSubtle: "rgba(106,161,248,0.10)",
  green: "#4ade80", greenSubtle: "rgba(74,222,128,0.10)",
  amber: "#fbbf24", amberSubtle: "rgba(251,191,36,0.10)",
  rose: "#fb7185", roseSubtle: "rgba(251,113,133,0.10)",
  purple: "#a78bfa", purpleSubtle: "rgba(167,139,250,0.10)",
  cyan: "#67e8f9", cyanSubtle: "rgba(103,232,249,0.10)",
  teal: "#2dd4bf", orange: "#fb923c",
  fontDisplay: "'Outfit', system-ui, sans-serif",
  fontBody: "'DM Sans', system-ui, sans-serif",
  fontMono: "'JetBrains Mono', monospace",
  radiusSm: 6, radiusMd: 8, radiusLg: 12, radiusXl: 16, radiusFull: 9999,
  shadowMd: "0 4px 12px rgba(0,0,0,0.25), 0 1px 3px rgba(0,0,0,0.15)",
  shadowLg: "0 8px 32px rgba(0,0,0,0.35), 0 2px 8px rgba(0,0,0,0.2)",
};

const NODE_COLORS = {
  Function: T.cyan, Class: T.amber, Method: T.cyan, Interface: T.green,
  Struct: T.purple, Module: T.text3, Package: T.text4, Enum: T.rose,
  Import: T.rose, Export: T.green, Variable: T.teal, Constant: T.purple,
};

// ─── Mock Data ───
const MOCK_NODES = [
  { id: "1", name: "run_pipeline", label: "Function", x: 320, y: 240, size: 28 },
  { id: "2", name: "Parser", label: "Class", x: 520, y: 180, size: 28 },
  { id: "3", name: "parse_file", label: "Function", x: 480, y: 340, size: 22 },
  { id: "4", name: "resolve_imports", label: "Function", x: 200, y: 360, size: 22 },
  { id: "5", name: "detect_communities", label: "Function", x: 160, y: 180, size: 22 },
  { id: "6", name: "ingest", label: "Module", x: 360, y: 120, size: 34 },
  { id: "7", name: "SymbolTable", label: "Struct", x: 580, y: 300, size: 26 },
  { id: "8", name: "KnowledgeGraph", label: "Class", x: 440, y: 440, size: 30 },
];
const MOCK_EDGES = [
  { from: "1", to: "3" }, { from: "1", to: "4" }, { from: "1", to: "5" },
  { from: "6", to: "1" }, { from: "2", to: "3" }, { from: "3", to: "7" },
  { from: "8", to: "7" }, { from: "4", to: "8" },
];

// ─── App ───
export default function CodeExplorerMockup() {
  const [activeTab, setActiveTab] = useState("graph");
  const [selectedNode, setSelectedNode] = useState(MOCK_NODES[0]);
  const [detailTab, setDetailTab] = useState("context");
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false);
  const [zoomLevel, setZoomLevel] = useState("module");
  const [hoveredNode, setHoveredNode] = useState(null);
  const [collapsedSections, setCollapsedSections] = useState({ callers: false, callees: false, community: true });

  const toggleSection = (key) => setCollapsedSections(prev => ({ ...prev, [key]: !prev[key] }));
  const sideW = sidebarCollapsed ? 52 : 220;

  return (
    <div style={{ width: "100%", height: "100vh", display: "flex", flexDirection: "column", background: T.bg0, fontFamily: T.fontBody, color: T.text1, fontSize: 13, overflow: "hidden" }}>
      <link href="https://fonts.googleapis.com/css2?family=Outfit:wght@400;500;600;700&family=DM+Sans:wght@400;500;600&family=JetBrains+Mono:wght@400;500&display=swap" rel="stylesheet" />

      {/* ═══ CommandBar ═══ */}
      <div style={{ height: 46, display: "flex", alignItems: "center", justifyContent: "space-between", padding: "0 16px", background: T.bg1, borderBottom: `1px solid ${T.surfaceBorder}`, flexShrink: 0 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
          <span style={{ display: "inline-flex", alignItems: "center", gap: 6, padding: "2px 10px", borderRadius: T.radiusFull, background: T.bg3, fontSize: 12, fontWeight: 500, color: T.text1, fontFamily: T.fontDisplay }}>
            <span style={{ width: 6, height: 6, borderRadius: "50%", background: T.green, boxShadow: `0 0 6px ${T.green}` }} />
            code-explorer
          </span>
          <ChevronRight size={12} color={T.text4} />
          <span style={{ padding: "2px 10px", borderRadius: T.radiusFull, background: T.accentSubtle, fontSize: 12, fontWeight: 500, color: T.accent }}>
            Graph Explorer
          </span>
          {selectedNode && (
            <>
              <ChevronRight size={12} color={T.text4} />
              <span style={{ padding: "2px 10px", borderRadius: T.radiusFull, background: T.purpleSubtle, fontSize: 12, fontWeight: 500, color: T.purple }}>
                {selectedNode.name}
              </span>
            </>
          )}
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 8, padding: "6px 14px", borderRadius: T.radiusMd, background: T.bg3, border: `1px solid ${T.surfaceBorder}`, cursor: "pointer", minWidth: 240 }}>
          <Search size={14} color={T.text3} />
          <span style={{ fontSize: 12, color: T.text3, flex: 1 }}>Search symbols…</span>
          <span style={{ fontSize: 10, color: T.text4, padding: "1px 6px", borderRadius: 4, background: T.bg2, fontFamily: T.fontMono }}>⌘K</span>
        </div>
      </div>

      <div style={{ display: "flex", flex: 1, minHeight: 0 }}>
        {/* ═══ Sidebar ═══ */}
        <div style={{ width: sideW, flexShrink: 0, display: "flex", flexDirection: "column", background: T.bg1, borderRight: `1px solid ${T.surfaceBorder}`, transition: "width 280ms cubic-bezier(0.16,1,0.3,1)" }}>
          {/* Logo */}
          <div style={{ display: "flex", alignItems: "center", gap: 10, padding: "0 16px", height: 52, flexShrink: 0 }}>
            <div style={{ width: 28, height: 28, borderRadius: T.radiusMd, display: "flex", alignItems: "center", justifyContent: "center", background: "linear-gradient(135deg, #6aa1f8, #8b5cf6)", color: "white", fontFamily: T.fontDisplay, fontWeight: 700, fontSize: 14, flexShrink: 0 }}>G</div>
            {!sidebarCollapsed && <span style={{ fontSize: 14, fontWeight: 600, color: T.text0, fontFamily: T.fontDisplay, letterSpacing: "-0.02em" }}>Code Explorer</span>}
            <button onClick={() => setSidebarCollapsed(!sidebarCollapsed)} style={{ marginLeft: "auto", padding: 4, borderRadius: T.radiusSm, border: "none", background: "transparent", color: T.text3, cursor: "pointer", flexShrink: 0 }}>
              <PanelLeftClose size={16} />
            </button>
          </div>
          <div style={{ height: 1, background: T.surfaceBorder, margin: "0 12px" }} />

          {/* Nav */}
          <div style={{ flex: 1, overflow: "auto", padding: "8px 12px" }}>
            {!sidebarCollapsed && <div style={{ padding: "12px 10px 6px", fontSize: 10, fontWeight: 600, textTransform: "uppercase", letterSpacing: "0.08em", color: T.text3, fontFamily: T.fontDisplay }}>Workspace</div>}
            <SidebarItem icon={<GitBranch size={16} />} label="Repositories" active={activeTab === "repos"} collapsed={sidebarCollapsed} onClick={() => setActiveTab("repos")} />
            <SidebarItem icon={<FolderTree size={16} />} label="File Explorer" active={activeTab === "files"} collapsed={sidebarCollapsed} onClick={() => setActiveTab("files")} />
            <div style={{ height: 1, background: T.surfaceBorder, margin: "12px 0" }} />
            {!sidebarCollapsed && <div style={{ padding: "4px 10px 6px", fontSize: 10, fontWeight: 600, textTransform: "uppercase", letterSpacing: "0.08em", color: T.text3, fontFamily: T.fontDisplay }}>Explore</div>}
            <SidebarItem icon={<Network size={16} />} label="Graph Explorer" active={activeTab === "graph"} collapsed={sidebarCollapsed} onClick={() => setActiveTab("graph")} />
            <SidebarItem icon={<Zap size={16} />} label="Impact Analysis" active={activeTab === "impact"} collapsed={sidebarCollapsed} onClick={() => setActiveTab("impact")} />
            <SidebarItem icon={<FileText size={16} />} label="Documentation" active={activeTab === "docs"} collapsed={sidebarCollapsed} onClick={() => setActiveTab("docs")} />
          </div>
          <div style={{ height: 1, background: T.surfaceBorder, margin: "0 12px" }} />
          <div style={{ padding: "10px 12px", flexShrink: 0 }}>
            <SidebarItem icon={<Settings size={16} />} label="Settings" active={false} collapsed={sidebarCollapsed} onClick={() => {}} />
          </div>
        </div>

        {/* ═══ Main Content ═══ */}
        <div style={{ flex: 1, display: "flex", minWidth: 0 }}>
          {/* Graph Area */}
          <div style={{ flex: 1, display: "flex", flexDirection: "column", minWidth: 0 }}>
            {/* GraphToolbar */}
            <div style={{ height: 40, display: "flex", alignItems: "center", gap: 12, padding: "0 16px", background: T.bg1, borderBottom: `1px solid ${T.surfaceBorder}`, flexShrink: 0 }}>
              {/* Zoom pills */}
              <div style={{ display: "flex", padding: 2, borderRadius: T.radiusFull, background: T.bg3 }}>
                {["package", "module", "symbol"].map(z => (
                  <button key={z} onClick={() => setZoomLevel(z)} style={{ padding: "4px 12px", borderRadius: T.radiusFull, border: "none", cursor: "pointer", fontSize: 11, fontWeight: 500, fontFamily: T.fontBody, background: zoomLevel === z ? T.accent : "transparent", color: zoomLevel === z ? "white" : T.text2, transition: "all 120ms ease-out" }}>
                    {z.charAt(0).toUpperCase() + z.slice(1)}s
                  </button>
                ))}
              </div>
              <div style={{ width: 1, height: 20, background: T.surfaceBorder }} />
              {/* Layout */}
              <div style={{ display: "flex", alignItems: "center", gap: 4, padding: "4px 10px", borderRadius: T.radiusSm, background: T.bg3, border: `1px solid ${T.surfaceBorder}`, fontSize: 11, color: T.text2, cursor: "pointer" }}>
                Force <ChevronDown size={12} />
              </div>
              <button style={{ padding: 6, borderRadius: T.radiusSm, border: `1px solid ${T.surfaceBorder}`, background: T.bg3, color: T.text2, cursor: "pointer", display: "flex" }}>
                <Maximize2 size={14} />
              </button>
              <div style={{ flex: 1 }} />
              <span style={{ fontSize: 10, fontFamily: T.fontMono, color: T.text3 }}>8 nodes</span>
              <span style={{ fontSize: 10, fontFamily: T.fontMono, color: T.text3 }}>8 edges</span>
            </div>

            {/* Graph Canvas */}
            <div style={{ flex: 1, position: "relative", background: `radial-gradient(circle at 50% 50%, rgba(106,161,248,0.02) 0%, transparent 70%)`, backgroundSize: "100% 100%", overflow: "hidden" }}>
              {/* Dot grid */}
              <div style={{ position: "absolute", inset: 0, backgroundImage: `radial-gradient(circle, ${T.bg4} 0.5px, transparent 0.5px)`, backgroundSize: "20px 20px" }} />
              {/* Edges */}
              <svg style={{ position: "absolute", inset: 0, width: "100%", height: "100%" }}>
                {MOCK_EDGES.map((e, i) => {
                  const from = MOCK_NODES.find(n => n.id === e.from);
                  const to = MOCK_NODES.find(n => n.id === e.to);
                  if (!from || !to) return null;
                  const isSelected = selectedNode && (from.id === selectedNode.id || to.id === selectedNode.id);
                  return <line key={i} x1={from.x} y1={from.y} x2={to.x} y2={to.y} stroke={isSelected ? T.accent : T.bg4} strokeWidth={isSelected ? 1.5 : 0.8} opacity={isSelected ? 1 : 0.5} />;
                })}
              </svg>
              {/* Nodes */}
              {MOCK_NODES.map(node => {
                const color = NODE_COLORS[node.label] || T.text3;
                const isSelected = selectedNode?.id === node.id;
                const isHovered = hoveredNode === node.id;
                const s = node.size * (isSelected ? 1.25 : isHovered ? 1.1 : 1);
                return (
                  <div key={node.id} onClick={() => setSelectedNode(node)} onMouseEnter={() => setHoveredNode(node.id)} onMouseLeave={() => setHoveredNode(null)}
                    style={{ position: "absolute", left: node.x - s/2, top: node.y - s/2, width: s, height: s, borderRadius: node.label === "Module" || node.label === "Package" ? T.radiusSm : "50%", background: color, border: isSelected ? `3px solid ${T.accent}` : `2px solid rgba(0,0,0,0.3)`, boxShadow: isSelected ? `0 0 20px ${T.accentSubtle}, 0 0 8px rgba(106,161,248,0.15)` : isHovered ? `0 0 12px ${color}33` : "none", cursor: "pointer", transition: "all 150ms cubic-bezier(0.16,1,0.3,1)", zIndex: isSelected ? 10 : isHovered ? 5 : 1 }}>
                  </div>
                );
              })}
              {/* Node Labels */}
              {MOCK_NODES.map(node => (
                <div key={`label-${node.id}`} style={{ position: "absolute", left: node.x, top: node.y + node.size / 2 + 8, transform: "translateX(-50%)", fontSize: 11, color: T.text1, fontWeight: selectedNode?.id === node.id ? 600 : 400, whiteSpace: "nowrap", pointerEvents: "none", textShadow: `0 1px 4px ${T.bg0}` }}>
                  {node.name}
                </div>
              ))}
              {/* Hover Tooltip */}
              {hoveredNode && !selectedNode?.id !== hoveredNode && (() => {
                const n = MOCK_NODES.find(x => x.id === hoveredNode);
                if (!n || n.id === selectedNode?.id) return null;
                return (
                  <div style={{ position: "absolute", left: n.x, top: n.y - 50, transform: "translateX(-50%)", padding: "8px 14px", borderRadius: T.radiusMd, background: "rgba(14,17,24,0.92)", backdropFilter: "blur(12px)", border: `1px solid ${T.surfaceBorder}`, boxShadow: T.shadowLg, zIndex: 20, pointerEvents: "none" }}>
                    <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
                      <span style={{ width: 8, height: 8, borderRadius: "50%", background: NODE_COLORS[n.label] || T.text3 }} />
                      <span style={{ fontSize: 12, fontWeight: 500, color: T.text0 }}>{n.name}</span>
                      <span style={{ fontSize: 10, color: T.text3, padding: "1px 6px", borderRadius: 4, background: T.bg3 }}>{n.label}</span>
                    </div>
                    <div style={{ fontSize: 10, color: T.text4, marginTop: 4, fontFamily: T.fontMono }}>src/ingest/{n.name.toLowerCase()}.rs</div>
                  </div>
                );
              })()}

              {/* Graph Legend (bottom-left) */}
              <div style={{ position: "absolute", bottom: 16, left: 16, padding: "10px 14px", borderRadius: T.radiusMd, background: "rgba(14,17,24,0.88)", backdropFilter: "blur(12px)", border: `1px solid ${T.surfaceBorder}`, zIndex: 15 }}>
                <div style={{ fontSize: 10, fontWeight: 600, color: T.text3, textTransform: "uppercase", letterSpacing: "0.05em", marginBottom: 8 }}>Legend</div>
                {["Function", "Class", "Module", "Struct"].map(type => (
                  <div key={type} style={{ display: "flex", alignItems: "center", gap: 8, padding: "2px 0" }}>
                    <span style={{ width: 8, height: 8, borderRadius: type === "Module" ? 2 : "50%", background: NODE_COLORS[type] }} />
                    <span style={{ fontSize: 11, color: T.text2 }}>{type}</span>
                  </div>
                ))}
              </div>

              {/* Minimap (bottom-right) */}
              <div style={{ position: "absolute", bottom: 16, right: 16, width: 160, height: 110, borderRadius: T.radiusMd, background: T.bg2, border: `1px solid ${T.surfaceBorder}`, opacity: 0.85, overflow: "hidden", zIndex: 15 }}>
                <svg width="160" height="110" style={{ position: "absolute", inset: 0 }}>
                  {MOCK_EDGES.map((e, i) => {
                    const from = MOCK_NODES.find(n => n.id === e.from);
                    const to = MOCK_NODES.find(n => n.id === e.to);
                    if (!from || !to) return null;
                    return <line key={i} x1={from.x * 0.22 + 10} y1={from.y * 0.2 + 5} x2={to.x * 0.22 + 10} y2={to.y * 0.2 + 5} stroke={T.bg4} strokeWidth={0.5} />;
                  })}
                  {MOCK_NODES.map(n => (
                    <circle key={n.id} cx={n.x * 0.22 + 10} cy={n.y * 0.2 + 5} r={3} fill={NODE_COLORS[n.label] || T.text3} />
                  ))}
                  <rect x={20} y={15} width={80} height={60} fill="none" stroke={T.accent} strokeWidth={1} strokeDasharray="3,2" rx={2} opacity={0.6} />
                </svg>
              </div>
            </div>
          </div>

          {/* ═══ Detail Panel ═══ */}
          <div style={{ width: 380, flexShrink: 0, display: "flex", flexDirection: "column", background: T.bg0, borderLeft: `1px solid ${T.surfaceBorder}` }}>
            {/* Detail Tab Bar */}
            <div style={{ display: "flex", gap: 6, padding: "12px 16px", background: T.bg1, borderBottom: `1px solid ${T.surfaceBorder}`, flexShrink: 0 }}>
              {["context", "code", "properties"].map(tab => (
                <button key={tab} onClick={() => setDetailTab(tab)} style={{ padding: "6px 14px", borderRadius: T.radiusFull, border: "none", cursor: "pointer", fontSize: 12, fontWeight: 500, fontFamily: T.fontBody, background: detailTab === tab ? T.accent : T.bg3, color: detailTab === tab ? "white" : T.text2, transition: "all 120ms ease-out" }}>
                  {tab.charAt(0).toUpperCase() + tab.slice(1)}
                </button>
              ))}
            </div>

            {/* Detail Content */}
            <div style={{ flex: 1, overflow: "auto", padding: 16 }}>
              {selectedNode && detailTab === "context" && (
                <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
                  {/* Node Header Card */}
                  <div style={{ borderRadius: T.radiusMd, padding: 14, background: T.bg1, borderLeft: `4px solid ${NODE_COLORS[selectedNode.label] || T.accent}` }}>
                    <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 8 }}>
                      <span style={{ padding: "2px 8px", borderRadius: 4, fontSize: 11, fontWeight: 500, color: "white", background: NODE_COLORS[selectedNode.label] || T.accent }}>{selectedNode.label}</span>
                      <span style={{ padding: "2px 8px", borderRadius: 4, fontSize: 11, fontWeight: 500, color: "white", background: T.green }}>exported</span>
                    </div>
                    <div style={{ fontSize: 16, fontWeight: 600, color: T.text0, fontFamily: T.fontDisplay, marginBottom: 4 }}>{selectedNode.name}</div>
                    <div style={{ fontSize: 11, color: T.text3, fontFamily: T.fontMono }}>src/pipeline.rs:42-120</div>
                  </div>

                  {/* Collapsible: Callers */}
                  <CollapsibleSection title="Callers" count={1} collapsed={collapsedSections.callers} onToggle={() => toggleSection("callers")}>
                    <RelationCard type="Module" name="ingest" path="src/ingest/mod.rs" color={NODE_COLORS.Module} />
                  </CollapsibleSection>

                  {/* Collapsible: Callees */}
                  <CollapsibleSection title="Callees" count={3} collapsed={collapsedSections.callees} onToggle={() => toggleSection("callees")}>
                    <RelationCard type="Function" name="parse_file" path="src/parser.rs" color={NODE_COLORS.Function} />
                    <RelationCard type="Function" name="resolve_imports" path="src/ingest/imports.rs" color={NODE_COLORS.Function} />
                    <RelationCard type="Function" name="detect_communities" path="src/ingest/community.rs" color={NODE_COLORS.Function} />
                  </CollapsibleSection>

                  {/* Collapsible: Community */}
                  <CollapsibleSection title="Community" count={null} collapsed={collapsedSections.community} onToggle={() => toggleSection("community")}>
                    <div style={{ borderRadius: T.radiusMd, padding: 12, background: T.bg1, borderLeft: `4px solid ${T.purple}` }}>
                      <div style={{ fontSize: 14, fontWeight: 500, color: T.text0 }}>Pipeline Core</div>
                      <div style={{ fontSize: 11, color: T.text3, marginTop: 4 }}>Main ingestion pipeline</div>
                      <div style={{ display: "flex", gap: 12, marginTop: 8, fontSize: 11, color: T.text2 }}>
                        <span>6 members</span>
                        <span>Cohesion: 0.85</span>
                      </div>
                    </div>
                  </CollapsibleSection>
                </div>
              )}

              {selectedNode && detailTab === "code" && (
                <div style={{ fontFamily: T.fontMono, fontSize: 12, lineHeight: 1.7, color: T.text1 }}>
                  <div style={{ padding: "8px 12px", borderRadius: T.radiusMd, background: T.bg1, marginBottom: 12 }}>
                    <span style={{ color: T.text3 }}>src/pipeline.rs</span>
                    <span style={{ color: T.text4 }}> : 42-120</span>
                  </div>
                  <pre style={{ padding: 12, borderRadius: T.radiusMd, background: T.bg1, overflow: "auto", fontSize: 12, lineHeight: 1.7 }}>
{`pub fn run_pipeline(
    graph: &mut KnowledgeGraph,
    config: &PipelineConfig,
) -> Result<PipelineStats> {
    let phases = [
        Phase::Structure,
        Phase::Parsing,
        Phase::Imports,
        Phase::Calls,
        Phase::Heritage,
        Phase::Community,
    ];

    for phase in &phases {
        phase.execute(graph, config)?;
    }

    Ok(graph.stats())
}`}
                  </pre>
                </div>
              )}

              {selectedNode && detailTab === "properties" && (
                <div style={{ display: "grid", gap: 8 }}>
                  {[["ID", selectedNode.id], ["Label", selectedNode.label], ["Name", selectedNode.name], ["File", "src/pipeline.rs"], ["Lines", "42-120"], ["Language", "Rust"], ["Exported", "true"], ["Parameters", "2"], ["Return Type", "Result<PipelineStats>"]].map(([k, v]) => (
                    <div key={k} style={{ padding: 12, borderRadius: T.radiusMd, background: T.bg1, border: `1px solid ${T.surfaceBorder}` }}>
                      <div style={{ fontSize: 11, fontWeight: 500, color: T.text2, marginBottom: 4 }}>{k}</div>
                      <div style={{ fontSize: 12, color: T.text0, fontFamily: k === "File" || k === "Return Type" ? T.fontMono : T.fontBody }}>{v}</div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          </div>
        </div>
      </div>

      {/* ═══ StatusBar ═══ */}
      <div style={{ height: 28, display: "flex", alignItems: "center", justifyContent: "space-between", padding: "0 16px", background: T.bg1, borderTop: `1px solid ${T.surfaceBorder}`, fontSize: 10, color: T.text3, flexShrink: 0 }}>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span style={{ display: "flex", alignItems: "center", gap: 4 }}>
            <span style={{ width: 6, height: 6, borderRadius: "50%", background: T.green, animation: "pulse 2s ease-in-out infinite" }} />
            code-explorer
          </span>
          <span style={{ color: T.text4 }}>|</span>
          <span>Zoom: {zoomLevel}</span>
          <span style={{ color: T.text4 }}>|</span>
          <span>Active: 1</span>
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span style={{ fontFamily: T.fontMono }}>3.8k nodes · 12.7k edges</span>
          <span style={{ color: T.text4 }}>|</span>
          <span>Code Explorer v0.1.0</span>
        </div>
      </div>

      <style>{`@keyframes pulse { 0%,100% { opacity: 0.6; } 50% { opacity: 1; } }`}</style>
    </div>
  );
}

// ─── Sub-components ───

function SidebarItem({ icon, label, active, collapsed, onClick }) {
  const [hovered, setHovered] = useState(false);
  return (
    <button onClick={onClick} onMouseEnter={() => setHovered(true)} onMouseLeave={() => setHovered(false)}
      style={{ width: "100%", display: "flex", alignItems: "center", gap: 10, padding: collapsed ? 8 : "7px 10px", justifyContent: collapsed ? "center" : "flex-start", borderRadius: T.radiusMd, border: "none", cursor: "pointer", fontFamily: T.fontBody, fontSize: 13, fontWeight: 500, position: "relative", overflow: "hidden", transition: "all 120ms ease-out",
        background: active ? T.accentSubtle : hovered ? T.surfaceHover : "transparent",
        color: active ? T.accent : hovered ? T.text1 : T.text2 }}>
      {active && (
        <>
          <div style={{ position: "absolute", left: 0, top: "50%", transform: "translateY(-50%)", width: 2.5, height: 20, borderRadius: "0 4px 4px 0", background: `linear-gradient(180deg, transparent, ${T.accent}, transparent)`, boxShadow: `0 0 12px rgba(106,161,248,0.15)` }} />
          <div style={{ position: "absolute", inset: 0, background: `radial-gradient(80px circle at left, rgba(106,161,248,0.04), transparent)`, pointerEvents: "none" }} />
        </>
      )}
      <span style={{ position: "relative", zIndex: 1, display: "flex" }}>{icon}</span>
      {!collapsed && <span style={{ position: "relative", zIndex: 1 }}>{label}</span>}
    </button>
  );
}

function CollapsibleSection({ title, count, collapsed, onToggle, children }) {
  return (
    <div>
      <button onClick={onToggle} style={{ display: "flex", alignItems: "center", gap: 6, width: "100%", padding: "4px 0", border: "none", background: "transparent", cursor: "pointer", color: T.text2 }}>
        {collapsed ? <ChevronRight size={14} /> : <ChevronDown size={14} />}
        <span style={{ fontSize: 11, fontWeight: 600, textTransform: "uppercase", letterSpacing: "0.06em" }}>{title}</span>
        {count !== null && <span style={{ fontSize: 11, color: T.text3 }}>({count})</span>}
      </button>
      {!collapsed && <div style={{ display: "flex", flexDirection: "column", gap: 6, marginTop: 8 }}>{children}</div>}
    </div>
  );
}

function RelationCard({ type, name, path, color }) {
  const [hovered, setHovered] = useState(false);
  return (
    <button onMouseEnter={() => setHovered(true)} onMouseLeave={() => setHovered(false)}
      style={{ display: "flex", alignItems: "flex-start", gap: 8, width: "100%", padding: "8px 12px", borderRadius: T.radiusMd, border: `1px solid ${hovered ? T.surfaceBorderHover : T.surfaceBorder}`, background: hovered ? T.surfaceHover : T.bg1, cursor: "pointer", textAlign: "left", transition: "all 120ms ease-out", fontFamily: T.fontBody }}>
      <span style={{ padding: "1px 6px", borderRadius: 4, fontSize: 10, fontWeight: 500, background: T.bg3, color: T.text2, flexShrink: 0 }}>{type}</span>
      <div style={{ minWidth: 0 }}>
        <div style={{ fontSize: 12, fontWeight: 500, color: T.text0 }}>{name}</div>
        <div style={{ fontSize: 10, color: T.text3, fontFamily: T.fontMono, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{path}</div>
      </div>
    </button>
  );
}
