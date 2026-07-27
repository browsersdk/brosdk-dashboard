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

test("overview prioritizes runtime activity and keeps SDK self-check in diagnostics", async ({ page }) => {
  const issues = monitorPageIssues(page);
  await page.goto(`/?${scenario}`);
  await expectHealthyDashboard(page, "总览");
  await expect(page.getByRole("button", { name: "SDK Smoke" })).toHaveCount(0);
  await expect(page.getByRole("heading", { name: "运行活动" })).toBeVisible();
  await expect(page.getByText("2 个运行中", { exact: true })).toBeVisible();

  await page.getByRole("button", { name: "设置", exact: true }).click();
  await expectHealthyDashboard(page, "设置");
  const selfCheck = page.getByRole("button", { name: "运行 SDK 自检" });
  await expect(selfCheck).toBeVisible();
  await expect(selfCheck).toBeDisabled();
  await expect(page.getByText("需先停止全部环境并完成状态对账", { exact: true })).toBeVisible();
  await expect(page.getByPlaceholder("留空则自动选择")).toBeVisible();
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

test("environment start callback progress is visible without exposing payload JSON", async ({ page }) => {
  const issues = monitorPageIssues(page);
  await page.goto("/?preview=workspace&scenario=starting-progress&page=environments");
  await expectHealthyDashboard(page, "环境");

  const row = page.locator('tr[data-env-id="env-demo-01"]');
  await expect(row).toContainText("启动中");
  await expect(row).toContainText("37%");
  await expect(row).toContainText("browser-open · Downloading · 37%");
  await expect(row).not.toContainText('"data"');
  await expect(row.getByRole("progressbar", { name: "环境启动进度" })).toHaveAttribute("aria-valuenow", "37");
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
  await page.getByLabel("工具", { exact: true }).selectOption("env.navigate");
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

test("AI conversations keep an immutable global or environment MCP scope", async ({ page }) => {
  const issues = monitorPageIssues(page);
  await page.goto(`/?${scenario}&page=ai`);
  await expectHealthyDashboard(page, "AI 助手");

  const context = page.getByRole("region", { name: "AI 关联环境详情" });
  const scope = page.getByLabel("AI 会话作用域");
  await expect(scope).toContainText("全局");
  await expect(scope).toContainText("全部环境");
  await expect(page.getByLabel("AI 关联环境", { exact: true })).toHaveCount(0);

  await page.getByRole("button", { name: "新建会话" }).click();
  const dialog = page.getByRole("dialog", { name: "新建 AI 会话" });
  await expect(dialog).toContainText("作用域创建后不可修改");
  await dialog.getByRole("button", { name: "单环境" }).click();
  const environment = dialog.getByLabel("新会话关联环境");
  await expect(environment.locator("option")).toHaveText([
    "选择环境",
    "共享工作环境 · env-demo-01",
    "共享工作环境 · env-demo-02",
  ]);
  await environment.selectOption("env-demo-02");
  await dialog.getByRole("button", { name: "创建" }).click();
  await expect(scope).toContainText("单环境");
  await expect(scope).toContainText("env-demo-02");
  await expect(context).toContainText("TCP CDP");
  await expect(context).toContainText("ws://127.0.0.1/preview/env-demo-02");
  await expect(page.getByLabel("新会话关联环境")).toHaveCount(0);

  await page.getByRole("button", { name: "Agent" }).click();
  await expect(page.getByRole("button", { name: "每次批准" })).toHaveClass(/active/);
  await page.getByRole("button", { name: "自动执行" }).click();
  await expect(page.getByRole("button", { name: "自动执行" })).toHaveClass(/active/);

  await page.getByRole("button", { name: "新建会话" }).click();
  await page.getByRole("dialog", { name: "新建 AI 会话" }).getByRole("button", { name: "创建" }).click();
  await expect(page.locator(".ai-conversation-row")).toHaveCount(3);
  await expect(scope).toContainText("全局");
  await expect(scope).toContainText("全部环境");
  await page.reload();
  await expect(page.getByLabel("AI 会话作用域")).toContainText("全局");
  await expect(page.getByLabel("AI 会话作用域")).toContainText("全部环境");
  await expect(page.locator(".ai-conversation-row")).toHaveCount(3);
  await expect(page.getByRole("button", { name: "自动执行" })).toHaveClass(/active/);

  await page.getByRole("button", { name: "AI Provider 设置" }).click();
  await expectHealthyDashboard(page, "设置");
  await expect(page.getByRole("heading", { name: "AI Provider" })).toBeVisible();
  await expect(page.getByLabel("OpenAI-compatible Base URL")).toHaveValue("https://api.deepseek.com");
  expect(issues).toEqual([]);
});

test("AI message history opens at the latest reply", async ({ page }) => {
  const issues = monitorPageIssues(page);
  const createdAt = "2026-07-27T04:00:00.000Z";
  const messages = Array.from({ length: 48 }, (_, index) => ({
    id: `message-${index}`,
    role: index % 2 === 0 ? "user" : "assistant",
    mode: "chat",
    content: `${index % 2 === 0 ? "用户问题" : "AI 回复"} ${index + 1}：用于验证长会话会自动显示最新回复。`,
    createdAt,
  }));
  await page.addInitScript((storedMessages) => {
    localStorage.setItem("brosdk-dashboard.ai-conversations.v1", JSON.stringify({
      activeConversationId: "conversation-scroll-e2e",
      conversations: [{
        id: "conversation-scroll-e2e",
        title: "长会话滚动验证",
        mode: "chat",
        executionMode: "manual",
        contextEnvId: null,
        createdAt: "2026-07-27T04:00:00.000Z",
        updatedAt: "2026-07-27T04:01:00.000Z",
        messages: storedMessages,
      }],
    }));
  }, messages);

  await page.goto(`/?${scenario}&page=ai`);
  await expectHealthyDashboard(page, "AI 助手");
  const messageList = page.getByLabel("当前会话消息");
  await expect(messageList).toContainText("AI 回复 48");
  const dimensions = await messageList.evaluate((element) => ({
    clientHeight: element.clientHeight,
    scrollHeight: element.scrollHeight,
  }));
  expect(dimensions.scrollHeight).toBeGreaterThan(dimensions.clientHeight);
  await expect.poll(() => messageList.evaluate((element) => (
    element.scrollHeight - element.scrollTop - element.clientHeight
  ))).toBeLessThanOrEqual(2);
  expect(issues).toEqual([]);
});

test("Agent approval mode and MCP runtime controls fit the viewport", async ({ page }, testInfo) => {
  const issues = monitorPageIssues(page);
  await page.goto(`/?${scenario}&page=ai`);
  await expectHealthyDashboard(page, "AI 助手");
  await page.getByRole("button", { name: "Agent" }).click();
  await page.getByRole("button", { name: "自动执行" }).click();
  await expect(page.getByRole("button", { name: "自动执行" })).toHaveClass(/active/);
  await expect(page.getByLabel("AI 请求")).toBeVisible();
  await expect(page.getByLabel("当前会话消息")).toBeVisible();
  const viewport = page.viewportSize();
  const composerBox = await page.locator(".ai-composer").boundingBox();
  const messageBox = await page.getByLabel("当前会话消息").boundingBox();
  expect(viewport).not.toBeNull();
  expect(composerBox).not.toBeNull();
  expect(messageBox).not.toBeNull();
  expect(messageBox!.height).toBeGreaterThan(80);
  expect(composerBox!.y + composerBox!.height).toBeLessThanOrEqual(
    viewport!.height - (viewport!.width <= 700 ? 62 : 0) + 1,
  );
  await page.screenshot({ path: testInfo.outputPath("ai-agent-automatic.png"), fullPage: false });

  await page.goto(`/?${scenario}&page=mcp`);
  await expectHealthyDashboard(page, "MCP");
  await page.getByRole("button", { name: "单环境" }).click();
  await page.getByLabel("工具", { exact: true }).selectOption("env.navigate");
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
