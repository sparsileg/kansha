// The screens the shot scripts photograph (#63): how to set up the book
// for them, and how to reach each one. Shared by guide-shots.mjs (the
// User's Guide) and walkthrough-shots.mjs (the astryx.tools walkthrough).
import { PASSPHRASE } from './kansha-run.mjs';

/** Opens `item` from the menu bar's `menu`. */
export async function menu(p, name, item) {
    await p.getByRole('menuitem', { name, exact: true }).or(p.getByRole('button', { name, exact: true })).first().click({ timeout: 5000 });
    await p.locator('[role=menu]').getByText(item, { exact: true }).first().click({ timeout: 5000 });
    await p.waitForTimeout(600);
}

/** Opens an account's register from the Accounts panel. */
export async function account(p, name) {
    await p.locator('aside, nav, [class*=account]').getByText(name, { exact: true }).first().click();
    await p.waitForTimeout(800);
}

/**
 * What the sample data leaves out but the shots need: schedules for the
 * Reminders, Calendar, and Due soon, and today's price for every
 * security (else Needs attention flags a stale one). All through the
 * real commands.
 */
export async function stage(run, today) {
    const { call } = run;
    const accounts = Object.fromEntries((await call('account_list')).map(a => [a.name, a.id]));
    const cats = await call('category_list');
    const byId = Object.fromEntries(cats.map(c => [c.id, c]));
    const path = c => (c.parent ? path(byId[c.parent]) + ':' : '') + c.name;
    const category = Object.fromEntries(cats.map(c => [path(c), c.id]));
    const need = (map, name) => {
        if (map[name] === undefined) throw new Error(`stage: no ${name}`);
        return map[name];
    };

    const schedule = (payee, account, direction, amount, target, recurrence, extra = {}) =>
        call('schedule_create', {
            payeeName: payee,
            fields: {
                account: need(accounts, account), payee: null, memo: '', direction,
                amount_type: 'fixed',
                lines: [{ target, amount: direction === 'payment' ? `-${amount}` : amount, memo: '', tag: null }],
                recurrence: { interval: 1, day1: null, day2: null, weekday: null, week_of_month: null,
                    weekend_rule: 'none', ...recurrence },
                end: { kind: 'never' }, remind_days: 7, mode: 'remind', average_of: null,
                ...extra,
            },
        });
    const cat = name => ({ kind: 'category', id: need(category, name) });
    const acct = name => ({ kind: 'account', id: need(accounts, name) });

    await schedule('City Power', 'Checking', 'payment', '62.40', cat('Utilities:Electric'),
        { frequency: 'monthly', day1: 27, start_date: '2026-10-27' }, { amount_type: 'estimated' });
    await schedule('Employer', 'Checking', 'deposit', '3106.50', cat('Income:Salary'),
        { frequency: 'twice_monthly', day1: 1, day2: 15, start_date: '2026-11-01' }, { remind_days: 10 });
    await schedule('Comcast', 'Checking', 'payment', '87.18', cat('Utilities:Internet'),
        { frequency: 'monthly', day1: 3, start_date: '2026-11-03' }, { mode: 'auto', remind_days: 3 });
    await schedule('Auto Loan Payment', 'Checking', 'payment', '412.36', acct('Auto Loan'),
        { frequency: 'monthly', day1: 28, start_date: '2026-11-28' }, { mode: 'auto', remind_days: 3 });
    await schedule('Water Dept', 'Checking', 'payment', '54.02', cat('Utilities:Water'),
        { frequency: 'monthly', day1: 12, start_date: '2026-11-12' }, { remind_days: 3 });

    // A paycheck split into gross pay, taxes, and part sent to savings,
    // for the split-panel shot. (A split line can't go to an investment
    // account such as a 401(k): that takes the investment register.)
    const fed = await call('category_create_path', { path: 'Tax:Fed', kind: 'expense' });
    const state = await call('category_create_path', { path: 'Tax:State', kind: 'expense' });
    const line = (target, amount) => ({ target, amount, memo: '', cleared: 'unmarked', tags: [] });
    await call('entry_create', { payeeName: 'Employer', entry: {
        account: need(accounts, 'Checking'), date: '2026-10-23', payee: null, check_num: '', memo: 'Paycheck',
        notes: '', amount: '3106.50', cleared: 'unmarked', tags: [],
        lines: [
            line(cat('Income:Salary'), '4800.00'),
            line({ kind: 'category', id: fed.id }, '-720.00'),
            line({ kind: 'category', id: state.id }, '-253.50'),
            line(acct('Savings'), '-720.00'),
        ],
    } });

    // The overdue rent, for the due-dialog shot only (else Needs
    // attention flags it in the other shots).
    run.overdueRent = () => schedule('Landlord', 'Checking', 'payment', '2016.50', cat('Housing:Rent'),
        { frequency: 'monthly', day1: 1, start_date: '2026-10-01' });

    // For the loan shot: an expense category for the interest. For the
    // budget shot: a budget made from last year's actuals.
    await call('category_create_path', { path: 'Auto:Loan Interest', kind: 'expense' });
    await call('budget_create', { name: 'Household' });

    // Today's price for each security: the last one known.
    for (const sec of await call('security_list')) {
        const prices = await call('price_list', { security: sec.id });
        const last = prices.reduce((a, b) => (a && a.date > b.date ? a : b), null);
        if (last && last.date < today) await call('price_set', { security: sec.id, date: today, price: last.price });
    }
}

