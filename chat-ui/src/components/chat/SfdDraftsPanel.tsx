import { useCallback, useState } from 'react';
import { FileText, FilePen, ShieldCheck, X, RefreshCw, AlertTriangle, CheckCircle2, AlertCircle } from 'lucide-react';
import { useSfd } from '../../hooks/use-sfd';
import { useChatStore } from '../../stores/chat-store';
import type { SfdValidationIssue, SfdValidationPageReport } from '../../api/mcp-client';

/**
 * Slide-in panel that surfaces the SFD doc-authoring workflow.
 *
 * Lists module pages and in-progress drafts under `.codeexplorer/docs/`, lets
 * the user run the pre-delivery linter against the whole tree or only the
 * `_drafts/` directory, and renders the structured report inline.
 *
 * Hidden behind the toggle button in `ChatPanel`'s header — no chat budget
 * eaten when the user just wants to talk.
 */
export function SfdDraftsPanel() {
  const open = useChatStore((s) => s.isSfdPanelOpen);
  const setSfdPanelOpen = useChatStore((s) => s.setSfdPanelOpen);
  const repo = useChatStore((s) => s.selectedRepo);
  const sfd = useSfd(open ? repo : null);
  const [scope, setScope] = useState<'all' | 'drafts'>('drafts');
  const close = useCallback(() => setSfdPanelOpen(false), [setSfdPanelOpen]);

  if (!open) return null;

  return (
    <aside
      className="absolute right-0 top-14 z-30 flex h-[calc(100%-3.5rem)] w-full max-w-md flex-col border-l border-neutral-800 bg-neutral-950/95 shadow-xl backdrop-blur"
      aria-label="Brouillons SFD"
    >
      <header className="flex items-center gap-2 border-b border-neutral-900 px-4 py-3">
        <FileText className="h-4 w-4 text-neutral-400" aria-hidden />
        <h2 className="text-sm font-medium text-neutral-200">Brouillons SFD</h2>
        <button
          type="button"
          onClick={() => sfd.refreshPages()}
          disabled={!repo || sfd.busy}
          className="ml-auto rounded-md p-1 text-neutral-400 hover:bg-neutral-900 hover:text-neutral-200 disabled:opacity-40"
          aria-label="Rafraîchir"
        >
          <RefreshCw className={`h-4 w-4 ${sfd.busy ? 'animate-spin' : ''}`} />
        </button>
        <button
          type="button"
          onClick={close}
          className="rounded-md p-1 text-neutral-400 hover:bg-neutral-900 hover:text-neutral-200"
          aria-label="Fermer le panneau"
        >
          <X className="h-4 w-4" />
        </button>
      </header>

      {!repo ? (
        <EmptyState
          icon={<AlertCircle className="h-5 w-5 text-amber-400" />}
          title="Aucun projet sélectionné"
          detail="Choisis un projet dans le sélecteur en haut à droite pour voir les brouillons SFD."
        />
      ) : sfd.missing ? (
        <EmptyState
          icon={<AlertCircle className="h-5 w-5 text-amber-400" />}
          title="Pas de documentation"
          detail={`Lance \`code-explorer generate docs\` dans ${sfd.docsDir || '.codeexplorer/docs/'} pour démarrer.`}
        />
      ) : (
        <div className="flex min-h-0 flex-1 flex-col overflow-y-auto">
          <Section
            icon={<FileText className="h-3.5 w-3.5" />}
            title={`Pages publiées (${sfd.pages.length})`}
          >
            {sfd.pages.length === 0 ? (
              <p className="px-2 py-1 text-xs text-neutral-500">Aucune page dans modules/.</p>
            ) : (
              <ul className="space-y-0.5">
                {sfd.pages.map((p) => (
                  <li key={p}>
                    <span className="block rounded px-2 py-1 text-xs text-neutral-300">{p}</span>
                  </li>
                ))}
              </ul>
            )}
          </Section>

          <Section
            icon={<FilePen className="h-3.5 w-3.5" />}
            title={`Brouillons (${sfd.drafts.length})`}
          >
            {sfd.drafts.length === 0 ? (
              <p className="px-2 py-1 text-xs text-neutral-500">
                Le LLM peut écrire ici via <code className="text-neutral-400">write_sfd_draft</code>.
              </p>
            ) : (
              <ul className="space-y-0.5">
                {sfd.drafts.map((d) => (
                  <li key={d}>
                    <span className="block rounded px-2 py-1 text-xs text-amber-300">{d}</span>
                  </li>
                ))}
              </ul>
            )}
          </Section>

          <Section icon={<ShieldCheck className="h-3.5 w-3.5" />} title="Validation">
            <div className="flex items-center gap-2 px-2">
              <select
                value={scope}
                onChange={(e) => setScope(e.target.value as 'all' | 'drafts')}
                className="rounded-md border border-neutral-800 bg-neutral-900 px-2 py-1 text-xs text-neutral-200"
                aria-label="Cible de la validation"
              >
                <option value="drafts">_drafts/</option>
                <option value="all">Toute la doc</option>
              </select>
              <button
                type="button"
                onClick={() => sfd.validate(scope === 'drafts' ? '_drafts' : '')}
                disabled={sfd.busy}
                className="rounded-md border border-neutral-800 bg-neutral-900 px-3 py-1 text-xs text-neutral-200 hover:bg-neutral-800 disabled:opacity-40"
              >
                Valider
              </button>
            </div>
            {sfd.report && sfd.reportStatus && (
              <ReportView status={sfd.reportStatus} report={sfd.report} />
            )}
          </Section>

          {sfd.error && (
            <div className="m-4 rounded-md border border-red-900 bg-red-950/40 px-3 py-2 text-xs text-red-300">
              {sfd.error}
            </div>
          )}
        </div>
      )}
    </aside>
  );
}

