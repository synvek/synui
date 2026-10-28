import { test, expect } from "@playwright/test";

test("web_hello DomA11y +1 increments count", async ({ page }) => {
  await page.goto("/");
  await expect(page.locator("#synui-canvas")).toBeVisible({ timeout: 30_000 });
  await expect(page.getByTestId("synui-count")).toBeVisible({ timeout: 60_000 });
  await expect(page.getByTestId("synui-count")).toContainText(/Count:\s*0/);
  await page.getByTestId("synui-inc").click();
  await expect(page.getByTestId("synui-count")).toContainText(/Count:\s*1/, {
    timeout: 15_000,
  });
});

test("web_hello canvas receives pointer events", async ({ page }) => {
  await page.goto("/");
  const canvas = page.locator("#synui-canvas");
  await expect(canvas).toBeVisible({ timeout: 60_000 });
  await canvas.click({ position: { x: 400, y: 120 } });
  await expect(page.locator("#synui-a11y-host")).toBeVisible();
});
