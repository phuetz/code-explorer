import { useEffect, useState } from 'react';
import SyntaxHighlighter from 'react-syntax-highlighter/dist/esm/prism-light';
import { oneLight, vscDarkPlus } from 'react-syntax-highlighter/dist/esm/styles/prism';
import bash from 'react-syntax-highlighter/dist/esm/languages/prism/bash';
import c from 'react-syntax-highlighter/dist/esm/languages/prism/c';
import cpp from 'react-syntax-highlighter/dist/esm/languages/prism/cpp';
import csharp from 'react-syntax-highlighter/dist/esm/languages/prism/csharp';
import css from 'react-syntax-highlighter/dist/esm/languages/prism/css';
import diff from 'react-syntax-highlighter/dist/esm/languages/prism/diff';
import go from 'react-syntax-highlighter/dist/esm/languages/prism/go';
import java from 'react-syntax-highlighter/dist/esm/languages/prism/java';
import javascript from 'react-syntax-highlighter/dist/esm/languages/prism/javascript';
import json from 'react-syntax-highlighter/dist/esm/languages/prism/json';
import jsx from 'react-syntax-highlighter/dist/esm/languages/prism/jsx';
import kotlin from 'react-syntax-highlighter/dist/esm/languages/prism/kotlin';
import markup from 'react-syntax-highlighter/dist/esm/languages/prism/markup';
import powershell from 'react-syntax-highlighter/dist/esm/languages/prism/powershell';
import python from 'react-syntax-highlighter/dist/esm/languages/prism/python';
import ruby from 'react-syntax-highlighter/dist/esm/languages/prism/ruby';
import rust from 'react-syntax-highlighter/dist/esm/languages/prism/rust';
import sql from 'react-syntax-highlighter/dist/esm/languages/prism/sql';
import swift from 'react-syntax-highlighter/dist/esm/languages/prism/swift';
import tsx from 'react-syntax-highlighter/dist/esm/languages/prism/tsx';
import typescript from 'react-syntax-highlighter/dist/esm/languages/prism/typescript';
import yaml from 'react-syntax-highlighter/dist/esm/languages/prism/yaml';

const LANGUAGES = {
  bash,
  c,
  cpp,
  csharp,
  css,
  diff,
  go,
  java,
  javascript,
  json,
  jsx,
  kotlin,
  markup,
  powershell,
  python,
  ruby,
  rust,
  sql,
  swift,
  tsx,
  typescript,
  yaml,
} as const;

type CodeLanguage = keyof typeof LANGUAGES;

const LANGUAGE_ALIASES = new Map<string, CodeLanguage>([
  ['c#', 'csharp'],
  ['cs', 'csharp'],
  ['html', 'markup'],
  ['htm', 'markup'],
  ['cshtml', 'markup'],
  ['xml', 'markup'],
  ['js', 'javascript'],
  ['mjs', 'javascript'],
  ['cjs', 'javascript'],
  ['ts', 'typescript'],
  ['kt', 'kotlin'],
  ['kts', 'kotlin'],
  ['rs', 'rust'],
  ['py', 'python'],
  ['rb', 'ruby'],
  ['ps1', 'powershell'],
  ['pwsh', 'powershell'],
  ['sh', 'bash'],
  ['shell', 'bash'],
  ['yml', 'yaml'],
]);

Object.entries(LANGUAGES).forEach(([name, grammar]) => {
  SyntaxHighlighter.registerLanguage(name, grammar);
});

export { SyntaxHighlighter };
export type { CodeLanguage };

export function resolveCodeLanguage(language?: string | null, path?: string | null): CodeLanguage | null {
  for (const candidate of [language, extensionFromPath(path)]) {
    const resolved = resolveLanguageCandidate(candidate);
    if (resolved) return resolved;
  }
  return null;
}

export function codeThemeStyle(theme: 'light' | 'dark') {
  return theme === 'dark' ? vscDarkPlus : oneLight;
}

export function useDocumentCodeTheme(): 'light' | 'dark' {
  const [theme, setTheme] = useState<'light' | 'dark'>(() => readDocumentCodeTheme());

  useEffect(() => {
    const root = document.documentElement;
    const syncTheme = () => setTheme(readDocumentCodeTheme());
    const observer = new MutationObserver(syncTheme);
    observer.observe(root, { attributes: true, attributeFilter: ['data-theme'] });
    syncTheme();
    return () => observer.disconnect();
  }, []);

  return theme;
}

function resolveLanguageCandidate(value?: string | null): CodeLanguage | null {
  if (!value) return null;
  const normalized = value.trim().toLowerCase().replace(/^language-/, '');
  if (normalized in LANGUAGES) return normalized as CodeLanguage;
  return LANGUAGE_ALIASES.get(normalized) ?? null;
}

function extensionFromPath(path?: string | null): string | null {
  if (!path) return null;
  const name = path.split(/[\\/]/).pop() ?? path;
  const dot = name.lastIndexOf('.');
  return dot >= 0 ? name.slice(dot + 1) : null;
}

function readDocumentCodeTheme(): 'light' | 'dark' {
  if (typeof document === 'undefined') return 'light';
  return document.documentElement.dataset.theme === 'dark' ? 'dark' : 'light';
}
