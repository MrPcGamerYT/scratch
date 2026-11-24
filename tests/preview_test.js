const { chromium } = require('playwright');
const assert = require('node:assert/strict');
(async () => {
  const browser = await chromium.launch({headless:true});
  try {
    for (const viewport of [{width:1440,height:900},{width:390,height:844}]) {
      const page = await browser.newPage({viewport});
      await page.goto('http://127.0.0.1:4173/');
      assert.equal((await page.request.get('http://127.0.0.1:4173/.git/config')).status(), 404);
      assert.match(await page.locator('.preview-banner').innerText(), /not connected/);
      await page.locator('#openSettings').click();
      assert(await page.locator('#settingsOverlay').isVisible());
      for (const name of ['display','graphics','device']) {
        await page.locator(`[data-page="${name}"]`).click();
        assert.match(await page.locator('#settingsPage').innerText(), /not available/i);
        if (name === 'display') assert(await page.getByText('Show FPS at bottom left').locator('input').isDisabled());
      }
      await page.locator('[data-page="preferences"]').click();
      assert(await page.getByText('Experimental multi-touch keymapping').isVisible());
      await page.locator('#settingsOverlay [data-save]').click();
      assert.match(await page.locator('#toast').innerText(), /Preview only/);
      await page.locator('#openKeys').click();
      assert.match(await page.locator('#keysOverlay').innerText(), /preview only/i);
      assert.match(await page.locator('#keysOverlay').innerText(), /WASD joystick.*not implemented/i);
      assert.equal(await page.locator('#keySelect option').count(), 37);
      await page.locator('#keySelect').selectOption({label:'B'});
      assert(await page.locator('#repeatDelay').isDisabled());
      await page.locator('#modeSelect').selectOption('repeat');
      assert(await page.locator('#repeatDelay').isEnabled());
      await page.locator('#repeatDelay').fill('400');
      await page.locator('#repeatDelay').dispatchEvent('change');
      let count = await page.locator('.demo-canvas .marker').count();
      await page.locator('#demoCanvas').click({position:{x:20,y:20}});
      assert.equal(await page.locator('.demo-canvas .marker').count(), count+1);
      assert.match(await page.locator('#bindingList').innerText(), /B ↻/);
      await page.locator('#removeMarker').click();
      assert.equal(await page.locator('.demo-canvas .marker').count(), count);
      await page.locator('#openAim').click();
      assert(await page.locator('#aimOverlay').isVisible());
      assert(await page.getByLabel('Camera area X').isVisible());
      assert(await page.getByLabel('Sensitivity').isVisible());
      await page.keyboard.press('Escape');
      assert(!(await page.locator('#aimOverlay').isVisible()));
      assert(await page.locator('#keysOverlay').isVisible());
      await page.keyboard.press('Escape');
      assert(!(await page.locator('#keysOverlay').isVisible()));
      await page.locator('#openAimDock').click();
      assert(await page.locator('#aimOverlay').isVisible());
      await page.locator('#aimOverlay [data-save]').click();
      assert.match(await page.locator('#toast').innerText(), /Preview only/);
      await page.close();
    }
    console.log('Interactive preview desktop and mobile checks passed');
  } finally { await browser.close(); }
})().catch(e => {console.error(e); process.exit(1)});
