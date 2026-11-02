// Captures the astryx.tools Kansha walkthrough screenshots through the
// dev-only bridge (#63): the same book and screens as the User's Guide
// shots (shots.mjs), in the Nordic theme and a larger window, whole
// window every time. Writes <out>/<NNN-Name>.png.
//   just ui-walkthrough-shots [out] [shot ids or slide names]
import { start } from './kansha-run.mjs';
import { SHOTS, stage, closeDue } from './shots.mjs';
import { mkdirSync } from 'node:fs';

const OUT_DIR = process.argv[2] ?? 'target/walkthrough';

// Slide file name → the shot in shots.mjs that sets up its screen.
export const SLIDES = [
    ['010-Money_at_a_glance', 'insights'],
    ['020-Locked_and_private', 'passphrase-screen'],
    ['030-Bring_your_Quicken_data', 'import-preview'],
    ['040-The_register', 'register'],
    ['050-Splits_that_add_up', 'split-panel'],
    ['060-Bills_it_remembers', 'reminders'],
    ['070-Whats_due', 'due-dialog'],
    ['080-The_month_ahead', 'calendar'],
    ['090-Loans_paid_down', 'loan-details'],
    ['100-Match_the_statement', 'reconcile-session'],
    ['110-Where_the_money_went', 'report-window'],
    ['120-A_budget_to_steer_by', 'budget-window'],
    ['130-Net_worth_over_time', 'net-worth-report'],
    ['140-Every_holding', 'investments-view'],
    ['150-Sell_the_right_shares', 'investment-entry'],
    ['160-Entries_from_your_phone', 'review-entries'],
    ['170-Backups_you_can_check', 'restore'],
];
// Slides shot in a run of their own: the book must not be open yet.
const START_SLIDES = { 'passphrase-screen': { book: 'locked' } };
const ONLY = process.argv.slice(3);
const wanted = ([name, id]) => !ONLY.length || ONLY.includes(id) || ONLY.includes(name);


mkdirSync(OUT_DIR, { recursive: true });
let failed = 0;
for (const [name, id] of SLIDES.filter(([, id]) => START_SLIDES[id]).filter(wanted)) {
    const r = await start({ width: 1440, height: 900, theme: 'nordic', ...START_SLIDES[id] });
    try {
        await r.open();
        await r.page.evaluate(() => document.activeElement?.blur?.());
        await r.page.screenshot({ path: `${OUT_DIR}/${name}.png` });
        console.log('wrote', `${OUT_DIR}/${name}.png`);
    } catch (e) {
        failed++;
        console.error(`${name}: ${e.message.split('\n')[0]}`);
    } finally {
        await r.stop();
    }
}
const run = await start({ width: 1440, height: 900, theme: 'nordic' });
try {
    await stage(run, run.today);
    // The due dialog adds an overdue rent the others shouldn't show: last.
    const main = SLIDES.filter(([, id]) => !START_SLIDES[id]).filter(wanted);
    const order = main.filter(([, id]) => id !== 'due-dialog').concat(main.filter(([, id]) => id === 'due-dialog'));
    for (const [name, id] of order) {
        try {
            await run.open();
            await closeDue(run.page);
            await SHOTS[id](run.page, run);
            await run.page.evaluate(() => document.activeElement?.blur?.());
            await run.page.waitForTimeout(400);
            await run.page.screenshot({ path: `${OUT_DIR}/${name}.png` });
            console.log('wrote', `${OUT_DIR}/${name}.png`);
        } catch (e) {
            failed++;
            console.error(`${name}: ${e.message.split('\n')[0]}`);
            await run.page.screenshot({ path: `${OUT_DIR}/${name}.failed.png` }).catch(() => {});
        }
    }
} finally {
    await run.stop();
}
process.exit(failed ? 1 : 0);
