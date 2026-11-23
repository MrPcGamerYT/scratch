const { chromium } = require('playwright');
const assert = require('node:assert/strict');
(async () => {
  const browser = await chromium.launch({headless:true});
  try {
    for (const viewport of [{width:1440,height:900},{width:390,height:844}]) {
      const page = await browser.newPage({viewport});
      await page.goto('http://127.0.0.1:4173/');
      assert.match(await page.locator('.preview-banner').innerText(), /not connected/);
      await page.locator('#openSettings').click();
      assert(await page.locator('#settingsOverlay').isVisible());
      for (const name of ['display','graphics','device']) {
        await page.locator(`[data-page="${name}"]`).click();
        assert.match(await page.locator('#settingsPage').innerText(), /not available/i);
      }
      await page.locator('#settingsOverlay [data-save]').click();
      assert.match(await page.locator('#toast').innerText(), /Preview only/);
      await page.locator('#openKeys').click();
      let count = await page.locator('.demo-canvas .marker').count();
      await page.locator('#demoCanvas').click({position:{x:70,y:70}});
      assert.equal(await page.locator('.demo-canvas .marker').count(), count+1);
      await page.keyboard.press('Escape');
      assert(!(await page.locator('#keysOverlay').isVisible()));
      await page.close();
    }
    console.log('Interactive preview desktop and mobile checks passed');
  } finally { await browser.close(); }
})().catch(e => {console.error(e); process.exit(1)});
