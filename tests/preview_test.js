const { chromium } = require('playwright');
const assert = require('node:assert/strict');

(async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    for (const viewport of [{ width: 1440, height: 900 }, { width: 390, height: 844 }]) {
      const page = await browser.newPage({ viewport });
      await page.goto('http://127.0.0.1:4173/');

      // Never expose repository internals
      assert.equal((await page.request.get('http://127.0.0.1:4173/.git/config')).status(), 404);
      assert.match(await page.locator('.preview-banner').innerText(), /not connected/);

      await page.locator("#keymapMenuButton").click();
      assert.equal(await page.locator('#audioOutputPreview').count(), 0);
      assert.equal(await page.locator('#cleanMemoryPreview').count(), 1);
      // Stage hints toggle works live
      assert(await page.locator('#stageOverlay .hint-pill').count() >= 1);
      await page.locator('#toggleHints').click();
      assert.equal(await page.locator('#stageOverlay .hint-pill').count(), 0);
      await page.locator('#toggleHints').click();
      assert(await page.locator('#stageOverlay .hint-pill').count() >= 1);

      assert.equal(await page.getByRole('button', {name:'Retry', exact:true}).count(),0);
      await page.keyboard.press('F11');
      assert(await page.locator('body').evaluate(el=>el.classList.contains('preview-fullscreen')));
      await page.keyboard.press('F11');
      // Settings pages preserve honest status
      await page.locator('#openSettings').click();
      assert(await page.locator('#settingsOverlay').isVisible());
      await page.locator('#autoResourcesPreview').check();
      assert(await page.locator('#settingsPage .field input').isDisabled());
      await page.locator('#autoResourcesPreview').uncheck();
      for (const name of ['graphics', 'device']) {
        await page.locator(`[data-page="${name}"]`).click();
        assert.match(await page.locator('#settingsPage').innerText(), /not available/i);
        if (name === 'graphics') await page.locator('#graphicsPreview').selectOption({index: 2});

      }
      await page.locator('[data-page="input"]').click();
      await page.locator('#mouseInputPreview').selectOption({index: 1});
      await page.locator('#aimBatchPreview').selectOption({index: 1});
      assert.match(await page.locator('#settingsPage').innerText(), /No game FPS/);
      await page.locator('[data-page="display"]').click();
      await page.locator('#guestResolution').selectOption({label: '1920x1080'});
      await page.locator('#guestDensity').selectOption({label: '240 DPI'});
      await page.locator('#applyGuestDisplay').click();
      assert.match(await page.locator('#displayPreviewStatus').innerText(), /No Android VM/);
      await page.locator('[data-page="preferences"]').click();
      assert(await page.getByText('Experimental multi-touch keymapping').isVisible());
      await page.locator('#settingsOverlay [data-save]').click();
      assert.match(await page.locator('#toast').innerText(), /Preview only/);

      // Keymap Studio: templates, extended keys, repeat/burst, D-pad/Swipe/Zoom/MOBA/Script, Undo/Redo
      await page.locator('#keymapMenuButton').click();
      await page.locator('#openKeys').click();
      assert.match(await page.locator('#keysOverlay').innerText(), /preview only/i);
      assert(await page.locator('#keySelect option').count() >= 50);

      // Load MOBA template and verify MOBA skills appear
      await page.locator('#presetSelect').selectOption('moba');
      assert.match(await page.locator('#bindingList').innerText(), /MOBA Skill Q/);
      assert.match(await page.locator('#conflictBar').innerText(), /Zero key conflicts/);

      // Place a Repeat tap marker and remove it, then test Undo
      await page.locator('[data-tool="tap"]').click();
      await page.locator('#keySelect').selectOption({ label: 'B' });
      assert(await page.locator('#repeatDelay').isDisabled());
      await page.locator('#modeSelect').selectOption('repeat');
      assert(await page.locator('#repeatDelay').isEnabled());
      await page.locator('#repeatDelay').fill('125');
      await page.locator('#repeatDelay').dispatchEvent('change');
      const countBefore = await page.locator('#canvasNodes .marker').count();
      await page.locator('#demoCanvas').click({ position: { x: 30, y: 30 } });
      assert.equal(await page.locator('#canvasNodes .marker').count(), countBefore + 1);
      assert(await page.locator('.marker.pending').isVisible());
      await page.keyboard.press('b');
      assert.equal(await page.locator('.marker.pending').count(), 0);
      assert.match(await page.locator('#bindingList').innerText(), /B ↻/);

      // Cancel leaves no serialized binding; the next mouse press binds at the pending spot.
      const committed = await page.locator('#bindingList button').count();
      await page.locator('#demoCanvas').click({position:{x:90,y:90}});
      await page.keyboard.press('Escape');
      assert.equal(await page.locator('.marker.pending').count(),0);
      assert.equal(await page.locator('#bindingList button').count(),committed);
      await page.locator('#demoCanvas').click({position:{x:90,y:90}});
      await page.locator('#demoCanvas').click({position:{x:90,y:90},button:'right'});
      assert.equal(await page.locator('.marker.pending').count(),0);
      assert.match(await page.locator('#bindingList').innerText(),/Right Click/);

      // Place a Macro Script and verify live script validation
      await page.locator('[data-tool="script"]').click();
      await page.locator('#keySelect').selectOption({ label: 'F1' });
      await page.locator('#demoCanvas').click({ position: { x: 60, y: 40 } });
      assert(await page.locator('#scriptInspector').isVisible());
      await page.locator('#scriptText').fill('badcmd 10 20');
      assert.match(await page.locator('#scriptStatus').innerText(), /unknown command/i);
      await page.locator('#scriptText').fill('tap 50 50 40\nwait 60\nswipe 20 80 20 30 120');
      assert.match(await page.locator('#scriptStatus').innerText(), /Valid macro \(3 steps\)/);

      // Remove selected & Undo
      const countWithScript = await page.locator('#canvasNodes .marker').count();
      await page.locator('#removeMarker').click();
      assert.equal(await page.locator('#canvasNodes .marker').count(), countWithScript - 1);
      await page.locator('#undoBtn').click();
      assert.equal(await page.locator('#canvasNodes .marker').count(), countWithScript);

      // Aim, Shoot, ADS & Free-Look dialog
      await page.locator('#openAim').click();
      assert(await page.locator('#aimOverlay').isVisible());
      assert(await page.getByLabel('Camera area X').isVisible());
      assert(await page.getByLabel('Sensitivity', { exact: true }).isVisible());
      assert(await page.getByLabel('Sensitivity Y').isVisible());
      await page.keyboard.press('Escape');
      assert(!(await page.locator('#aimOverlay').isVisible()));
      assert(await page.locator('#keysOverlay').isVisible());
      await page.keyboard.press('Escape');
      assert(!(await page.locator('#keysOverlay').isVisible()));

      assert.equal(await page.locator('#openAimDock').count(), 0);
      await page.locator('#keymapMenuButton').click();
      await page.locator('#openKeys').click();
      await page.locator('#openAim').click();
      assert(await page.locator('#aimOverlay').isVisible());
      await page.locator('#aimOverlay [data-save]').click();
      assert.match(await page.locator('#toast').innerText(), /Preview only/);

      await page.close();
    }
    console.log('Interactive Keymap Studio desktop and mobile preview checks passed');
  } finally {
    await browser.close();
  }
})().catch(e => { console.error(e); process.exit(1); });
