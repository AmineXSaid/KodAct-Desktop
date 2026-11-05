// Renders the emblem to design/emblem/emblem-1024.png, which
// scripts/make_ico.py reduces for the icon's large sizes. Uses the
// Playwright Chromium that comes with the dev environment.
//
//   node design/emblem/render.js
const { chromium } = require('playwright');
const fs = require('fs');
const path = require('path');
(async () => {
  const svg = fs.readFileSync(path.join(__dirname, 'emblem.svg'), 'utf8');
  const b = await chromium.launch();
  const p = await b.newPage({ viewport: { width: 1024, height: 1024 } });
  await p.setContent(`<body style="margin:0;background:transparent"><div style="width:1024px;height:1024px">${svg}</div></body>`);
  await p.screenshot({ path: path.join(__dirname, 'emblem-1024.png'), omitBackground: true });
  await b.close();
  console.log('wrote design/emblem/emblem-1024.png');
})();
