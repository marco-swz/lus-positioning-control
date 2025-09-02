// @ts-check
import { test, expect } from '@playwright/test';

test.beforeEach(async ({ page }) => {
    await page.goto('http://localhost:8085/');
});

test('rendering control tab', async ({ page }) => {
    await page.waitForTimeout(500);
    // Expect a title "to contain" a substring.
    await expect(page.locator('#control_state')).toHaveValue('Stopped');
    await expect(page.locator('#inp-voltage1')).toHaveValue('-');
    await expect(page.locator('#inp-voltage2')).toHaveValue('-');
    await expect(page.getByRole('button', {name: 'Start'})).toBeVisible();
    await expect(page.getByRole('button', {name: 'Stop'})).toBeHidden();
    await expect(page.getByRole('button', {name: 'Show'})).toBeHidden();
    await expect(page.getByRole('button', {name: 'Activate'})).toBeHidden();
    await expect(page.locator('#ui-status')).toHaveValue('connected');
    await expect(page.locator('#inp-pos-cross')).toBeDisabled();
    await expect(page.locator('#inp-pos-coax')).toBeDisabled();
    await expect(page.locator('#inp-pos-min-coax')).toHaveValue('0');
    await expect(page.locator('#inp-pos-min-cross')).toHaveValue('0');
    await expect(page.locator('#inp-pos-max-coax')).toHaveValue(/99.99/);
    await expect(page.locator('#inp-pos-max-cross')).toHaveValue(/99.99/);
    await expect(page.locator('#inp-pos-target-coax')).toHaveValue('-');
    await expect(page.locator('#inp-pos-target-cross')).toHaveValue('-');
    await expect(page.locator('#inp-pos-actual-coax')).toHaveValue('-');
    await expect(page.locator('#inp-pos-actual-cross')).toHaveValue('-');

    await page.getByRole('button', {name: 'Start'}).click();

    await expect(page.locator('#control_state')).toHaveValue('Running');
    await expect(page.locator('#inp-voltage1')).toHaveValue('0');
    await expect(page.locator('#inp-voltage2')).toHaveValue('0');
    await expect(page.locator('#inp-pos-actual-coax')).toHaveValue('0');
    await expect(page.locator('#inp-pos-target-coax')).toHaveValue('0');
    await expect(page.locator('#inp-pos-actual-cross')).toHaveValue(/9.999/);
    await expect(page.locator('#inp-pos-target-cross')).toHaveValue(/9.999/);
    await expect(page.getByRole('button', {name: 'Start'})).toBeHidden();
    await expect(page.getByRole('button', {name: 'Stop'})).toBeVisible();

    await page.getByRole('combobox').selectOption('Manual');

    await expect(page.getByRole('button', {name: 'Activate'})).toBeVisible();

    await page.getByRole('button', {name: 'Activate'}).click();

    await expect(page.getByRole('button', {name: 'Activate'})).toBeHidden();
    await expect(page.getByRole('button', {name: 'Start'})).toBeVisible();
    await expect(page.getByRole('button', {name: 'Stop'})).toBeHidden();
    await expect(page.locator('#control_state')).toHaveValue('Stopped');

    await page.getByRole('button', {name: 'Start'}).click();

    await expect(page.locator('#control_state')).toHaveValue('Running');

    await page.locator('#inp-pos-target-coax').fill('90');
    await page.locator('#inp-pos-target-coax').blur();
    await page.locator('#inp-pos-target-cross').fill('90');
    await page.locator('#inp-pos-target-cross').blur();

    await page.waitForTimeout(1500);

    await expect(page.locator('#inp-pos-actual-coax')).toHaveValue(/89.99/);
    await expect(page.locator('#inp-pos-actual-cross')).toHaveValue(/89.99/);

    await page.locator('#inp-pos-target-coax').fill('150');
    await page.locator('#inp-pos-target-coax').blur();
    await page.locator('#inp-pos-target-cross').fill('150');
    await page.locator('#inp-pos-target-cross').blur();

    await page.waitForTimeout(500);

    await expect(page.locator('#inp-pos-actual-coax')).toHaveValue(/99.99/);
    await expect(page.locator('#inp-pos-actual-cross')).toHaveValue(/99.99/);

    await page.locator('#inp-pos-target-coax').fill('0');
    await page.locator('#inp-pos-target-coax').blur();
    await page.locator('#inp-pos-target-cross').fill('0');
    await page.locator('#inp-pos-target-cross').blur();

    await page.waitForTimeout(1500);

    await expect(page.locator('#inp-pos-actual-coax')).toHaveValue('0');
    await expect(page.locator('#inp-pos-actual-cross')).toHaveValue('0');

    await page.getByText('Configuration').click();

    await expect(page.getByRole('button', {name: 'Stop'})).toBeHidden();
    await expect(page.getByRole('button', {name: 'Save'})).toBeVisible();

    await page.locator('input[name="limit_max_coax"]').fill('50');
    await page.getByRole('button', {name: 'Save'}).click();

    await page.getByText('Control').click();

    await expect(page.getByRole('button', {name: 'Stop'})).toBeVisible();
    await expect(page.getByRole('button', {name: 'Save'})).toBeHidden();

    await page.getByRole('button', {name: 'Stop'}).click();
    await page.getByText('Configuration').click();
    await page.locator('input[name="limit_min_coax"]').fill('10');
    await page.locator('input[name="limit_min_cross"]').fill('15');
    await page.locator('input[name="limit_max_coax"]').fill('50');
    await page.locator('input[name="limit_max_cross"]').fill('60');
    await page.getByRole('button', {name: 'Save'}).click();

    await expect(page.locator('input[name="limit_min_coax"]')).toHaveValue(/9.99/);
    await expect(page.locator('input[name="limit_min_cross"]')).toHaveValue(/14.99/);
    await expect(page.locator('input[name="limit_max_coax"]')).toHaveValue(/49.99/);
    await expect(page.locator('input[name="limit_max_cross"]')).toHaveValue(/60.00/);

    await page.getByText('Control').click();

    await expect(page.locator('#inp-pos-min-coax')).toHaveValue(/9.999/);
    await expect(page.locator('#inp-pos-min-cross')).toHaveValue(/14.99/);
    await expect(page.locator('#inp-pos-max-coax')).toHaveValue(/49.99/);
    await expect(page.locator('#inp-pos-max-cross')).toHaveValue(/60.00/);

    await page.getByRole('button', {name: 'Start'}).click();

    await expect(page.locator('#control_state')).toHaveValue('Running');
});

