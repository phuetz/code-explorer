import { Moon, Sun } from 'lucide-react';
import type { AppTheme } from '../../hooks/use-theme';

interface Props {
  theme: AppTheme;
  onToggle: () => void;
}

export function ThemeToggle({ theme, onToggle }: Props) {
  const isLight = theme === 'light';
  const label = isLight ? 'Activer le thème sombre' : 'Activer le thème clair';
  const Icon = isLight ? Moon : Sun;

  return (
    <button
      type="button"
      onClick={onToggle}
      className="control-button flex items-center gap-1.5 rounded-md border px-2 py-1 text-xs transition"
      aria-label={label}
      title={label}
    >
      <Icon className="h-3.5 w-3.5" aria-hidden />
      <span className="hidden xl:inline">{isLight ? 'Sombre' : 'Clair'}</span>
    </button>
  );
}
