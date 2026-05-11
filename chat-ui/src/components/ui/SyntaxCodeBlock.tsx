import { useState } from 'react';
import { Check, Copy } from 'lucide-react';
import { copyTextToClipboard } from '../../utils/clipboard';
import {
  codeThemeStyle,
  resolveCodeLanguage,
  SyntaxHighlighter,
  useDocumentCodeTheme,
} from './code-highlighting';

interface Props {
  language: string;
  code: string;
}

export function SyntaxCodeBlock({ language, code }: Props) {
  const resolvedLanguage = resolveCodeLanguage(language);
  const theme = useDocumentCodeTheme();
  const [copied, setCopied] = useState(false);
  const copyCode = async () => {
    const ok = await copyTextToClipboard(code);
    if (ok) {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    }
  };

  const body = !resolvedLanguage ? (
    <pre className="code-block-plain overflow-x-auto p-3 text-xs">
      <code>{code}</code>
    </pre>
  ) : (
    <SyntaxHighlighter
      language={resolvedLanguage}
      style={codeThemeStyle(theme)}
      PreTag="div"
      customStyle={{
        background: 'transparent',
        color: 'var(--code-text)',
        margin: 0,
        borderRadius: 0,
        border: 0,
        fontSize: '0.8125rem',
        lineHeight: 1.6,
        padding: '0.875rem 1rem',
      }}
      codeTagProps={{
        style: {
          fontFamily:
            'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", monospace',
        },
      }}
    >
      {code}
    </SyntaxHighlighter>
  );

  return (
    <div className="code-block my-3 overflow-hidden rounded-lg border">
      <div className="code-block-header flex items-center justify-between gap-3 border-b px-3 py-1.5 text-xs">
        <span className="truncate font-mono text-neutral-400">{resolvedLanguage ?? language}</span>
        <button
          type="button"
          onClick={() => void copyCode()}
          className="icon-button rounded p-1.5"
          aria-label="Copier le bloc de code"
          title={copied ? 'Copié !' : 'Copier le code'}
        >
          {copied ? (
            <Check className="h-3.5 w-3.5" aria-hidden="true" />
          ) : (
            <Copy className="h-3.5 w-3.5" aria-hidden="true" />
          )}
        </button>
      </div>
      <div className="overflow-x-auto">{body}</div>
    </div>
  );
}
