import { ChatPanel } from './components/chat/ChatPanel';
import {
  WorkspacePanel,
  type GraphTarget,
  type SourceTarget,
  type WorkspaceTab,
} from './components/explorer/WorkspacePanel';
import { useTheme } from './hooks/use-theme';

function App() {
  if (isDetachedWorkspaceRoute()) {
    return <DetachedWorkspaceApp />;
  }
  return <ChatPanel />;
}

export default App;

function DetachedWorkspaceApp() {
  const { theme } = useTheme();
  const params = new URLSearchParams(window.location.search);
  return (
    <div className={`theme-${theme} app-shell h-screen w-screen overflow-hidden`}>
      <WorkspacePanel
        detached
        initialTab={readWorkspaceTab(params)}
        initialSourceTarget={readSourceTarget(params)}
        initialGraphTarget={readGraphTarget(params)}
        onClose={() => window.close()}
      />
    </div>
  );
}

function isDetachedWorkspaceRoute(): boolean {
  return new URLSearchParams(window.location.search).get('codeExplorerPanel') === 'workspace';
}

function readWorkspaceTab(params: URLSearchParams): WorkspaceTab {
  return params.get('tab') === 'graph' ? 'graph' : 'sources';
}

function readSourceTarget(params: URLSearchParams): SourceTarget | null {
  const path = params.get('sourcePath');
  if (!path) return null;
  return {
    path,
    startLine: readPositiveInt(params.get('startLine')),
    endLine: readPositiveInt(params.get('endLine')),
  };
}

function readGraphTarget(params: URLSearchParams): GraphTarget | null {
  const nodeId = params.get('nodeId');
  const name = params.get('nodeName');
  if (!nodeId || !name) return null;
  return {
    nodeId,
    name,
    label: params.get('nodeLabel') ?? undefined,
    filePath: params.get('nodeFile') ?? undefined,
    startLine: readPositiveInt(params.get('nodeStart')),
    endLine: readPositiveInt(params.get('nodeEnd')),
  };
}

function readPositiveInt(value: string | null): number | undefined {
  if (!value) return undefined;
  const parsed = Number(value);
  return Number.isInteger(parsed) && parsed > 0 ? parsed : undefined;
}
