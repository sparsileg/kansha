// One shot through the bridge, to prove the wiring (#63).
//   node tests/ui/smoke-bridge.mjs [theme]
import { start } from './kansha-run.mjs';
import { mkdirSync } from 'node:fs';

const run = await start({ theme: process.argv[2] ?? 'light' });
try {
    console.log('sample', run.sample);
    await run.open();
    mkdirSync('target/shots', { recursive: true });
    await run.page.screenshot({ path: 'target/shots/smoke.png' });
    console.log('wrote target/shots/smoke.png');
} finally {
    await run.stop();
}
