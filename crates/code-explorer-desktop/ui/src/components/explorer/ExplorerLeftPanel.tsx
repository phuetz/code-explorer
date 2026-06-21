import { memo } from "react";
import { FolderTree } from "lucide-react";
import { FileTreeView } from "../files/FileTreeView";

export const ExplorerLeftPanel = memo(function ExplorerLeftPanel() {
  return (
    <div
      className="flex flex-col h-full overflow-hidden"
      style={{
        background: "var(--glass-bg)",
        backdropFilter: "blur(var(--glass-blur))",
        borderRight: "1px solid var(--glass-border)",
      }}
    >
      {/* File tree header */}
      <div
        className="flex items-center gap-2 w-full shrink-0"
        style={{
          padding: "8px 12px",
          fontSize: 11,
          fontWeight: 600,
          color: "var(--text-3)",
          textTransform: "uppercase",
          letterSpacing: "0.04em",
          borderBottom: "1px solid var(--surface-border)",
        }}
      >
        <FolderTree size={12} />
        Files
      </div>
      <div className="flex-1 min-h-0 overflow-auto">
        <FileTreeView />
      </div>
    </div>
  );
});
