import { lazy, Suspense, useMemo } from 'react';
import ReactMarkdown, { type Components } from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { MermaidBlock } from './MermaidBlock';
import {
  looksLikeMermaid,
  normalizeBareMermaid,
  normalizeCodeFenceLanguage,
} from '../../utils/markdown';
import {
  linkifySourceReferences,
  parseSourceReferenceHref,
  type SourceReference,
} from '../../utils/source-references';

const SyntaxCodeBlock = lazy(() =>
  import('./SyntaxCodeBlock').then((m) => ({ default: m.SyntaxCodeBlock }))
);

interface Props {
  children: string;
  onOpenSourceReference?: (reference: SourceReference) => void;
}

export function Markdown({ children, onOpenSourceReference }: Props) {
  const markdown = useMemo(() => {
    const normalized = normalizeBareMermaid(children);
    return onOpenSourceReference ? linkifySourceReferences(normalized) : normalized;
  }, [children, onOpenSourceReference]);
  const markdownComponents = useMemo(
    () => createComponents(onOpenSourceReference),
    [onOpenSourceReference]
  );

  return (
    <div className="prose prose-invert prose-sm max-w-none prose-pre:bg-transparent prose-pre:border-0 prose-pre:p-0 prose-code:before:content-[''] prose-code:after:content-['']">
      <ReactMarkdown remarkPlugins={[remarkGfm]} components={markdownComponents}>
        {markdown}
      </ReactMarkdown>
    </div>
  );
}

const MERMAID_LANGUAGE_ALIASES = new Set([
  'mermaid',
  'mermaidjs',
  'mermaid-js',
  'mmd',
  'maid',
  'maimaid',
  'mermaide',
  'diagram',
  'flowchart',
  'sequence',
  'sequencediagram',
  'classdiagram',
]);

function createComponents(onOpenSourceReference?: (reference: SourceReference) => void): Components {
  return {
    ...baseComponents,
    a(props) {
      const { href, children } = props;
      const sourceReference = parseSourceReferenceHref(href);
      if (sourceReference && onOpenSourceReference) {
        return (
          <button
            type="button"
            onClick={() => onOpenSourceReference(sourceReference)}
            className="rounded border border-violet-500/30 bg-violet-500/10 px-1 py-0.5 font-mono text-[0.92em] text-violet-200 hover:border-violet-400/60 hover:bg-violet-500/20"
            title="Ouvrir dans l'explorateur GitNexus"
          >
            {children}
          </button>
        );
      }
      return (
        <a href={href} target={href?.startsWith('http') ? '_blank' : undefined} rel="noreferrer">
          {children}
        </a>
      );
    },
  };
}

function isMermaidLanguage(language: string | undefined): boolean {
  return !!language && MERMAID_LANGUAGE_ALIASES.has(language.toLowerCase());
}

const baseComponents: Components = {
  code(props) {
    const { className, children, ...rest } = props;
    const match = /language-([^\s]+)/.exec(className ?? '');
    const rawLanguage = match?.[1];
    const language = normalizeCodeFenceLanguage(rawLanguage);
    const raw = String(children).replace(/\n$/, '');

    // Explicit fence wins. Defensive fallback: if the block has no language
    // tag (LLM dropped it) but the content starts with a known Mermaid graph
    // type keyword, treat it as Mermaid anyway. Avoids the failure mode
    // where the model writes `flowchart TD` directly after `Diagramme :`
    // without a triple-backtick header — react-markdown then renders it as
    // a generic code block and we get plain text instead of an SVG.
    if (isMermaidLanguage(rawLanguage) || isMermaidLanguage(language) || looksLikeMermaid(raw)) {
      return <MermaidBlock text={raw} />;
    }

    if (!language) {
      return (
        <code className={className} {...rest}>
          {children}
        </code>
      );
    }

    return (
      <Suspense
        fallback={
          <pre className="code-block-plain overflow-x-auto rounded-md border p-3 text-xs">
            <code>{raw}</code>
          </pre>
        }
      >
        <SyntaxCodeBlock language={language} code={raw} />
      </Suspense>
    );
  },
};
