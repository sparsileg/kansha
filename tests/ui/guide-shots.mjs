// Captures the User's Guide screenshots (the {.shot id="..."} placeholders
// in guide/chapters/*.md) through the dev-only bridge (#63): the real app
// and engine, a sample-data book, a fixed "today", the Light theme.
// Writes guide/images/<id>.png, which `just guide` puts in the PDF.
//   just ui-guide-shots            all shots
//   just ui-guide-shots register calendar
import { start } from './kansha-run.mjs';
import { SHOTS, stage, closeDue } from './shots.mjs';
import { mkdirSync } from 'node:fs';

const OUT_DIR = process.env.SHOT_DIR ?? 'guide/images';
const ONLY = process.argv.slice(2);

const START_IDS = ['setup-screen', 'passphrase-screen'];
const ORDER = [...START_IDS, ...Object.keys(SHOTS)].filter(id => id !== 'due-dialog').concat('due-dialog');
const ids = ONLY.length ? ORDER.filter(id => ONLY.includes(id)) : ORDER;
const unknown = ONLY.filter(id => !ORDER.includes(id));
if (unknown.length) throw new Error(`no such shot: ${unknown.join(', ')}`);

mkdirSync(OUT_DIR, { recursive: true });
let failed = 0;
// The start screens need a book that isn't open: their own runs.
// Sized like the app's own start window.
const START_SHOTS = {
    'setup-screen': { book: false, width: 640, height: 640 },
    'passphrase-screen': { book: 'locked', width: 520, height: 640 },
};
for (const id of ids.filter(id => START_SHOTS[id])) {
    const shot = START_SHOTS[id];
    const r = await start({ width: shot.width, height: shot.height, theme: 'light', book: shot.book });
    try {
        await r.open();
        await r.page.evaluate(() => document.activeElement?.blur?.());
        const path = `${OUT_DIR}/${id}.png`;
        await r.page.screenshot({ path });
        console.log('wrote', path);
    } catch (e) {
        failed++;
        console.error(`${id}: ${e.message.split('\n')[0]}`);
    } finally {
        await r.stop();
    }
}

const run = await start({ width: 1280, height: 800, theme: 'light' });
try {
    await stage(run, run.today);
    for (const id of ids.filter(id => SHOTS[id])) {
        try {
            await run.open();
            await closeDue(run.page);
            const target = await SHOTS[id](run.page, run);
            await run.page.evaluate(() => document.activeElement?.blur?.());
            await run.page.waitForTimeout(400);
            const path = `${OUT_DIR}/${id}.png`;
            if (target) await target.screenshot({ path });
            else await run.page.screenshot({ path });
            console.log('wrote', path);
        } catch (e) {
            failed++;
            console.error(`${id}: ${e.message.split('\n')[0]}`);
            mkdirSync('target/shots', { recursive: true });
            await run.page.screenshot({ path: `target/shots/${id}.failed.png` }).catch(() => {});
        }
    }
} finally {
    await run.stop();
}
process.exit(failed ? 1 : 0);
