/**
 * DocsContent — Markdown renderer with Mermaid diagram support.
 *
 * Renders documentation pages as rich HTML from Markdown source,
 * with automatic Mermaid diagram rendering for ```mermaid code blocks.
 */

import { useEffect, useRef, useMemo, useState, useId } from "react";
import { ChevronRight, BookOpen, Play } from "lucide-react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import type { Components } from "react-markdown";
import { getUiHighlighter } from "../../lib/shiki-runtime";
import { t as tRaw } from "../../lib/i18n";
import { useI18n } from "../../hooks/use-i18n";
import { PresentationMode } from "./PresentationMode";
// Shiki is loaded via the shared UI runtime inside HighlightedCode.

/** Strip inline event handlers and script elements from SVG to prevent XSS. */
function sanitizeSvg(svg: string): string {
  return svg
    .replace(/<script[\s\S]*?<\/script>/gi, "")
    .replace(/\son\w+\s*=\s*"[^"]*"/gi, "")
    .replace(/\son\w+\s*=\s*'[^']*'/gi, "");
}

/** Convert text to a URL-safe slug for use as heading IDs. */
function slugify(text: string): string {
  return text
    .toLowerCase()
    .replace(/[^a-z0-9\s-]/g, "")
    .replace(/\s+/g, "-")
    .replace(/-+/g, "-")
    .replace(/^-|-$/g, "");
}

/** Return the effective theme ("dark" or "light") by reading the data-theme attribute. */
function getEffectiveTheme(): "dark" | "light" {
  const attr = document.documentElement.getAttribute("data-theme");
  if (attr === "light") return "light";
  if (attr === "system") {
    return window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
  }
  return "dark";
}

let mermaidPromise: Promise<typeof import("mermaid")["default"]> | null = null;

async function loadMermaid() {
  if (!mermaidPromise) {
    mermaidPromise = import("mermaid").then((m) => m.default);
  }
  return mermaidPromise;
}

/** (Re-)initialize mermaid for the given colour scheme. */
async function initMermaid(colorScheme: "dark" | "light") {
  const isDark = colorScheme === "dark";
  const mermaid = await loadMermaid();
  mermaid.initialize({
    startOnLoad: false,
    securityLevel: "strict",
    theme: isDark ? "dark" : "default",
    themeVariables: isDark
      ? {
          darkMode: true,
          background: "#0e1118",
          primaryColor: "#1c2233",
          primaryTextColor: "#e2e8f0",
          primaryBorderColor: "#5b9cf6",
          secondaryColor: "#252b3b",
          secondaryTextColor: "#c1cad8",
          tertiaryColor: "#1c212d",
          lineColor: "#5b9cf6",
          textColor: "#c1cad8",
          mainBkg: "#1c2233",
          nodeBorder: "#5b9cf6",
          clusterBkg: "#151922",
          clusterBorder: "rgba(148, 163, 194, 0.15)",
          titleColor: "#e2e8f0",
          edgeLabelBackground: "#151922",
          nodeTextColor: "#e2e8f0",
        }
      : {
          darkMode: false,
          background: "#f0f2f7",
          primaryColor: "#dde1eb",
          primaryTextColor: "#1a1d26",
          primaryBorderColor: "#4a85e0",
          secondaryColor: "#e8ebf2",
          secondaryTextColor: "#2e3341",
          tertiaryColor: "#f0f2f7",
          lineColor: "#4a85e0",
          textColor: "#2e3341",
          mainBkg: "#dde1eb",
          nodeBorder: "#4a85e0",
          clusterBkg: "#e8ebf2",
          clusterBorder: "rgba(60, 70, 100, 0.12)",
          titleColor: "#1a1d26",
          edgeLabelBackground: "#f0f2f7",
          nodeTextColor: "#1a1d26",
        },
    flowchart: {
      htmlLabels: true,
      curve: "basis",
      padding: 12,
    },
    fontFamily: "'DM Sans', system-ui, sans-serif",
    fontSize: 13,
  });
}

// ─── Props ──────────────────────────────────────────────────────────

interface DocsContentProps {
  content: string;
  title: string;
  onNavigate?: (path: string) => void;
}

// ─── Mermaid Block ──────────────────────────────────────────────────

