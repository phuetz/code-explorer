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
  type SourceValidationState,
} from '../../utils/source-references';

const SyntaxCodeBlock = lazy(() =>
  import('./SyntaxCodeBlock').then((m) => ({ default: m.SyntaxCodeBlock }))
);

interface Props {
  children: string;
  onOpenSourceReference?: (reference: SourceReference) => void;
  validSourcePaths?: ReadonlySet<string> | null;
  sourceValidationState?: SourceValidationState;
}

export function Markdown({
  children,
  onOpenSourceReference,
  validSourcePaths = null,
  sourceValidationState,
}: Props) {
  const markdown = useMemo(() => {
    const normalized = normalizeBareMermaid(children);
    return onOpenSourceReference ? linkifySourceReferences(normalized) : normalized;
  }, [children, onOpenSourceReference]);
  const validationState = sourceValidationState ?? (validSourcePaths ? 'ready' : 'unavailable');
  const markdownComponents = useMemo(
    () => createComponents(onOpenSourceReference, validSourcePaths, validationState),
    [onOpenSourceReference, validSourcePaths, validationState]
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

function createComponents(
  onOpenSourceReference?: (reference: SourceReference) => void,
  validSourcePaths?: ReadonlySet<string> | null,
  sourceValidationState: SourceValidationState = 'unavailable'
): Components {
  return {
    ...baseComponents,
    a(props) {
      const { href, children } = props;
      const sourceReference = parseSourceReferenceHref(href);
      if (sourceReference && onOpenSourceReference) {
        if (sourceValidationState === 'pending') {
          return (
            <code
              className="rounded border border-sky-300/50 bg-sky-50 px-1 py-0.5 font-mono text-[0.92em] text-sky-800 dark:border-sky-500/30 dark:bg-sky-500/10 dark:text-sky-200"
              title="Validation du chemin source en cours dans l'index Code Explorer"
            >
              {children}
            </code>
          );
        }
        if (
          sourceValidationState === 'ready' &&
          validSourcePaths &&
          !validSourcePaths.has(normalizeSourcePath(sourceReference.path))
        ) {
          return (
            <code
              className="rounded border border-amber-300/50 bg-amber-50 px-1 py-0.5 font-mono text-[0.92em] text-amber-800 dark:border-amber-500/30 dark:bg-amber-500/10 dark:text-amber-200"
              title="Chemin cité par le modèle, mais absent de l'index Code Explorer du projet sélectionné"
            >
              {children}
            </code>
          );
        }
        return (
          <button
            type="button"
            onClick={() => onOpenSourceReference(sourceReference)}
            className="rounded border border-violet-500/30 bg-violet-500/10 px-1 py-0.5 font-mono text-[0.92em] text-violet-200 hover:border-violet-400/60 hover:bg-violet-500/20"
            title="Ouvrir dans l'explorateur Code Explorer"
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

function normalizeSourcePath(path: string): string {
  return path.replace(/\\/g, '/').toLowerCase();
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
