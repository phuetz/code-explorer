import type { Page } from "@playwright/test";
import { MOCK_RESPONSES } from "../src/lib/mock-data";

// Supply IPC fixtures in the test context; browser previews keep their real
// empty defaults, and production builds never enable a simulated repository.
export async function mockTauri(page: Page) {
  const responses = Object.fromEntries(
    Object.entries(MOCK_RESPONSES).filter(([, value]) => typeof value !== "function"),
  );
  responses.chat_get_config = {
    provider: "", apiKey: "", baseUrl: "", model: "gpt-4", maxTokens: 4096,
  };
  await page.addInitScript((fixtures) => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      value: {
        invoke: async (command: string) => structuredClone(fixtures[command] ?? []),
        transformCallback: () => 0,
        unregisterCallback: () => {},
      },
    });
  }, responses);
}