function MermaidDiagram({ chart }: { chart: string }) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [svg, setSvg] = useState<string>("");
  const [error, setError] = useState<string | null>(null);
  const reactId = useId();
  const mermaidId = `mermaid-${reactId.replace(/:/g, "")}`;

  useEffect(() => {
    let cancelled = false;

    async function render() {
      try {
        // Re-initialize mermaid with the current theme before each render
        const mermaid = await loadMermaid();
        await initMermaid(getEffectiveTheme());
        const { svg: rendered } = await mermaid.render(mermaidId, chart.trim());
        if (!cancelled) {
          setSvg(rendered);
          setError(null);
        }
      } catch (err) {
        if (!cancelled) {
          setError((err as Error).message || "Failed to render diagram");
          // Clean up any failed render elements
          const errEl = document.getElementById("d" + mermaidId);
          if (errEl) errEl.remove();
        }
      }
    }

    render();
    return () => { cancelled = true; };
  }, [chart, mermaidId]);

  if (error) {
    return (
      <div
        className="my-4 p-4 rounded-lg text-xs overflow-x-auto"
        style={{ background: "var(--rose-subtle)", color: "var(--rose)", border: "1px solid var(--rose)" }}
      >
        <p className="font-medium mb-1">{tRaw("docs.diagramError")}</p>
        <pre className="whitespace-pre-wrap">{error}</pre>
      </div>
    );
  }

  return (
    <div
      ref={containerRef}
      className="my-6 flex justify-center overflow-x-auto rounded-xl p-6"
      style={{
        background: "var(--bg-1)",
        border: "1px solid var(--surface-border)",
      }}
      dangerouslySetInnerHTML={{ __html: sanitizeSvg(svg) }}
    />
  );
}

// ─── Shiki Code Highlighting ────────────────────────────────────────

