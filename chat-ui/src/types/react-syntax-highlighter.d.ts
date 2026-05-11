/**
 * Type shims for `react-syntax-highlighter` ESM sub-paths.
 *
 * The official `@types/react-syntax-highlighter` package only ships types for
 * the top-level entry. We import directly from the ESM tree to keep Vite's
 * tree-shaking happy (the lib bundles dozens of grammars otherwise), but
 * those sub-paths have no `.d.ts`. These two declarations re-export from
 * the typed root so TypeScript stays satisfied without losing tree-shaking
 * at runtime.
 */
declare module 'react-syntax-highlighter/dist/esm/prism' {
  export { Prism } from 'react-syntax-highlighter';
}

declare module 'react-syntax-highlighter/dist/esm/prism-light' {
  export { PrismLight as default } from 'react-syntax-highlighter';
}

declare module 'react-syntax-highlighter/dist/esm/styles/prism' {
  const styles: { [key: string]: { [key: string]: React.CSSProperties } };
  export const oneLight: { [key: string]: React.CSSProperties };
  export const vscDarkPlus: { [key: string]: React.CSSProperties };
  export default styles;
}

declare module 'react-syntax-highlighter/dist/esm/languages/prism/*' {
  const language: unknown;
  export default language;
}
