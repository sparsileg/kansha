# Getting Started

::: {.note title="At a glance"}
Install Kansha, then answer the three questions on its setup screen:
a name for the book, where its backups go, and the passphrase that
locks both. After that, Kansha asks for the passphrase each time it
starts.
:::

## Install Kansha

Kansha runs on Kubuntu and other Ubuntu-based Linux systems, and on
Windows. Both installers are on the **Releases** page of Kansha's
GitHub repository. Download the newest one for your system.

### Kubuntu and Ubuntu

Download the file ending in `_amd64.deb`, open a terminal in the
folder you saved it to, and install it:

```
sudo apt install ./Kansha_0.13.0_amd64.deb
```

Use the name of the file you downloaded. Kansha then appears in the
application menu. To install a newer version, do the
same with the newer file; your books and settings stay as they are.

### Windows

Download the file ending in `_x64-setup.exe` and run it.

The installer isn't signed, so Windows may show a blue box saying
**Windows protected your PC**. Click **More info**, then **Run
anyway**. The installer puts Kansha in the Start menu.

::: why
Signing an installer means buying a certificate every year, which a
free, single-user app doesn't need. The warning only says that
Windows doesn't know who made the file. Download Kansha only from its
own Releases page, and the file is the one that was built there.
:::

## Set Up Your First Book

A **book** is where Kansha keeps everything: your accounts,
transactions, categories, schedules, and settings. It is two files
side by side in one folder: `<name>.db`, the data, and `<name>.key`,
the key that unlocks it. Most people need one book.

The first time Kansha starts, it shows the **Set up** screen. It has
three steps, top to bottom.

::: {.shot id="setup-screen"}
The Set up screen on first start: 1. Book with Create a new book
chosen, the Name and Folder, Start with; 2. Backup folder;
3. Backup passphrase with its warning.
:::

### 1. Book

Leave **Create a new book** chosen. (If you already have a Kansha
backup, choose **Restore from a backup** instead; see [Backups and
Restore](backups.md). To use a book you already have, for example on
a USB drive, click **Open an existing book…**.)

**Name** is the book's name. It becomes the name of the book's files
and of its backups, so it can use only letters, digits, `-`, and
`_`, up to 40 of them: `household`, `kansha`, `smith-family`.

**Folder** is where the book's files go. Kansha suggests its own
data folder; click **Browse…** to choose another.

**Start with** decides what the new book holds:

- **Empty** has only the few categories Kansha itself needs. Choose
  it if you're going to import your Quicken data, which brings its
  own categories.
- **Standard categories** adds a ready-made list of household
  categories, such as Food:Groceries and Tax:Fed, with the
  tax-related ones already marked and matched to their lines on the
  tax forms.
  Choose it if you're starting fresh.

### 2. Backup folder

Kansha makes backups by itself, and they all go to this folder. It
starts as your Downloads folder.

Click **Browse…** and choose a folder that isn't on this computer's
disk: a USB drive, a network drive, or a folder that a cloud service
such as Dropbox or OneDrive keeps in sync.

::: why
A backup in Downloads is on the same disk as the book. If the disk
fails or the computer is lost, both go together. A backup somewhere
else is the only copy that survives.
:::

### 3. Backup passphrase

Type a passphrase twice. It unlocks the book, and every backup of
it.

::: {.note title="Write it down now"}
**If the passphrase is lost, the book and every backup are lost with
it.** Kansha can't recover it and can't open the book without it.
Before you click Create book, save it in a password manager or write
it down and keep it somewhere safe.
:::

A passphrase is easier to remember and harder to guess than a
password: four or five ordinary words with spaces between them, such
as `copper kettle on the porch`, is a good one. You'll type it each
time Kansha starts.

Click **Create book**. The window grows to its working size and the
new book opens.

## Your First Look

A new book has no accounts, so Kansha shows **No accounts yet** with
two buttons:

- **New account…** starts your own book. [Accounts](accounts.md)
  explains the account types and what to enter. Start with your main
  checking account, with its balance from your last statement.
- **Load sample data** fills the book with about three years of
  made-up accounts, transactions, schedules, and investments, ending
  a few weeks from today. Use it to try Kansha out.

::: tip
Try the sample data in a book of its own: make a book called
`sample` (**File > New…**), load the sample data there, and click
around as much as you like. When you're done, **File > Recent**
switches back to your real book.
:::

If you're coming from Quicken, don't add accounts by hand: import
them, with their history (see [Coming from
Quicken](coming-from-quicken.md)).

## Opening Kansha Each Day

From then on, Kansha starts with a small window showing the book's
name and asks for the **Backup passphrase**. Type it and click
**Open**. A wrong passphrase is simply asked for again.

::: {.shot id="passphrase-screen"}
The small passphrase window with the Kansha mark, the book's name and
path, the Backup passphrase box, and the links underneath.
:::

Kansha opens the book you used last. Under the passphrase box are
links for the other choices:

- **Open:** followed by the names of the other books used on this
  computer. Click one to open it instead.
- **Open another book…** picks a book's `.db` file from a folder.
- **Create a new book…** makes a new book beside the others.
- **Restore from a backup…** brings back a book from one of its
  backups.

When the book opens, Kansha shows the screen chosen in Settings,
Insights to start with (see [Settings and Themes](settings.md)).

When you're done, close the window or choose **File > Exit**. Kansha
backs up the book as it closes.

## Pick a Theme and Text Size

Three lists at the right end of the menu bar change how Kansha looks:

- **Theme**: Nordic (the default), Light, Dark, Classic (which looks
  like Quicken 2013), and Matrix (green on black).
- **Font**: System (your computer's own), Arial, Verdana, or Courier
  New.
- **Font size**: from 10 to 24 pixels; 13 is the default. Everything
  grows with it, the columns too.

Changes take effect at once. These three are kept for this computer,
not in the book, so the passphrase window uses them too.

## Where Kansha Keeps Things

| What | Kubuntu | Windows |
|:---|:---|:---|
| Books, unless you chose another folder | `~/.local/share/tools.astryx.kansha/` | `%APPDATA%\tools.astryx.kansha\` |
| This computer's settings: theme, font, window size, recent books | `~/.config/tools.astryx.kansha/config.json` | `%APPDATA%\tools.astryx.kansha\config.json` |
| Backups | The backup folder you chose | The backup folder you chose |

A book can live anywhere, a USB drive included. To move one, close
Kansha and move both its files, `.db` and `.key`, together; then
start Kansha and open it with **Open another book…**.

::: note
Every other setting, such as the date format or which screen opens
at startup, is kept inside the book. It moves with the book and
comes back when a backup is restored.
:::
