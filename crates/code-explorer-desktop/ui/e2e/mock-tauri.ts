import type { Page } from "@playwright/test";
import { MOCK_RESPONSES } from "../src/lib/mock-data";

// Supply IPC fixtures in the test context; browser previews keep their real
// empty defaults, and production builds never enable a simulated repository.
export async function mockTauri(
  page: Page,
  options: { mode?: "explorer" | "chat" | null; responses?: Record<string, unknown> } = {},
) {
  const responses = Object.fromEntries(
    Object.entries(MOCK_RESPONSES).filter(([, value]) => typeof value !== "function"),
  );
  responses.chat_get_config = {
    provider: "", apiKey: "", baseUrl: "", model: "gpt-4", maxTokens: 4096,
  };
  Object.assign(responses, options.responses);
  await page.addInitScript(({ fixtures, mode }) => {
    // Manual repository-opening tests start in Explorer. Chat otherwise
    // auto-selects the first fixture repo while the welcome assertions run.
    // null leaves the profile fresh and exercises the actual startup mode.
    if (mode !== null) {
      localStorage.setItem("code-explorer-app-state", JSON.stringify({
        state: { mode, activeRepo: null }, version: 0,
      }));
    }
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      value: {
        invoke: async (command: string) => structuredClone(fixtures[command] ?? []),
        transformCallback: () => 0,
        unregisterCallback: () => {},
      },
    });
  }, { fixtures: responses, mode: options.mode ?? null });
}
