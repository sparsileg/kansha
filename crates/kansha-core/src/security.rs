//! Keys (SECU-010 … SECU-040, SECU-090, BAK-060).
//!
//! One backup passphrase unlocks everything. Setting it makes an `age`
//! X25519 key pair: the public key encrypts backups and the database key
//! without needing the passphrase; the private key is stored only locked
//! with the passphrase (`age` scrypt). The database has its own random
//! 256-bit SQLCipher key, kept in the key file encrypted to the public
//! key. The passphrase itself is stored nowhere.
//!
//! The key file sits next to the database. It holds the public key, the
//! locked private key, and the encrypted database key, as JSON. A missing
//! or damaged key file cannot be repaired (SECU-090).

use std::fmt;
use std::io::{Read, Write};
use std::path::Path;
use std::str::FromStr;

use age::secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Key file format version.
const KEY_FILE_FORMAT: u32 = 1;

/// The backup passphrase, as typed. Not `Debug`-printable.
pub struct Passphrase {
    secret: SecretString,
    /// scrypt cost (log2 N) when locking; `None` lets `age` pick one that
    /// takes about a second on this computer.
    work_factor: Option<u8>,
}

impl Passphrase {
    pub fn new(text: impl Into<String>) -> Passphrase {
        Passphrase {
            secret: SecretString::from(text.into()),
            work_factor: None,
        }
    }

    /// A cheap scrypt cost, so tests do not spend a second per key.
    #[doc(hidden)]
    pub fn for_tests(text: impl Into<String>) -> Passphrase {
        Passphrase {
            secret: SecretString::from(text.into()),
            work_factor: Some(4),
        }
    }

    fn is_blank(&self) -> bool {
        self.secret.expose_secret().trim().is_empty()
    }

    /// Lock `plaintext` with this passphrase (armored `age` scrypt).
    fn lock(&self, plaintext: &[u8]) -> Result<String> {
        let mut r = age::scrypt::Recipient::new(self.secret.clone());
        if let Some(n) = self.work_factor {
            r.set_work_factor(n);
        }
        encrypt_armored(&r, plaintext)
    }

    /// Unlock text made by [`Passphrase::lock`].
    fn unlock(&self, armored: &str) -> Result<Vec<u8>> {
        let id = age::scrypt::Identity::new(self.secret.clone());
        decrypt_armored(&id, armored.as_bytes()).map_err(|e| match e {
            CryptError::NoMatch => Error::WrongPassphrase,
            CryptError::Other(m) => Error::Invalid(m),
        })
    }
}

impl fmt::Debug for Passphrase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Passphrase(..)")
    }
}

/// The SQLCipher key of one database: 32 random bytes. Not
/// `Debug`-printable.
#[derive(Clone, PartialEq, Eq)]
pub struct DbKey([u8; 32]);

impl DbKey {
    /// A new random key (SECU-010).
    pub fn generate() -> Result<DbKey> {
        let mut bytes = [0u8; 32];
        getrandom::getrandom(&mut bytes)
            .map_err(|e| Error::Io(format!("no random numbers: {e}")))?;
        Ok(DbKey(bytes))
    }

    /// Lowercase hex, 64 characters.
    pub fn hex(&self) -> String {
        use fmt::Write as _;
        let mut s = String::with_capacity(64);
        for b in self.0 {
            let _ = write!(s, "{b:02x}");
        }
        s
    }

    /// The value for `PRAGMA key` / `ATTACH … KEY`: SQLCipher's raw-key
    /// form, so no key derivation runs.
    pub fn sqlcipher_value(&self) -> String {
        format!("x'{}'", self.hex())
    }

    /// As shown to the user (SECU-020): DB Browser for SQLite takes it as
    /// a "Raw key".
    pub fn display_value(&self) -> String {
        format!("0x{}", self.hex())
    }

    fn from_hex(s: &str) -> Result<DbKey> {
        let s = s.trim();
        let bad = || Error::Invalid("the key file's database key is damaged".into());
        if s.len() != 64 || !s.is_ascii() {
            return Err(bad());
        }
        let mut bytes = [0u8; 32];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = u8::from_str_radix(&s[2 * i..2 * i + 2], 16).map_err(|_| bad())?;
        }
        Ok(DbKey(bytes))
    }
}