function Section({
  icon,
  title,
  children,
}: {
  icon: React.ReactNode;
  title: string;
  children: React.ReactNode;
}) {
  return (
    <section className="border-b border-neutral-900 px-2 py-3">
      <h3 className="mb-2 flex items-center gap-1.5 px-2 text-[11px] font-medium uppercase tracking-wide text-neutral-500">
        {icon}
        {title}
      </h3>
      {children}
    </section>
  );
}

function EmptyState({
  icon,
  title,
  detail,
}: {
  icon: React.ReactNode;
  title: string;
  detail: string;
}) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-2 px-6 text-center">
      {icon}
      <p className="text-sm font-medium text-neutral-200">{title}</p>
      <p className="text-xs text-neutral-500">{detail}</p>
    </div>
  );
}

function ReportView({
  status,
  report,
}: {
  status: 'green' | 'yellow' | 'red';
  report: import('../../api/mcp-client').SfdValidationReport;
}) {
  const tone =
    status === 'green'
      ? 'border-emerald-700 bg-emerald-950/30 text-emerald-200'
      : status === 'yellow'
        ? 'border-amber-700 bg-amber-950/30 text-amber-200'
        : 'border-red-700 bg-red-950/30 text-red-200';
  const Icon = status === 'green' ? CheckCircle2 : AlertTriangle;
  const label =
    status === 'green'
      ? 'GREEN — prêt à livrer'
      : status === 'yellow'
        ? 'YELLOW — livre mais avec du style à corriger'
        : 'RED — bloqué, à corriger';

  const sortedPages = [...report.pages].sort((a, b) => b.issues.length - a.issues.length);

  return (
    <div className="mx-2 mt-3 space-y-2">
      <div className={`flex items-center gap-2 rounded-md border px-3 py-2 text-xs ${tone}`}>
        <Icon className="h-4 w-4" aria-hidden />
        <span className="font-medium">{label}</span>
      </div>
      <p className="px-2 text-xs text-neutral-500">
        {report.total_pages} page(s), {report.pages_with_issues} avec issues — {report.red_count}{' '}
        RED, {report.yellow_count} YELLOW.
      </p>
      {sortedPages.slice(0, 5).map((page) => (
        <PageIssues key={page.path} page={page} />
      ))}
    </div>
  );
}

function PageIssues({ page }: { page: SfdValidationPageReport }) {
  if (page.issues.length === 0) return null;
  return (
    <details className="rounded-md border border-neutral-800 bg-neutral-900/40 px-2 py-1 text-xs text-neutral-300">
      <summary className="cursor-pointer font-medium text-neutral-200">
        {page.path}{' '}
        <span className="text-neutral-500">— {page.issues.length} issue(s)</span>
      </summary>
      <ul className="mt-1 space-y-0.5 pl-3">
        {page.issues.slice(0, 5).map((iss, idx) => (
          <li key={idx} className="flex gap-1.5">
            <SeverityTag severity={iss.severity} />
            <span className="text-neutral-400">
              {iss.line ? `L${iss.line}` : '-'} {iss.kind}: {iss.detail}
            </span>
          </li>
        ))}
        {page.issues.length > 5 && (
          <li className="text-neutral-500">… +{page.issues.length - 5} de plus</li>
        )}
      </ul>
    </details>
  );
}

function SeverityTag({ severity }: { severity: SfdValidationIssue['severity'] }) {
  const cls =
    severity === 'red'
      ? 'bg-red-900/40 text-red-300'
      : 'bg-amber-900/40 text-amber-300';
  return <span className={`rounded px-1 text-[10px] uppercase ${cls}`}>{severity}</span>;
}