function HighlightedCode({ code, lang }: { code: string; lang: string }) {
  const ref = useRef<HTMLElement>(null);

  useEffect(() => {
    if (!lang || !ref.current) return;

    let cancelled = false;

    (async () => {
      let highlighter: Awaited<ReturnType<typeof getUiHighlighter>> | null = null;
      try {
        highlighter = await getUiHighlighter();
        if (cancelled) return;
        await highlighter.loadLanguage(lang as never);

        if (cancelled || !ref.current) return;

        const highlighted = highlighter.codeToHtml(code, {
          lang: lang as never,
          theme: "github-dark" as never,
        });
        const match = highlighted.match(/<code[^>]*>([\s\S]*)<\/code>/);
        if (match && ref.current) {
          // Shiki output is sanitized syntax-highlighted HTML; CSP blocks script execution
          ref.current.innerHTML = match[1];
        }
      } catch {
        if (!cancelled && ref.current) {
          ref.current.textContent = code;
        }
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [code, lang]);

  return <code ref={ref}>{code}</code>;
}

// ─── Markdown Components ────────────────────────────────────────────

function createMarkdownComponents(onNavigate?: (path: string) => void): Components {
  return {
    // Headings
    h1: ({ children }) => {
      const text = typeof children === "string" ? children : String(children ?? "");
      const id = slugify(text);
      return (
        <h1
          id={id}
          className="text-2xl font-bold mt-0 mb-4 pb-3"
          style={{
            fontFamily: "var(--font-display)",
            color: "var(--text-0)",
            borderBottom: "1px solid var(--surface-border)",
          }}
        >
          {children}
        </h1>
      );
    },
    h2: ({ children }) => {
      const text = typeof children === "string" ? children : String(children ?? "");
      const id = slugify(text);
      return (
        <h2
          id={id}
          className="text-lg font-semibold mt-8 mb-3"
          style={{ fontFamily: "var(--font-display)", color: "var(--text-0)" }}
        >
          {children}
        </h2>
      );
    },
    h3: ({ children }) => {
      const text = typeof children === "string" ? children : String(children ?? "");
      const id = slugify(text);
      return (
        <h3
          id={id}
          className="text-base font-semibold mt-6 mb-2"
          style={{ fontFamily: "var(--font-display)", color: "var(--text-0)" }}
        >
          {children}
        </h3>
      );
    },

    // Paragraphs
    p: ({ children }) => (
      <p className="mb-4 leading-relaxed" style={{ color: "var(--text-1)" }}>
        {children}
      </p>
    ),

    // Links — handle relative doc links
    a: ({ href, children }) => {
      const isInternal = href && !href.startsWith("http") && !href.startsWith("#");
      return (
        <a
          href={href}
          onClick={(e) => {
            if (isInternal && onNavigate) {
              e.preventDefault();
              onNavigate(href!);
            }
          }}
          className="transition-colors"
          style={{ color: "var(--accent)", textDecoration: "underline", textUnderlineOffset: "2px" }}
        >
          {children}
        </a>
      );
    },

    // Code blocks — special handling for mermaid and syntax highlighting
    pre: ({ children }) => {
      // Check if this is a code element with mermaid language
      const child = children as React.ReactElement<{ className?: string; children?: React.ReactNode }>;
      if (child?.props?.className === "language-mermaid") {
        const code = String(child.props.children ?? "").replace(/\n$/, "");
        return <MermaidDiagram chart={code} />;
      }

      // Extract language and code for syntax highlighting
      const className = child?.props?.className ?? "";
      const match = className.match(/language-(\w+)/);
      const lang = match ? match[1] : "";
      const code = String(child?.props?.children ?? "").replace(/\n$/, "");

      return (
        <pre
          className="my-4 p-4 rounded-lg overflow-x-auto text-[13px] leading-relaxed"
          style={{
            background: "var(--bg-1)",
            border: "1px solid var(--surface-border)",
            fontFamily: "var(--font-mono)",
          }}
        >
          {lang ? (
            <HighlightedCode code={code} lang={lang} />
          ) : (
            <code>{children}</code>
          )}
        </pre>
      );
    },

    // Inline code
    code: ({ className, children }) => {
      // If inside a pre (block), render as-is
      if (className) {
        return <code className={className}>{children}</code>;
      }
      return (
        <code
          className="px-1.5 py-0.5 rounded text-[12px]"
          style={{
            background: "var(--bg-3)",
            color: "var(--accent)",
            fontFamily: "var(--font-mono)",
          }}
        >
          {children}
        </code>
      );
    },

    // Tables
    table: ({ children }) => (
      <div className="my-4 overflow-x-auto rounded-lg" style={{ border: "1px solid var(--surface-border)" }}>
        <table className="w-full text-[13px]">{children}</table>
      </div>
    ),
    thead: ({ children }) => (
      <thead style={{ background: "var(--bg-2)" }}>{children}</thead>
    ),
    th: ({ children }) => (
      <th
        className="px-3 py-2 text-left font-medium text-xs"
        style={{ color: "var(--text-2)", borderBottom: "1px solid var(--surface-border)" }}
      >
        {children}
      </th>
    ),
    td: ({ children }) => (
      <td
        className="px-3 py-2"
        style={{ color: "var(--text-1)", borderBottom: "1px solid var(--surface-border)" }}
      >
        {children}
      </td>
    ),

    // Lists
    ul: ({ children }) => (
      <ul className="mb-4 pl-5 space-y-1" style={{ color: "var(--text-1)", listStyleType: "disc" }}>
        {children}
      </ul>
    ),
    ol: ({ children }) => (
      <ol className="mb-4 pl-5 space-y-1" style={{ color: "var(--text-1)", listStyleType: "decimal" }}>
        {children}
      </ol>
    ),
    li: ({ children }) => <li className="leading-relaxed">{children}</li>,

    // Blockquotes
    blockquote: ({ children }) => (
      <blockquote
        className="my-4 pl-4 py-1"
        style={{
          borderLeft: "3px solid var(--accent)",
          color: "var(--text-2)",
          background: "var(--accent-subtle)",
          borderRadius: "0 var(--radius-sm) var(--radius-sm) 0",
        }}
      >
        {children}
      </blockquote>
    ),

    // Horizontal rule
    hr: () => <hr className="my-8" style={{ border: "none", borderTop: "1px solid var(--surface-border)" }} />,

    // Strong / emphasis
    strong: ({ children }) => <strong style={{ color: "var(--text-0)", fontWeight: 600 }}>{children}</strong>,
    em: ({ children }) => <em style={{ color: "var(--text-1)" }}>{children}</em>,
  };
}

// ─── Breadcrumb ────────────────────────────────────────────────────

function DocsBreadcrumb({ title, onNavigate }: { title: string; onNavigate?: (path: string) => void }) {
  const segments = title.split(/[/\\]/).filter(Boolean);
  if (segments.length === 0) return null;

  return (
    <nav
      className="flex items-center gap-1 text-[12px]"
      aria-label="Breadcrumb"
    >
      <button
        onClick={() => onNavigate?.("index")}
        className="flex items-center gap-1.5 hover-surface rounded px-1.5 py-0.5"
        style={{ color: "var(--text-3)" }}
      >
        <BookOpen size={12} />
        <span>Docs</span>
      </button>
      {segments.map((segment, i) => {
        const isLast = i === segments.length - 1;
        const partialPath = segments.slice(0, i + 1).join("/");
        return (
          <span key={i} className="flex items-center gap-1">
            <ChevronRight size={10} style={{ color: "var(--text-4)" }} />
            <button
              onClick={() => {
                if (!isLast && onNavigate) onNavigate(partialPath);
              }}
              className="hover-surface rounded px-1.5 py-0.5"
              style={{
                color: isLast ? "var(--text-0)" : "var(--text-3)",
                fontWeight: isLast ? 500 : 400,
                cursor: isLast ? "default" : "pointer",
              }}
            >
              {segment.replace(/[-_]/g, " ").replace(/\b\w/g, (c) => c.toUpperCase())}
            </button>
          </span>
        );
      })}
    </nav>
  );
}

// ─── Main Component ─────────────────────────────────────────────────

export function DocsContent({ content, title, onNavigate }: DocsContentProps) {
  const { t } = useI18n();
  const contentRef = useRef<HTMLDivElement>(null);
  const [isPresentationMode, setIsPresentationMode] = useState(false);

  // Derive table of contents from markdown headings (pure computation, no useEffect needed)
  const toc = useMemo(() => {
    const headings: { id: string; text: string; level: number }[] = [];
    const lines = content.split("\n");
    for (const line of lines) {
      const match = line.match(/^(#{2,3})\s+(.+)$/);
      if (match) {
        const level = match[1].length;
        const text = match[2].trim();
        const id = slugify(text);
        headings.push({ id, text, level });
      }
    }
    return headings;
  }, [content]);

  // Use title for accessibility (document title or aria)
  useEffect(() => {
    if (title) document.title = `${title} — Code Explorer Docs`;
  }, [title]);

  // Scroll to top when content changes
  useEffect(() => {
    contentRef.current?.scrollTo(0, 0);
  }, [content]);

  const components = useMemo(() => createMarkdownComponents(onNavigate), [onNavigate]);

  return (
    <div className="flex h-full">
      {/* Main content */}
      <div ref={contentRef} className="flex-1 overflow-y-auto">
        <div className="max-w-3xl mx-auto px-8 py-8">
          <div className="flex items-center justify-between mb-6">
            <DocsBreadcrumb title={title} onNavigate={onNavigate} />
            <button
              onClick={() => setIsPresentationMode(true)}
              className="flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg text-[12px] font-medium transition-colors hover:bg-[var(--bg-3)]"
              style={{ color: "var(--text-1)" }}
              title="Present as slideshow"
            >
              <Play size={12} style={{ color: "var(--accent)" }} />
              Present
            </button>
          </div>
          <article className="docs-prose">
            <ReactMarkdown remarkPlugins={[remarkGfm]} components={components}>
              {content}
            </ReactMarkdown>
          </article>
        </div>
      </div>

      {isPresentationMode && (
        <PresentationMode 
          content={content} 
          title={title} 
          onExit={() => setIsPresentationMode(false)} 
        />
      )}

      {/* Table of contents (right sidebar) */}
      {toc.length > 2 && (
        <div
          className="flex-shrink-0 overflow-y-auto py-8 px-4 hidden xl:block"
          style={{ width: 200, borderLeft: "1px solid var(--surface-border)" }}
        >
          <p
            className="text-[11px] font-medium uppercase tracking-wider mb-3"
            style={{ color: "var(--text-3)" }}
          >
            {t("docs.onThisPage")}
          </p>
          <nav className="space-y-1" aria-label="Table of contents">
            {toc.map((heading) => (
              <a
                key={heading.id}
                href={`#${heading.id}`}
                className="block text-[12px] py-0.5 transition-colors truncate"
                style={{
                  color: "var(--text-3)",
                  paddingLeft: heading.level === 3 ? 12 : 0,
                }}
                onClick={(e) => {
                  e.preventDefault();
                  // Find heading by text content
                  const headings = contentRef.current?.querySelectorAll("h2, h3");
                  headings?.forEach((h) => {
                    if (h.textContent?.trim() === heading.text) {
                      h.scrollIntoView({ behavior: "smooth", block: "start" });
                    }
                  });
                }}
              >
                {heading.text}
              </a>
            ))}
          </nav>
        </div>
      )}
    </div>
  );
}
