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