impl fmt::Debug for DbKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DbKey(..)")
    }
}

/// The public key: encrypts backups and database keys (BAK-060).
#[derive(Clone)]
pub struct PublicKey(age::x25519::Recipient);

impl PublicKey {
    /// Encrypt to this key (binary `age`).
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        age::encrypt(&self.0, plaintext).map_err(|e| Error::Io(format!("encryption failed: {e}")))
    }

    fn encrypt_armored(&self, plaintext: &[u8]) -> Result<String> {
        encrypt_armored(&self.0, plaintext)
    }
}

impl fmt::Display for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Debug for PublicKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PublicKey({})", self.0)
    }
}

/// The unlocked private key. Held only while needed; not
/// `Debug`-printable.
pub struct PrivateKey(age::x25519::Identity);

impl PrivateKey {
    pub(crate) fn generate() -> PrivateKey {
        PrivateKey(age::x25519::Identity::generate())
    }

    pub fn public(&self) -> PublicKey {
        PublicKey(self.0.to_public())
    }

    /// Decrypt binary or armored `age` data encrypted to this key.
    pub fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>> {
        decrypt_armored(&self.0, ciphertext).map_err(|e| match e {
            CryptError::NoMatch => {
                Error::Invalid("this data was not encrypted with this key".into())
            }
            CryptError::Other(m) => Error::Invalid(m),
        })
    }

    /// Lock with the passphrase: the only form the private key is ever
    /// stored in.
    pub fn lock(&self, passphrase: &Passphrase) -> Result<LockedPrivateKey> {
        if passphrase.is_blank() {
            return Err(Error::Invalid("the passphrase is empty".into()));
        }
        let text = self.0.to_string();
        Ok(LockedPrivateKey(
            passphrase.lock(text.expose_secret().as_bytes())?,
        ))
    }
}

impl fmt::Debug for PrivateKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PrivateKey(..)")
    }
}

/// The private key locked with the backup passphrase (armored `age`
/// scrypt). Safe to store; every backup carries a copy (BAK-035).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LockedPrivateKey(String);

impl LockedPrivateKey {
    pub fn from_armored(text: String) -> LockedPrivateKey {
        LockedPrivateKey(text)
    }

    pub fn armored(&self) -> &str {
        &self.0
    }

    /// Unlock; [`Error::WrongPassphrase`] when the passphrase is wrong.
    pub fn unlock(&self, passphrase: &Passphrase) -> Result<PrivateKey> {
        let bytes = passphrase.unlock(&self.0)?;
        let text = String::from_utf8(bytes)
            .map_err(|_| Error::Invalid("the private key is damaged".into()))?;
        age::x25519::Identity::from_str(text.trim())
            .map(PrivateKey)
            .map_err(|_| Error::Invalid("the private key is damaged".into()))
    }
}

/// The key file next to the database (SECU-010).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyFile {
    format: u32,
    public_key: String,
    private_key: LockedPrivateKey,
    /// The database key (hex) encrypted to `public_key`, armored.
    database_key: String,
}

/// What unlocking the key file gives.
#[derive(Debug)]
pub struct Unlocked {
    pub private_key: PrivateKey,
    pub db_key: DbKey,
}

impl KeyFile {
    /// Set a new backup passphrase for a new database: a new key pair and
    /// a new random database key (SECU-080).
    pub fn create(passphrase: &Passphrase) -> Result<(KeyFile, DbKey)> {
        let key = DbKey::generate()?;
        let file = Self::for_key(&PrivateKey::generate(), passphrase, &key)?;
        Ok((file, key))
    }

    /// A key file holding `db_key` under `private_key` (a restore keeps
    /// the backup's key pair and gives the database a new key, BAK-070).
    pub fn for_key(
        private_key: &PrivateKey,
        passphrase: &Passphrase,
        db_key: &DbKey,
    ) -> Result<KeyFile> {
        let public = private_key.public();
        Ok(KeyFile {
            format: KEY_FILE_FORMAT,
            public_key: public.to_string(),
            private_key: private_key.lock(passphrase)?,
            database_key: public.encrypt_armored(db_key.hex().as_bytes())?,
        })
    }

