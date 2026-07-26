import { expect, test, type Page } from "@playwright/test";

const scenario = "preview=workspace&scenario=duplicate-names";

test("browser first run offers a working workspace preview entry", async ({ page }) => {
  const issues = monitorPageIssues(page);
  await page.goto("/");
  await expectHealthyDashboard(page, "连接 SDK 服务");
  await expect(page.getByLabel("API Key")).toHaveCount(0);

  await page.getByRole("button", { name: "进入工作台预览" }).click();
  await expect(page).toHaveURL(/preview=workspace/);
  await expectHealthyDashboard(page, "总览");
  expect(issues).toEqual([]);
});

test("same-name environments remain independently searchable and selectable", async ({ page }) => {
  const issues = monitorPageIssues(page);
  await page.goto(`/?${scenario}&page=environments`);
  await expectHealthyDashboard(page, "环境");
  await expect(page.getByText("浏览器预览 · 只读", { exact: true })).toBeVisible();

  const first = page.locator('tr[data-env-id="env-demo-01"]');
  const second = page.locator('tr[data-env-id="env-demo-02"]');
  await expect(first).toHaveCount(1);
  await expect(second).toHaveCount(1);
  await expect(first).toContainText("共享工作环境");
  await expect(second).toContainText("共享工作环境");

  await page.getByRole("checkbox", { name: "选择 共享工作环境 (env-demo-02)" }).check();
  await expect(page.getByRole("toolbar", { name: "批量环境操作" })).toContainText("1 个已选择");

  await first.click();
  const detail = page.locator("aside.environment-detail");
  await expect(detail).toContainText("env-demo-01");

  await page.getByPlaceholder("搜索名称或 envId").fill("env-demo-02");
  await expect(first).toBeHidden();
  await expect(second).toBeVisible();
  await expect(page.getByRole("checkbox", { name: "选择 共享工作环境 (env-demo-02)" })).toBeChecked();
  expect(issues).toEqual([]);
});

test("fingerprint comparison keeps columns bound to envId", async ({ page }) => {
  const issues = monitorPageIssues(page);
  await page.goto(`/?${scenario}&page=fingerprints`);
  await expectHealthyDashboard(page, "指纹");
  const viewer = page.locator(".fingerprint-viewer");
  await expect(viewer).toContainText("Canvas");
  await expect(viewer).toContainText("真实");
  await expect(viewer).not.toContainText("其它");
  await expect(viewer).not.toContainText("{");

  await expect(page.getByRole("button", { name: "查看 共享工作环境 (env-demo-01)" })).toHaveCount(1);
  await expect(page.getByRole("button", { name: "查看 共享工作环境 (env-demo-02)" })).toHaveCount(1);
  await page.getByRole("button", { name: "对比" }).click();
  await page.getByRole("checkbox", { name: "对比 共享工作环境 (env-demo-02)" }).check();

  const firstColumn = page.locator('.fingerprint-comparison th[data-env-id="env-demo-01"]');
  const secondColumn = page.locator('.fingerprint-comparison th[data-env-id="env-demo-02"]');
  await expect(firstColumn).toContainText("共享工作环境");
  await expect(firstColumn).toContainText("env-demo-01");
  await expect(secondColumn).toContainText("共享工作环境");
  await expect(secondColumn).toContainText("env-demo-02");
  await expect(page.getByText("2/4", { exact: true })).toBeVisible();
  expect(issues).toEqual([]);
});

test("environment pickers expose envId instead of relying on names", async ({ page }) => {
  const issues = monitorPageIssues(page);
  await page.goto(`/?${scenario}&page=mcp`);
  await expectHealthyDashboard(page, "MCP");

  await page.getByRole("button", { name: "单环境" }).click();
  const runtimeEnvironment = page.getByLabel("运行环境");
  await expect(runtimeEnvironment.locator("option")).toHaveText([
    "共享工作环境 · env-demo-01",
    "共享工作环境 · env-demo-02",
  ]);
  await runtimeEnvironment.selectOption("env-demo-02");
  await expect(runtimeEnvironment).toHaveValue("env-demo-02");
  await page.getByLabel("工具", { exact: true }).selectOption("navigate");
  await expect(page.getByLabel("MCP JSON 参数")).toHaveValue("{}");
  await page.getByLabel("MCP JSON 参数").fill('{"url":"https://example.com"}');
  await expect(page.getByRole("button", { name: "运行工具" })).toBeDisabled();

  await page.goto(`/?${scenario}&page=proxies`);
  await expectHealthyDashboard(page, "代理");
  await page.locator('[data-resource-id="proxy-demo"]').click();
  const boundEnvironment = page.getByLabel("绑定环境");
  await expect(boundEnvironment.locator("option")).toHaveText([
    "不绑定",
    "共享工作环境 · env-demo-01",
    "共享工作环境 · env-demo-02",
  ]);
  expect(issues).toEqual([]);
});

