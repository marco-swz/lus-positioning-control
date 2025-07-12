// @ts-check
import { test, expect } from '@playwright/test';

test.beforeEach(async ({ page }) => {
    await page.goto('http://localhost:8085/');
});

test('rendering control tab', async ({ page }) => {
    await page.waitForTimeout(500);
    // Expect a title "to contain" a substring.
    await expect(page.locator('#control_state')).toHaveValue('Stopped');
    await expect(page.locator('#inp-voltage1')).toHaveValue('0');
    await expect(page.locator('#inp-voltage2')).toHaveValue('0');
    await expect(page.getByRole('button', {name: 'Start'})).toBeVisible();
    await expect(page.locator('#ui-status')).toHaveValue('connected');
    await expect(page.locator('#inp-pos-cross')).toBeDisabled();
    await expect(page.locator('#inp-pos-coax')).toBeDisabled();
    await expect(page.locator('#inp-pos-min-coax')).toHaveValue('0');
    await expect(page.locator('#inp-pos-min-cross')).toHaveValue('0');
    await expect(page.locator('#inp-pos-max-coax')).toHaveValue('100');
    await expect(page.locator('#inp-pos-max-coax')).toHaveValue('100');
});