    /// A key file for a restored database (BAK-070): the backup's key
    /// pair, still locked with its passphrase, and the new database key.
    pub fn for_restore(
        private_key: &PrivateKey,
        locked: &LockedPrivateKey,
        db_key: &DbKey,
    ) -> Result<KeyFile> {
        let public = private_key.public();
        Ok(KeyFile {
            format: KEY_FILE_FORMAT,
            public_key: public.to_string(),
            private_key: locked.clone(),
            database_key: public.encrypt_armored(db_key.hex().as_bytes())?,
        })
    }

    /// The public key, for backups (no passphrase needed).
    pub fn public_key(&self) -> Result<PublicKey> {
        age::x25519::Recipient::from_str(&self.public_key)
            .map(PublicKey)
            .map_err(|_| Error::Invalid("the key file's public key is damaged".into()))
    }

    pub fn locked_private_key(&self) -> &LockedPrivateKey {
        &self.private_key
    }

    /// Unlock with the backup passphrase (SECU-020).
    pub fn unlock(&self, passphrase: &Passphrase) -> Result<Unlocked> {
        let private_key = self.private_key.unlock(passphrase)?;
        let hex = private_key.decrypt(self.database_key.as_bytes())?;
        let hex = String::from_utf8(hex)
            .map_err(|_| Error::Invalid("the key file's database key is damaged".into()))?;
        Ok(Unlocked {
            db_key: DbKey::from_hex(&hex)?,
            private_key,
        })
    }

    /// Change the backup passphrase (SECU-040): a new key pair; the
    /// database key stays. Old backups keep their old passphrase.
    pub fn change_passphrase(&self, old: &Passphrase, new: &Passphrase) -> Result<KeyFile> {
        let db_key = self.unlock(old)?.db_key;
        Self::for_key(&PrivateKey::generate(), new, &db_key)
    }

    /// Read a key file. A missing or unreadable one is an error; the
    /// database is then recovered from a backup (SECU-090).
    pub fn read(path: &Path) -> Result<KeyFile> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| Error::Io(format!("cannot read the key file {}: {e}", path.display())))?;
        let file: KeyFile = serde_json::from_str(&text)
            .map_err(|_| Error::Invalid(format!("the key file {} is damaged", path.display())))?;
        if file.format != KEY_FILE_FORMAT {
            return Err(Error::Invalid(format!(
                "the key file {} has unknown format {}",
                path.display(),
                file.format
            )));
        }
        file.public_key()?;
        Ok(file)
    }

    /// Write the key file: to a temporary file beside it, then renamed
    /// over it, so a crash never leaves half a key file.
    pub fn write(&self, path: &Path) -> Result<()> {
        let text = serde_json::to_string_pretty(self)?;
        write_atomic(path, text.as_bytes())
    }
}

/// Write `bytes` to `path` through a temporary file in the same folder.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp);
    let result = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        std::fs::rename(&tmp, path)
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(Error::Io(format!("cannot write {}: {e}", path.display())));
    }
    Ok(())
}

enum CryptError {
    /// The key or passphrase does not match.
    NoMatch,
    Other(String),
}

fn encrypt_armored(r: &dyn age::Recipient, plaintext: &[u8]) -> Result<String> {
    use age::armor::{ArmoredWriter, Format};
    let fail = |e: &dyn fmt::Display| Error::Io(format!("encryption failed: {e}"));
    let enc = age::Encryptor::with_recipients(std::iter::once(r)).map_err(|e| fail(&e))?;
    let mut out = Vec::new();
    let armor = ArmoredWriter::wrap_output(&mut out, Format::AsciiArmor).map_err(|e| fail(&e))?;
    let mut w = enc.wrap_output(armor).map_err(|e| fail(&e))?;
    w.write_all(plaintext).map_err(|e| fail(&e))?;
    w.finish()
        .and_then(|armor| armor.finish())
        .map_err(|e| fail(&e))?;
    String::from_utf8(out).map_err(|e| fail(&e))
}

