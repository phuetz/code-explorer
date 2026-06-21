import { createBundledHighlighter, type HighlighterGeneric } from "@shikijs/core";
import { createJavaScriptRegexEngine } from "@shikijs/engine-javascript";

export type UiLang =
  | "rust"
  | "typescript"
  | "javascript"
  | "python"
  | "java"
  | "c"
  | "cpp"
  | "csharp"
  | "go"
  | "php"
  | "ruby"
  | "kotlin"
  | "swift"
  | "toml"
  | "json"
  | "yaml"
  | "markdown"
  | "html"
  | "css"
  | "sql"
  | "bash";

export type UiTheme = "github-dark" | "github-dark-default" | "tokyo-night";
export type UiHighlighter = HighlighterGeneric<UiLang, UiTheme>;

export const UI_LANG_MAP: Record<string, UiLang> = {
  rust: "rust",
  rs: "rust",
  typescript: "typescript",
  ts: "typescript",
  tsx: "typescript",
  javascript: "javascript",
  js: "javascript",
  jsx: "javascript",
  python: "python",
  py: "python",
  java: "java",
  c: "c",
  cpp: "cpp",
  cxx: "cpp",
  cc: "cpp",
  csharp: "csharp",
  cs: "csharp",
  go: "go",
  php: "php",
  ruby: "ruby",
  rb: "ruby",
  kotlin: "kotlin",
  kt: "kotlin",
  swift: "swift",
  toml: "toml",
  json: "json",
  yaml: "yaml",
  yml: "yaml",
  markdown: "markdown",
  md: "markdown",
  html: "html",
  css: "css",
  sql: "sql",
  bash: "bash",
  sh: "bash",
  shell: "bash",
  shellsession: "bash",
  ps1: "bash",
  powershell: "bash",
};

const loadedUiLangs = new Set<UiLang>();

export function resolveUiLanguage(language: string | null | undefined): UiLang | null {
  if (!language) return null;
  return UI_LANG_MAP[language.toLowerCase()] ?? null;
}

export async function ensureUiLanguageLoaded(
  highlighter: UiHighlighter,
  lang: UiLang
): Promise<boolean> {
  if (loadedUiLangs.has(lang)) return true;
  try {
    await highlighter.loadLanguage(lang);
    loadedUiLangs.add(lang);
    return true;
  } catch {
    return false;
  }
}

const createUiHighlighter = createBundledHighlighter<UiLang, UiTheme>({
  engine: () => createJavaScriptRegexEngine(),
  langs: {
    rust: () => import("@shikijs/langs/rust"),
    typescript: () => import("@shikijs/langs/typescript"),
    javascript: () => import("@shikijs/langs/javascript"),
    python: () => import("@shikijs/langs/python"),
    java: () => import("@shikijs/langs/java"),
    c: () => import("@shikijs/langs/c"),
    cpp: () => import("@shikijs/langs/cpp"),
    csharp: () => import("@shikijs/langs/csharp"),
    go: () => import("@shikijs/langs/go"),
    php: () => import("@shikijs/langs/php"),
    ruby: () => import("@shikijs/langs/ruby"),
    kotlin: () => import("@shikijs/langs/kotlin"),
    swift: () => import("@shikijs/langs/swift"),
    toml: () => import("@shikijs/langs/toml"),
    json: () => import("@shikijs/langs/json"),
    yaml: () => import("@shikijs/langs/yaml"),
    markdown: () => import("@shikijs/langs/markdown"),
    html: () => import("@shikijs/langs/html"),
    css: () => import("@shikijs/langs/css"),
    sql: () => import("@shikijs/langs/sql"),
    bash: () => import("@shikijs/langs/bash"),
  },
  themes: {
    "github-dark": () => import("@shikijs/themes/github-dark"),
    "github-dark-default": () => import("@shikijs/themes/github-dark-default"),
    "tokyo-night": () => import("@shikijs/themes/tokyo-night"),
  },
});

let uiHighlighterPromise: Promise<UiHighlighter> | null = null;

export function getUiHighlighter() {
  if (!uiHighlighterPromise) {
    uiHighlighterPromise = createUiHighlighter({
      themes: ["github-dark-default", "tokyo-night"],
      langs: [],
    });
  }
  return uiHighlighterPromise;
}
