# Command line and file entry: hand test (#20)

Spec §14.3, CLI-010 … CLI-080. Plan: `cli-entry-plan.md`. Written
2026-10-12 for the book `test` at `/run/media/stan/Astro/Kansha/`.
Every command is complete: copy and paste as is. Tick each box; note
anything odd with its step number.

## 0. Setup

- Build: `npm run tauri dev` (also builds the CLI binary).

- Book `test`: `/run/media/stan/Astro/Kansha/test.db`. Names used
  below come from its names copy: `"Checking 083"`, `"Citi Double
  Cash 8440"`, `Food:Groceries`, `Car:BlueForester:Gas`,
  `House:Goods`.

- Account names hold spaces, so they are quoted. Known open item.

- Start with the app closed and the inbox empty:
  
  ```
  ls /run/media/stan/Astro/Kansha/test-inbox/
  ```

## A. Command line (app closed)

- [ ] **1. Help.** Usage text, exit 0; then the usage error, exit 2.
  
  ```
  ~/github/kansha/target/debug/kansha help; echo "exit $?"
  ~/github/kansha/target/debug/kansha run; echo "exit $?"
  ```

- [ ] **2. No book.** "No book at /no/such.db.", exit 1.
  
  ```
  ~/github/kansha/target/debug/kansha add --book /no/such.db today x 1 y acct z; echo "exit $?"
  ```

- [ ] **3. No names copy.** Move the copy aside, add, put it back.
  Answer: accepted (unverified) plus "names not checked: there is no
  names copy yet". `House:Stuff` is not in the book: it is accepted
  here and flagged at review (step 14).
  
  ```
  mv /run/media/stan/Astro/Kansha/test-names.txt /run/media/stan/Astro/Kansha/test-names.txt.bak
  ~/github/kansha/target/debug/kansha add --book /run/media/stan/Astro/Kansha/test.db today Target 12.34 House:Stuff acct "Checking 083"; echo "exit $?"
  mv /run/media/stan/Astro/Kansha/test-names.txt.bak /run/media/stan/Astro/Kansha/test-names.txt
  ls /run/media/stan/Astro/Kansha/test-inbox/
  ```

- [ ] **4. Names copy.** `kansha-names 1`, `book "test"`,
  `date-format MM/DD/YYYY`, open checking/savings/cash/credit card
  accounts, expense categories only, no payees, no income categories.
  
  ```
  less /run/media/stan/Astro/Kansha/test-names.txt
  ```

- [ ] **5. One-line add.** "accepted (unverified): 1 entry for
  test". Case ignored; `groceries` (unique last part) resolves.
  
  ```
  ~/github/kansha/target/debug/kansha add --book /run/media/stan/Astro/Kansha/test.db today Kroger 45.20 groceries acct "citi double cash 8440"; echo "exit $?"
  ```

- [ ] **6. Bad names.** One error per name with close matches,
  exit 1, nothing new in the inbox. `Gas` alone is ambiguous
  (two Car gases, Utilities:Gas).
  
  ```
  ~/github/kansha/target/debug/kansha add --book /run/media/stan/Astro/Kansha/test.db today Shell 30.00 Gas acct "Chekcing 083"; echo "exit $?"
  ls /run/media/stan/Astro/Kansha/test-inbox/
  ```

- [ ] **7. Dates.** All three accepted; the newest inbox files show
  `2026-10-11`, `2026-10-01`, `2026-10-02`.
  
  ```
  ~/github/kansha/target/debug/kansha add --book /run/media/stan/Astro/Kansha/test.db yesterday Shell 30.00 Car:BlueForester:Gas acct "Citi Double Cash 8440"
  ~/github/kansha/target/debug/kansha add --book /run/media/stan/Astro/Kansha/test.db 2026-10-01 Shell 31.00 Car:BlueForester:Gas acct "Citi Double Cash 8440"
  ~/github/kansha/target/debug/kansha add --book /run/media/stan/Astro/Kansha/test.db 10/02/2026 Shell 32.00 Car:BlueForester:Gas acct "Citi Double Cash 8440"
  tail -n +1 /run/media/stan/Astro/Kansha/test-inbox/*.txt
  ```

- [ ] **8. Bad amount.** Each refused with a reason, exit 1.
  
  ```
  ~/github/kansha/target/debug/kansha add --book /run/media/stan/Astro/Kansha/test.db today Shell 0 Car:BlueForester:Gas acct "Checking 083"; echo "exit $?"
  ~/github/kansha/target/debug/kansha add --book /run/media/stan/Astro/Kansha/test.db today Shell 1.234 Car:BlueForester:Gas acct "Checking 083"; echo "exit $?"
  ~/github/kansha/target/debug/kansha add --book /run/media/stan/Astro/Kansha/test.db today Shell abc Car:BlueForester:Gas acct "Checking 083"; echo "exit $?"
  ```