/// Decrypt binary or armored `age` data.
fn decrypt_armored(
    id: &dyn age::Identity,
    ciphertext: &[u8],
) -> std::result::Result<Vec<u8>, CryptError> {
    use age::DecryptError as D;
    let map = |e: D| match e {
        D::DecryptionFailed | D::NoMatchingKeys => CryptError::NoMatch,
        D::ExcessiveWork { .. } => {
            CryptError::Other("the passphrase lock is too costly to open on this computer".into())
        }
        other => CryptError::Other(format!("cannot decrypt: {other}")),
    };
    let dec = age::Decryptor::new(age::armor::ArmoredReader::new(ciphertext)).map_err(map)?;
    let mut r = dec.decrypt(std::iter::once(id)).map_err(map)?;
    let mut out = Vec::new();
    r.read_to_end(&mut out)
        .map_err(|e| CryptError::Other(format!("cannot decrypt: {e}")))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pass(s: &str) -> Passphrase {
        Passphrase::for_tests(s)
    }

    #[test]
    fn db_key_hex_round_trips() {
        let k = DbKey::generate().unwrap();
        assert_eq!(k.hex().len(), 64);
        assert_eq!(DbKey::from_hex(&k.hex()).unwrap(), k);
        assert!(k.sqlcipher_value().starts_with("x'"));
        assert!(k.display_value().starts_with("0x"));
        assert_ne!(DbKey::generate().unwrap(), k, "keys are random");
        assert!(DbKey::from_hex("zz").is_err());
    }

    #[test]
    fn key_file_unlocks_with_its_passphrase_only() {
        let (file, key) = KeyFile::create(&pass("correct horse")).unwrap();
        let u = file.unlock(&pass("correct horse")).unwrap();
        assert_eq!(u.db_key, key);
        assert_eq!(
            file.unlock(&pass("wrong")).unwrap_err(),
            Error::WrongPassphrase
        );
    }

    #[test]
    fn empty_passphrase_is_refused() {
        assert!(matches!(
            KeyFile::create(&pass("  ")),
            Err(Error::Invalid(_))
        ));
    }

    #[test]
    fn key_file_survives_write_and_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kansha.key");
        let (file, key) = KeyFile::create(&pass("p")).unwrap();
        file.write(&path).unwrap();
        let back = KeyFile::read(&path).unwrap();
        assert_eq!(back, file);
        assert_eq!(back.unlock(&pass("p")).unwrap().db_key, key);
        // No passphrase or database key in the clear.
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains(&key.hex()));
        assert!(!text.contains("AGE-SECRET-KEY"));
    }

    #[test]
    fn damaged_or_missing_key_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kansha.key");
        assert!(matches!(KeyFile::read(&path), Err(Error::Io(_))));
        std::fs::write(&path, "{ not json").unwrap();
        assert!(matches!(KeyFile::read(&path), Err(Error::Invalid(_))));
    }

    #[test]
    fn change_passphrase_keeps_database_key_and_changes_key_pair() {
        let (file, key) = KeyFile::create(&pass("old")).unwrap();
        let old_public = file.public_key().unwrap().to_string();
        assert_eq!(
            file.change_passphrase(&pass("bad"), &pass("new"))
                .unwrap_err(),
            Error::WrongPassphrase
        );
        let changed = file.change_passphrase(&pass("old"), &pass("new")).unwrap();
        assert_eq!(changed.unlock(&pass("new")).unwrap().db_key, key);
        assert_eq!(
            changed.unlock(&pass("old")).unwrap_err(),
            Error::WrongPassphrase
        );
        assert_ne!(changed.public_key().unwrap().to_string(), old_public);
    }

    #[test]
    fn public_key_encrypts_for_private_key() {
        let k = PrivateKey::generate();
        let data = k.public().encrypt(b"ledger").unwrap();
        assert_eq!(k.decrypt(&data).unwrap(), b"ledger");
        let other = PrivateKey::generate();
        assert!(other.decrypt(&data).is_err());
    }

    #[test]
    fn locked_private_key_round_trips() {
        let k = PrivateKey::generate();
        let locked = k.lock(&pass("p")).unwrap();
        let again = LockedPrivateKey::from_armored(locked.armored().to_owned());
        let back = again.unlock(&pass("p")).unwrap();
        assert_eq!(back.public().to_string(), k.public().to_string());
        assert_eq!(
            again.unlock(&pass("q")).unwrap_err(),
            Error::WrongPassphrase
        );
    }
}
