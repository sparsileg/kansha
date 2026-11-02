// Starts a throwaway Kansha with the dev-only bridge (#63) and the Vite
// dev server, and opens the UI in Playwright's Chromium with the bridge
// shim. Shared by the screenshot scripts in tests/ui/.
//
// Needs a debug build (`cargo build -p kansha`) and Playwright from
// PLAYWRIGHT_DIR (the Astryx install, ~/.local/share/astryx-playwright).
import { createRequire } from 'node:module';
import { spawn } from 'node:child_process';
import { mkdirSync, rmSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const require = createRequire(process.env.PLAYWRIGHT_DIR + '/');
const { chromium } = require('playwright');

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = join(HERE, '..', '..');
// Not 1420: `just dev` serves there.
const VITE_PORT = 1443;
const BRIDGE_PORT = 1444;
export const PASSPHRASE = 'screenshot-run';
// "Today" for every run: shots stay the same from run to run.
export const TODAY = '2026-10-24';

async function up(url) {
    try { await fetch(url); return true; } catch { return false; }
}

async function waitFor(check, what, tries = 150) {
    for (let i = 0; i < tries; i++) {
        const v = await check();
        if (v) return v;
        await new Promise(r => setTimeout(r, 200));
    }
    throw new Error(`${what} did not start`);
}

// Starts everything, makes a book of sample data, and sets the theme.
// The page is not loaded yet: call `run.open()`.
export async function start({ width = 1440, height = 900, theme = 'light', today = TODAY, seed = 1, book = true } = {}) {
    // book: true (sample data), false (none: setup), or 'locked'.
    if (await up(`http://localhost:${VITE_PORT}`)) throw new Error(`port ${VITE_PORT} is in use`);
    if (await up(`http://127.0.0.1:${BRIDGE_PORT}`)) throw new Error(`port ${BRIDGE_PORT} is in use`);

    // A fixed name: the backup folder shows in some shots. One run at a
    // time (the ports above make sure).
    const dir = join(tmpdir(), 'kansha-screenshots');
    rmSync(dir, { recursive: true, force: true });
    mkdirSync(join(dir, 'Backups'), { recursive: true });
    const children = [];

    const vite = spawn('npx', ['vite', '--port', String(VITE_PORT), '--strictPort'],
        { cwd: ROOT, stdio: 'ignore', detached: true });
    children.push(vite);

    let bridge = null;
    let app = null;
    // The app with the bridge on. Started again for a locked book.
    const launch = () => {
        bridge = null;
        app = spawn(join(ROOT, 'target', 'debug', 'kansha'), [], {
            cwd: ROOT,
            env: { ...process.env, KANSHA_DB: join(dir, 'Household.db'), KANSHA_BRIDGE: String(BRIDGE_PORT), KANSHA_TODAY: today },
            stdio: ['ignore', 'pipe', 'inherit'],
            detached: true,
        });
        children.push(app);
        app.stdout.on('data', chunk => {
            const m = /KANSHA_BRIDGE (\S+) token=(\S+)/.exec(chunk.toString());
            if (m) bridge = { url: m[1], token: m[2] };
        });
    };
    launch();

    const stop = async () => {
        // Whole process groups: `npx` leaves Vite behind otherwise.
        for (const c of children) {
            try { process.kill(-c.pid); } catch { c.kill(); }
        }
        rmSync(dir, { recursive: true, force: true });
    };

    try {
        await waitFor(() => up(`http://localhost:${VITE_PORT}`), 'vite');

        const call = async (cmd, args = {}) => {
            const r = await fetch(`${bridge.url}/invoke`, {
                method: 'POST',
                headers: { 'content-type': 'application/json', 'x-kansha-token': bridge.token },
                body: JSON.stringify({ cmd, args }),
            });
            const body = await r.json();
            if ('err' in body) throw new Error(`${cmd}: ${JSON.stringify(body.err)}`);
            return body.ok;
        };
        // The window must exist before commands can run through it.
        const ready = async () => {
            await waitFor(() => bridge, 'the Kansha bridge');
            await waitFor(async () => { try { await call('app_version'); return true; } catch { return false; } },
                'the Kansha window');
        };
        await ready();

        let sample = null;
        if (book) {
            await call('book_setup', { passphrase: PASSPHRASE, backupFolder: join(dir, 'Backups'), name: 'Household',
                folder: null, contents: { kind: 'empty' } });
            sample = await call('sample_data_load', { seed });
        }
        // `book: 'locked'`: start again on the book, as on a real start,
        // so the passphrase screen shows.
        if (book === 'locked') {
            await call('appearance_set', { appearance: { theme, font: 'system', font_size: 13 } });
            process.kill(-app.pid, 'SIGTERM');
            await new Promise(r => app.on('exit', r));
            launch();
            await ready();
        }
        await call('appearance_set', { appearance: { theme, font: 'system', font_size: 13 } });

        const browser = await chromium.launch();
        const page = await browser.newPage({ viewport: { width, height } });
        await page.addInitScript(b => { window.__KANSHA_BRIDGE__ = b; }, bridge);
        await page.addInitScript({ content: readFileSync(join(HERE, 'bridge-shim.js'), 'utf8') });
        page.on('pageerror', e => console.error('page error:', e.message));
        page.on('console', m => { if (m.type() === 'error' || m.type() === 'warning') console.error(`console ${m.type()}:`, m.text()); });

        return {
            dir, call, page, sample, today, url: `http://localhost:${VITE_PORT}`,
            open: async () => { await page.goto(`http://localhost:${VITE_PORT}`); await page.waitForTimeout(2500); },
            stop: async () => { await browser.close(); await stop(); },
        };
    } catch (e) {
        await stop();
        throw e;
    }
}
