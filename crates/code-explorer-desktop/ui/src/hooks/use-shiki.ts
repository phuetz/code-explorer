/**
 * Lazy-loaded Shiki highlighter singleton.
 * Returns tokenized lines for a given code + language so we can render
 * them with our own line-number gutter and hover styles.
 */
import { useState, useEffect } from "react";
import type { ThemedToken } from "@shikijs/core";
import { ensureUiLanguageLoaded, getUiHighlighter, resolveUiLanguage } from "../lib/shiki-runtime";

export type TokenizedLine = ThemedToken[];

/**
 * Hook that returns tokenized lines for syntax highlighting.
 * Falls back to plain text if the language isn't supported or shiki hasn't loaded yet.
 */
export function useShikiTokens(
  code: string | undefined,
  language: string | undefined
): { tokens: TokenizedLine[] | null; ready: boolean } {
  const [tokens, setTokens] = useState<TokenizedLine[] | null>(null);
  const [ready, setReady] = useState(false);

  // Handle no-code or unknown-language cases via render-time state adjustment.
  // (Synchronous setState inside an effect is forbidden by react-hooks/set-state-in-effect.)
  const lang = resolveUiLanguage(language);
  const [prevKey, setPrevKey] = useState<string>(`${code ?? ""}|${lang ?? ""}`);
  const currentKey = `${code ?? ""}|${lang ?? ""}`;
  if (currentKey !== prevKey) {
    setPrevKey(currentKey);
    if (!code || !lang) {
      // No code, or language not supported by Shiki — fall back to plain text.
      setTokens(null);
      setReady(true);
    }
  }

  // Async highlighter load for actual code with a supported language.
  useEffect(() => {
    if (!code) return;
    if (!lang) return;

    let cancelled = false;

    getUiHighlighter()
      .then(async (hl) => {
        if (cancelled) return;
        const langOk = await ensureUiLanguageLoaded(hl, lang);
        if (cancelled) return;
        // codeToTokensBase returns an array of lines, each line is an array of tokens
        const result = hl.codeToTokensBase(code, {
          lang: langOk ? lang : ("text" as never),
          theme: "github-dark-default",
        });
        setTokens(result);
        setReady(true);
      })
      .catch((err) => {
        if (cancelled) return;
        console.warn("Shiki tokenization failed, falling back to plain text:", err);
        setTokens(null);
        setReady(true);
      });

    return () => {
      cancelled = true;
    };
  }, [code, language, lang]);

  return { tokens, ready };
}
