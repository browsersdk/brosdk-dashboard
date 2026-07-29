import { chromium } from "@playwright/test";
import { mkdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";

const baseUrl = process.argv[2];
if (!baseUrl) throw new Error("A Dashboard base URL is required");

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const outputDir = path.join(repoRoot, "docs", "assets");
await mkdir(outputDir, { recursive: true });
const browser = await chromium.launch({ channel: "chrome" });
const context = await browser.newContext({
  viewport: { width: 1440, height: 900 },
  colorScheme: "light",
  locale: "zh-CN",
  deviceScaleFactor: 1,
});
const page = await context.newPage();
const issues = [];
page.on("console", (message) => {
  if (["error", "warning"].includes(message.type())) issues.push(message.text());
});
page.on("pageerror", (error) => issues.push(error.message));

async function capture(name, route, heading, prepare = async () => {}) {
  await page.goto(`${baseUrl}${route}`, { waitUntil: "networkidle" });
  await page.getByRole("heading", { level: 1, name: heading }).waitFor();
  await prepare();
  await page.evaluate(() => document.activeElement instanceof HTMLElement && document.activeElement.blur());
  const dimensions = await page.evaluate(() => ({
    clientWidth: document.documentElement.clientWidth,
    scrollWidth: document.documentElement.scrollWidth,
  }));
  if (dimensions.clientWidth !== dimensions.scrollWidth) {
    throw new Error(`${name} has horizontal overflow: ${dimensions.scrollWidth}/${dimensions.clientWidth}`);
  }
  await page.screenshot({
    path: path.join(outputDir, name),
    fullPage: false,
    animations: "disabled",
  });
}

try {
  const scenario = "?preview=workspace&scenario=duplicate-names";
  await capture("dashboard-overview.png", scenario, "工作台");
  await capture("environment-workspace.png", `${scenario}&page=environments`, "环境", async () => {
    await page.locator('tr[data-env-id="env-demo-01"]').click();
    await page.locator("aside.environment-detail").waitFor();
  });
  await capture("ai-agent-workspace.png", `${scenario}&page=ai`, "AI 助手", async () => {
    const createdAt = "2026-07-27T06:32:00.000Z";
    await page.evaluate(({ storageKey, persistenceKey, createdAt }) => {
      localStorage.setItem(persistenceKey, "enabled");
      localStorage.setItem(storageKey, JSON.stringify({
        activeConversationId: "conversation-readme",
        conversations: [{
          id: "conversation-readme",
          title: "检查环境标签页与控制通道",
          mode: "agent",
          executionMode: "automatic",
          contextEnvId: "env-demo-02",
          createdAt,
          updatedAt: createdAt,
          messages: [
            {
              id: "message-readme-user",
              role: "user",
              mode: "agent",
              content: "列出环境 env-demo-02 的标签页，并确认当前控制通道。",
              createdAt,
            },
            {
              id: "message-readme-agent",
              role: "assistant",
              mode: "agent",
              content: "已选择所关联环境的 env.tabs 工具；envId 将由 Manager 强制注入。",
              createdAt,
              executionAttempted: true,
              plan: {
                summary: "读取所选环境的标签页列表",
                action: "mcp.call",
                envId: "env-demo-02",
                expectedState: "ready",
                idempotencyKey: "readme-preview",
                arguments: { tool: "env.tabs", arguments: { action: "list" } },
              },
              execution: {
                action: "mcp.call",
                operation: { id: "op-agent-tabs-02", status: "succeeded" },
                response: { tabs: 3 },
                statusSemantics: "工具调用已完成，结果已按环境边界脱敏。",
                replayed: false,
              },
            },
          ],
        }],
      }));
    }, {
      storageKey: "brosdk-dashboard.ai-conversations.v1",
      persistenceKey: "brosdk-dashboard.ai-conversations.persistence.v1",
      createdAt,
    });
    await page.reload({ waitUntil: "networkidle" });
    await page.getByText("Operation op-agent-tabs-02", { exact: true }).waitFor();
  });
  if (issues.length) throw new Error(`Dashboard emitted browser issues: ${issues.join(" | ")}`);
  process.stdout.write(JSON.stringify({ status: "passed", screenshots: 3 }));
} finally {
  await context.close();
  await browser.close();
}
