import {
  Braces,
  Box,
  CircleDot,
  Database,
  Diamond,
  File,
  Folder,
  Globe,
  Hash,
  Layers,
  Layout,
  type LucideIcon,
  Monitor,
  Route,
  Server,
  Table2,
} from "lucide-react";

const ICON_MAP: Record<string, { icon: LucideIcon; color: string }> = {
  Function:    { icon: Braces,    color: "#5b9cf6" },
  Method:      { icon: Braces,    color: "#67e8f9" },
  Constructor: { icon: Braces,    color: "#67e8f9" },
  Class:       { icon: Box,       color: "#a78bfa" },
  Struct:      { icon: Box,       color: "#fb923c" },
  Interface:   { icon: Diamond,   color: "#fbbf24" },
  Trait:       { icon: Diamond,   color: "#4ade80" },
  Enum:        { icon: Layers,    color: "#fb7185" },
  File:        { icon: File,      color: "#5a6477" },
  Module:      { icon: File,      color: "#5a6477" },
  Folder:      { icon: Folder,    color: "#5a6477" },
  Package:     { icon: Folder,    color: "#5a6477" },
  Variable:    { icon: Hash,      color: "#2dd4bf" },
  Property:    { icon: Hash,      color: "#2dd4bf" },
  Const:       { icon: Hash,      color: "#2dd4bf" },
  Type:        { icon: CircleDot, color: "#c1cad8" },
  Namespace:   { icon: Layers,    color: "#5a6477" },
  Community:   { icon: Layers,    color: "#4ade80" },
  Process:     { icon: Layers,    color: "#fbbf24" },
  // ASP.NET MVC 5 / EF6
  Controller:       { icon: Server,   color: "#818cf8" },
  ControllerAction: { icon: Route,    color: "#67e8f9" },
  ApiEndpoint:      { icon: Globe,    color: "#34d399" },
  View:             { icon: Layout,   color: "#f472b6" },
  ViewModel:        { icon: Monitor,  color: "#c084fc" },
  DbEntity:         { icon: Table2,   color: "#fb923c" },
  DbContext:        { icon: Database, color: "#fbbf24" },
  Area:             { icon: Layers,   color: "#94a3b8" },
};

const DEFAULT = { icon: CircleDot, color: "#5a6477" };

export function NodeIcon({ label, size = 16 }: { label: string; size?: number }) {
  const config = ICON_MAP[label] || DEFAULT;
  const Icon = config.icon;

  return (
    <div
      className="flex items-center justify-center rounded-md shrink-0"
      aria-hidden="true"
      style={{
        width: size + 8,
        height: size + 8,
        background: `${config.color}15`,
        color: config.color,
      }}
    >
      <Icon size={size} />
    </div>
  );
}

// eslint-disable-next-line react-refresh/only-export-components
export function getNodeColor(label: string): string {
  return (ICON_MAP[label] || DEFAULT).color;
}