/** Picks `option` in the select whose label starts with `label`. */
async function choose(p, label, option, within = p) {
    await within.locator('label').filter({ hasText: new RegExp(`^\\s*${label}`) })
        .locator('select').first().selectOption({ label: option });
    await p.waitForTimeout(500);
}

/** Answers the next file or folder picker with `path`. */
async function pick(p, cmd, path) {
    await p.evaluate(([c, f]) => { (window.__KANSHA_PICKS__ ??= {})[c] = f; }, [cmd, path]);
}

const dialog = p => p.locator('[role=dialog]').last();

/**
 * Dialogs that open by themselves at load (Due and overdue, Review
 * entries) would cover the shot: close them.
 */
export async function closeDue(p) {
    for (let i = 0; i < 3; i++) {
        const open = p.locator('[role=dialog]');
        if (!(await open.count())) return;
        const d = open.last();
        await d.getByRole('button', { name: /^(×|Later|Close)$/ }).first().click();
        await p.waitForTimeout(300);
    }
}

// Each shot: what to do before the picture, and what to photograph
// (a locator; none = the whole window).
export const SHOTS = {
    'main-window': async p => { await account(p, 'Checking'); },
    'insights': async p => {},
    'category-manager': async p => {
        await menu(p, 'Tools', 'Categories');
        await p.getByText('Charity:Tithe', { exact: true }).first().click();
    },
    'securities': async p => {
        await menu(p, 'Tools', 'Securities');
        await p.getByText('Vanguard Total Stock Market ETF', { exact: true }).first().click();
    },
    'settings': async p => { await menu(p, 'Edit', 'Settings…'); return dialog(p); },
    'investments-view': async p => { await menu(p, 'Tools', 'Investments'); },
    'reminders': async p => { await menu(p, 'Tools', 'Reminders'); },
    'calendar': async p => {
        await menu(p, 'Tools', 'Calendar');
        await p.locator('select').filter({ hasText: 'All accounts' }).first().selectOption({ label: 'Checking' });
        await p.getByLabel('Projected balance').check();
        await p.waitForTimeout(800);
    },
    'register': async p => { await account(p, 'Checking'); },
    'investment-register': async p => { await account(p, 'Brokerage'); },
    // Last: it adds the overdue rent, which every later shot would show.
    'due-dialog': async (p, run) => {
        await run.overdueRent();
        await run.open();
        // Other dialogs that open at load (Review entries) go first.
        const others = p.locator('[role=dialog]').filter({ hasNotText: 'Due and overdue' });
        while (await others.count()) {
            await others.last().getByRole('button', { name: /^(×|Later|Close)$/ }).first().click();
            await p.waitForTimeout(300);
        }
        const due = p.locator('[role=dialog]').filter({ hasText: 'Due and overdue' });
        if (!(await due.count())) {
            await menu(p, 'Tools', 'Reminders');
            await p.getByRole('button', { name: /^Due and overdue/ }).click();
            await p.waitForTimeout(500);
        }
        return due.last();
    },
    'reconcile-start': async p => {
        await menu(p, 'Tools', 'Reconcile');
        await choose(p, 'Account', 'Checking');
        await p.getByLabel('Statement date').first().fill('09/30/2026');
        await p.getByLabel('Ending balance').first().fill('3,954.24');
    },
    'account-dialog': async p => {
        await menu(p, 'Tools', 'Accounts');
        await p.getByRole('button', { name: /New account/ }).first().click();
        await p.waitForTimeout(500);
        const d = dialog(p);
        const field = name => d.locator('label').filter({ hasText: new RegExp(`^\\s*${name}`) }).locator('input').first();
        await field('Name').fill('Everyday Checking');
        await field('Opening date').fill('10/24/2026');
        await field('Institution').fill('Riverside Credit Union');
        await field('Interest rate').fill('0.10');
        await field('Name').focus();
        return d;
    },
    'settings-backups': async (p, run) => {
        await run.call('backup_now');
        await menu(p, 'Edit', 'Settings…');
        await dialog(p).locator('select').first().selectOption({ label: 'Backups' });
        await p.waitForTimeout(500);
        return dialog(p);
    },
    'schedule-dialog': async p => {
        await menu(p, 'Tools', 'Reminders');
        await p.getByRole('button', { name: 'New schedule' }).click();
        await p.waitForTimeout(500);
        const d = dialog(p);
        await choose(p, 'Account', 'Checking', d);
        await d.getByPlaceholder('Choose or type a new payee').fill('City Power');
        await d.getByLabel('Line 1 category').fill('Utilities:Electric');
        await d.getByLabel('Line 1 category').press('Tab');
        await d.locator('label').filter({ hasText: /^\s*Amount/ }).locator('select').first().selectOption('average');
        await d.locator('label').filter({ hasText: /^\s*Day of month/ }).locator('input').fill('9');
        await d.locator('label').filter({ hasText: /^\s*First due/ }).locator('input').fill('2026-11-09');
        await p.waitForTimeout(300);
        return d;
    },
    'report-window': async p => {
        await menu(p, 'Reports', 'Spending');
        await p.locator('[role=menu]').getByText('Income/Expense by Category', { exact: true }).click();
        await p.waitForTimeout(1000);
        await p.locator('label').filter({ hasText: /^\s*Date range/ }).locator('select').first()
            .selectOption({ label: 'Last year' });
        await p.waitForTimeout(1000);
    },
    'restore': async (p, run) => {
        await run.call('backup_now');
        const { readdirSync } = await import('node:fs');
        const zip = readdirSync(`${run.dir}/Backups`).filter(f => f.endsWith('.zip')).sort().pop();
        await pick(p, 'pick_backup_file', `${run.dir}/Backups/${zip}`);
        await menu(p, 'File', 'Restore…');
        await p.getByRole('button', { name: 'Choose backup file…' }).click();
        await p.waitForTimeout(1000);
        await dialog(p).locator('input[type=password], input').last().fill(PASSPHRASE);
        await p.getByRole('button', { name: 'Open backup' }).click();
        await p.waitForTimeout(2500);
        return dialog(p);
    },
    'review-entries': async (p, run) => {
        const { writeFileSync } = await import('node:fs');
        const file = `${run.dir}/phone-entries.txt`;
        writeFileSync(file, [
            'kansha-entries 1',
            'from "Phone"',
            '2026-10-23 "Local Coffee" 6.75 Food:Coffee acct Visa',
            '2026-10-23 Kroger 82.14 Food:Groceries acct Checking memo "milk, bread"',
            '2026-10-24 Shell 41.30 Auto:Fuel acct Visa',
            '',
        ].join('\n'));
        const report = await run.call('entries_import', { path: file });
        if (report.refused.length) throw new Error(JSON.stringify(report.refused));
        await menu(p, 'File', 'Review Entries…');
        await p.waitForTimeout(800);
        return dialog(p);
    },
    'reconcile-session': async p => {
        await menu(p, 'Tools', 'Reconcile');
        await choose(p, 'Account', 'Checking');
        await p.getByLabel('Statement date').first().fill('09/30/2026');
        // The September statement (`cargo run -p kansha-core --example
        // sample_statements 2026-10-24`), ticked through the 20th.
        await p.getByLabel('Ending balance').first().fill('3,954.24');
        await p.getByRole('button', { name: 'Start reconciling' }).click();
        await p.waitForTimeout(1200);
        const rows = p.locator('tr').filter({ has: p.locator('input[type=checkbox]') });
        for (let i = 0; i < await rows.count(); i++) {
            const row = rows.nth(i);
            const m = /09\/(\d\d)\/2026/.exec(await row.innerText());
            if (!m) continue;
            const box = row.locator('input[type=checkbox]').first();
            if (Number(m[1]) <= 20) await box.check(); else await box.uncheck();
        }
        await p.waitForTimeout(500);
    },
    'split-panel': async p => {
        await account(p, 'Checking');
        await p.getByText('--Split--', { exact: true }).first().click();
        await p.keyboard.press('Enter');
        await p.waitForTimeout(800);
    },
    'investment-entry': async p => {
        await account(p, 'Brokerage');
        await p.getByLabel('New transaction').click();
        await p.waitForTimeout(800);
        const d = dialog(p);
        await choose(p, 'Action', 'Sell', d);
        const field = name => d.locator('label').filter({ hasText: new RegExp(`^\\s*${name}`) }).locator('input').first();
        await d.locator('label').filter({ hasText: /^\s*Security/ }).locator('select').first()
            .selectOption({ label: 'VTI — Vanguard Total Stock Market ETF' });
        await p.waitForTimeout(300);
        await field('Shares').fill('20');
        await field('Price per share').fill('263.75');
        await d.locator('label').filter({ hasText: /^\s*Lots/ }).locator('select').first().selectOption('specific');
        await p.waitForTimeout(800);
        const picks = d.locator('input[aria-label^="Shares from lot"]');
        if (await picks.count()) await picks.first().fill('20');
        await p.waitForTimeout(500);
        return d;
    },
    'budget-window': async p => {
        await menu(p, 'Budget', 'Household');
        await p.waitForTimeout(1200);
    },
    'loan-details': async (p, run) => {
        // Terms that come close to the sample's 412.36 payment, and the
        // balance the sample leaves owing.
        const id = (await run.call('account_list')).find(a => a.name === 'Auto Loan').id;
        const owed = (await run.call('account_bar')).balances.find(b => b.account === id).current.replace('-', '');
        await account(p, 'Auto Loan');
        await p.getByRole('button', { name: 'Loan details…' }).first().click();
        await p.waitForTimeout(800);
        const d = dialog(p);
        const field = name => d.locator('label').filter({ hasText: new RegExp(`^\\s*${name}`) }).locator('input').first();
        await choose(p, 'Type', 'Auto', d);
        await field('Lender').fill('Riverside Credit Union');
        await field('Opening date').fill('10/01/2023');
        await field('Original balance').fill('18000');
        await field('Interest rate').fill('4.74');
        await d.getByLabel('Original length', { exact: true }).fill('48');
        await d.getByLabel('Length unit').selectOption('months');
        await field('Current balance').fill(owed);
        await field('Next payment due').fill('10/28/2026');
        await choose(p, 'Pay from', 'Checking', d);
        await choose(p, 'Interest category', 'Auto:Loan Interest', d);
        await field('Lender').focus();
        await p.waitForTimeout(1200);
        await d.getByRole('button', { name: /amortization table/ }).click();
        await p.waitForTimeout(500);
        return d;
    },
    'net-worth-report': async p => {
        await menu(p, 'Reports', 'Net Worth');
        await p.locator('[role=menu]').getByText('Net Worth', { exact: true }).last().click();
        await p.waitForTimeout(1500);
    },
    'import-preview': async p => {
        await pick(p, 'pick_import_file', `${process.cwd()}/crates/kansha-core/tests/fixtures/qif/whole.qif`);
        await menu(p, 'File', 'Import…');
        await p.getByRole('button', { name: 'Choose QIF file…' }).click();
        await p.waitForTimeout(1500);
        return dialog(p);
    },
};