test("AI context exposes the selected envId and local CDP with a settings entry", async ({ page }) => {
  const issues = monitorPageIssues(page);
  await page.goto(`/?${scenario}&page=ai`);
  await expectHealthyDashboard(page, "AI 助手");

  const environment = page.getByLabel("AI 关联环境", { exact: true });
  await expect(environment.locator("option")).toHaveText([
    "全部环境",
    "共享工作环境 · env-demo-01",
    "共享工作环境 · env-demo-02",
  ]);
  await expect(page.getByRole("region", { name: "AI 关联环境详情" })).toContainText("TCP CDP");
  await expect(page.getByRole("region", { name: "AI 关联环境详情" })).toContainText("ws://127.0.0.1/preview/env-demo-01");

  await page.getByRole("button", { name: "Agent" }).click();
  await expect(page.getByRole("button", { name: "每次批准" })).toHaveClass(/active/);
  await page.getByRole("button", { name: "自动执行" }).click();
  await expect(page.getByRole("button", { name: "自动执行" })).toHaveClass(/active/);

  await environment.selectOption("env-demo-02");
  const context = page.getByRole("region", { name: "AI 关联环境详情" });
  await expect(context).toContainText("env-demo-02");
  await expect(context).toContainText("TCP CDP");
  await expect(context).toContainText("ws://127.0.0.1/preview/env-demo-02");

  await page.getByRole("button", { name: "新建会话" }).click();
  await expect(page.locator(".ai-conversation-row")).toHaveCount(2);
  await environment.selectOption("");
  await expect(context).toContainText("全部环境");
  await page.reload();
  await expect(page.getByLabel("AI 关联环境", { exact: true })).toHaveValue("");
  await expect(page.locator(".ai-conversation-row")).toHaveCount(2);
  await expect(page.getByRole("button", { name: "自动执行" })).toHaveClass(/active/);

  await page.getByRole("button", { name: "AI Provider 设置" }).click();
  await expectHealthyDashboard(page, "设置");
  await expect(page.getByRole("heading", { name: "AI Provider" })).toBeVisible();
  await expect(page.getByLabel("OpenAI-compatible Base URL")).toHaveValue("https://api.deepseek.com");
  expect(issues).toEqual([]);
});

test("Agent approval mode and MCP runtime controls fit the viewport", async ({ page }, testInfo) => {
  const issues = monitorPageIssues(page);
  await page.goto(`/?${scenario}&page=ai`);
  await expectHealthyDashboard(page, "AI 助手");
  await page.getByRole("button", { name: "Agent" }).click();
  await page.getByRole("button", { name: "自动执行" }).click();
  await expect(page.getByRole("button", { name: "自动执行" })).toHaveClass(/active/);
  await page.screenshot({ path: testInfo.outputPath("ai-agent-automatic.png"), fullPage: true });

  await page.goto(`/?${scenario}&page=mcp`);
  await expectHealthyDashboard(page, "MCP");
  await page.getByRole("button", { name: "单环境" }).click();
  await page.getByLabel("工具", { exact: true }).selectOption("navigate");
  await page.getByLabel("MCP JSON 参数").fill('{"url":"https://example.com"}');
  await expect(page.getByText("18 个可用工具")).toBeVisible();
  await expectHealthyDashboard(page, "MCP");
  await page.screenshot({ path: testInfo.outputPath("mcp-environment-tool.png"), fullPage: true });
  expect(issues).toEqual([]);
});

test("operation center filters by envId and protects unsupported actions", async ({ page }) => {
  const issues = monitorPageIssues(page);
  await page.goto(`/?${scenario}&page=operations`);
  await expectHealthyDashboard(page, "操作");
  await expect(page.getByLabel("操作摘要")).toContainText("显示 4/4");

  await page.getByLabel("环境筛选").selectOption("env-demo-02");
  await expect(page.getByLabel("操作摘要")).toContainText("显示 2/4");
  await expect(page.locator('tr[data-operation-id="op-preview-refresh-02"]')).toBeVisible();
  await expect(page.locator('tr[data-operation-id="op-preview-stop-02"]')).toBeVisible();
  await expect(page.getByRole("button", { name: /重试.*刷新.*env-demo-02/ })).toHaveCount(0);
  await expect(page.getByRole("button", { name: /取消.*停止.*env-demo-02/ })).toBeDisabled();

  await page.locator('tr[data-operation-id="op-preview-refresh-02"]').click();
  await expect(page.locator("aside.operation-detail")).toContainText("共享工作环境 · env-demo-02");
  expect(issues).toEqual([]);
});

async function expectHealthyDashboard(page: Page, pageHeading: string) {
  await expect(page).toHaveTitle("BroSDK Dashboard");
  await expect(page.getByRole("heading", { level: 1, name: pageHeading })).toBeVisible();
  await expect(page.locator("vite-error-overlay")).toHaveCount(0);
  const dimensions = await page.evaluate(() => ({
    clientWidth: document.documentElement.clientWidth,
    scrollWidth: document.documentElement.scrollWidth,
  }));
  expect(dimensions.scrollWidth).toBe(dimensions.clientWidth);
}

function monitorPageIssues(page: Page) {
  const issues: string[] = [];
  page.on("console", (message) => {
    if (["error", "warning"].includes(message.type())) issues.push(message.text());
  });
  page.on("pageerror", (error) => issues.push(error.message));
  return issues;
}