- [ ] **9. Interactive add.**
  
  ```
  ~/github/kansha/target/debug/kansha add --book /run/media/stan/Astro/Kansha/test.db
  ```
  
  - Date: Enter → today.
  - Payee `Costco`, amount `100`.
  - Account: type `Chek` → asked again with close matches; then
    `Che` + Tab completes.
  - Category `split`; `Line 1 amount (100.00 left):` →
    `60 Food:Groceries`; next prompt shows 40.00 left.
  - Line 2: `House:Goods` alone → "Enter the amount first", asked
    again. Then `40` alone → `Line 2 category (40.00):` asks for the
    category; `House:Goods`.
  - Memo `interactive` → accepted.
  - Run again, Ctrl+C at Payee → nothing saved, exit 1.
  - No history file: `ls -a ~ | grep -i hist` same as before.

- [ ] **10. File.** Accepted, 2 entries.
  
  ```
  cat > ~/t.txt <<'EOF'
  kansha-entries 1
  from "Stan's phone"
  2026-10-12 Costco 182.40 split acct "Checking 083" id 1b4e28ba-2fa1-4d2a-9c3e-0f6a8b7c5d21
    120.00 Food:Groceries
    62.40 House:Goods memo "paper goods"
  2026-10-12 Costco -24.99 House:Goods acct "Checking 083" memo "return" id 2c5f39cb-3ab2-4e3b-8d4f-1a7b9c8d6e32
  EOF
  ~/github/kansha/target/debug/kansha run --book /run/media/stan/Astro/Kansha/test.db ~/t.txt; echo "exit $?"
  ```

- [ ] **11. Stdin + sender.** Accepted; same ids, so skipped at
  review (step 15).
  
  ```
  cat ~/t.txt | ~/github/kansha/target/debug/kansha run --book /run/media/stan/Astro/Kansha/test.db --from Pat -; echo "exit $?"
  ```

- [ ] **12. Bad file.** Lines 3 and 6 reported with their line
  numbers (split does not add up; no `acct`), exit 1, nothing new in
  the inbox.
  
  ```
  cat > ~/bad.txt <<'EOF'
  kansha-entries 1
  from "Stan's phone"
  2026-10-12 Costco 100.00 split acct "Checking 083"
    50.00 Food:Groceries
    40.00 House:Goods
  2026-10-12 Kroger 10.00 Food:Groceries
  EOF
  ~/github/kansha/target/debug/kansha run --book /run/media/stan/Astro/Kansha/test.db ~/bad.txt; echo "exit $?"
  ls /run/media/stan/Astro/Kansha/test-inbox/
  ```

## B. In the app

- [ ] **13. Review opens by itself.** Open `test`: Review Entries
  opens. Each row: date, payee, amount, account, category or
  "Split (n)" with lines, memo, sender (`Stan's phone`, `command
  line`). Inbox now empty:
  
  ```
  ls /run/media/stan/Astro/Kansha/test-inbox/
  ```

- [ ] **14. Unresolved name.** Target 12.34 (step 3) shows
  ⚠ House:Stuff with text; Approve disabled. Edit → `House:Goods`
  → Approve enabled → Approve.

- [ ] **15. Duplicate.** The two Costco entries from steps 10/11
  show once each.

- [ ] **16. Approve plain.** Approve Kroger 45.20 → status bar note.
  Citi Double Cash 8440 balance owed up 45.20; Food:Groceries
  45.20. Approve Costco −24.99 → Checking 083 up 24.99,
  House:Goods down 24.99.

- [ ] **17. Approve and open (split).** Costco 182.40 → Approve and
  open → register opens with split lines showing. End the edit →
  list reopens (entries remain).

- [ ] **18. Undo.** Approve a Shell entry, then Ctrl+Z →
  transaction gone, entry back in the list. Approve it again → new
  transaction.

- [ ] **19. Reject + Later.** Reject a Shell entry (confirm) → gone,
  ledger unchanged. Later closes the list; File > Review Entries…
  (n) shows the count and reopens it.

- [ ] **20. While open.** With the app open, run the command below
  in a terminal and do **not** click the window → within a second the
  menu count and status bar note change; the list does **not** pop
  up.
  
  ```
  ~/github/kansha/target/debug/kansha add --book /run/media/stan/Astro/Kansha/test.db today Wendys 9.87 "Food:Dining Out" acct "Citi Double Cash 8440"
  ```

- [ ] **21. Import Entries….** First make the files, then
  File > Import Entries…: pick `~/t2.txt` → 1 entry in the list;
  pick `~/bad.txt` → line errors shown, nothing added. Pick
  `~/t.txt` → nothing new (ids seen; the rejected one too).
  
  ```
  cat > ~/t2.txt <<'EOF'
  kansha-entries 1
  2026-10-12 Lowes 55.00 House:Maintenance acct "Checking 083"
  EOF
  ```

- [ ] **22. Export Names for Phones….** Default name
  `test-names-for-phones.txt` in Downloads; same content as
  `test-names.txt`; `test-names.txt` time updated:
  
  ```
  ls -l /run/media/stan/Astro/Kansha/test-names.txt
  diff ~/Downloads/test-names-for-phones.txt /run/media/stan/Astro/Kansha/test-names.txt
  ```

- [ ] **23. History.** Right-click the approved Wendys row →
  History…: the top heading reads `<when> · create · ui`, then a
  Field / Before / After table of the new transaction. (Receive and
  reject audit rows on `pending_entry` have no screen; integration
  tests only.)

## After

- Remove test files: `rm ~/t.txt ~/t2.txt ~/bad.txt`
- Full run pending: `just perf`, then `just check`, then commit.
