//! One process owns an account read/change transaction at a time.
//! Never unlink the lock file: replacing its inode would split ownership.
use super::Account;
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};
const MAX_BYTES: u64 = 64 * 1024;

fn regular(path: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_file() => Ok(()),
        Ok(_) => Err(format!("{} must be a regular account file", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}
pub(super) fn read(path: &Path) -> Result<Option<Account>, String> {
    regular(path)?;
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("account must be a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("account file exceeds 64 KiB".into());
    }
    serde_json::from_slice(&bytes).map(Some).map_err(|_| {
        format!(
            "{} is not an account file; `rroflow logout` removes it",
            path.display()
        )
    })
}
fn private_options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options.read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
}
fn sync_parent(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    File::open(path.parent().unwrap())
        .and_then(|f| f.sync_all())
        .map_err(|e| format!("account directory sync failed: {e}"))?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}
pub(super) struct LockedAccount {
    path: PathBuf,
    _lock: File,
}
impl LockedAccount {
    pub(super) fn acquire(path: &Path) -> Result<Self, String> {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .ok_or("account file needs a parent directory")?;
        let mut directories = std::fs::DirBuilder::new();
        directories.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            directories.mode(0o700);
        }
        directories
            .create(parent)
            .map_err(|e| format!("{}: {e}", parent.display()))?;
        let lock_path = path.with_extension("json.lock");
        regular(&lock_path)?;
        let lock = private_options()
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(|e| format!("cannot open account lock: {e}"))?;
        lock.try_lock().map_err(|e|match e{
            std::fs::TryLockError::WouldBlock=>"account is being updated by another process; retry shortly (queued captures are retained)".to_string(),
            std::fs::TryLockError::Error(e)=>format!("cannot lock account: {e}"),
        })?;
        Ok(Self {
            path: path.to_owned(),
            _lock: lock,
        })
    }
    // A durable mutation marker also detects logout while a first login is
    // waiting for approval and account.json does not exist yet.
    pub(super) fn revision(&self) -> Result<Vec<u8>, String> {
        let mut lock = &self._lock;
        lock.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        lock.take(17)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 16 {
            return Err("invalid account mutation marker".into());
        }
        Ok(bytes)
    }
    fn changed(&self) -> Result<(), String> {
        let mut lock = &self._lock;
        lock.seek(SeekFrom::Start(0))
            .and_then(|_| lock.set_len(0))
            .and_then(|_| lock.write_all(uuid::Uuid::new_v4().as_bytes()))
            .and_then(|_| lock.sync_all())
            .map_err(|e| format!("account mutation marker failed: {e}"))
    }
    pub(super) fn read(&self) -> Result<Option<Account>, String> {
        read(&self.path)
    }
    pub(super) fn write(&self, account: &Account) -> Result<(), String> {
        regular(&self.path)?;
        let text = serde_json::to_vec_pretty(account).map_err(|e| e.to_string())?;
        if text.len() as u64 >= MAX_BYTES {
            return Err("account file exceeds 64 KiB".into());
        }
        self.changed()?;
        let temp = self
            .path
            .with_extension(format!("json.{}.tmp", uuid::Uuid::new_v4()));
        let mut file = private_options()
            .create_new(true)
            .open(&temp)
            .map_err(|e| format!("cannot create private account update: {e}"))?;
        let result = file
            .write_all(&text)
            .and_then(|_| file.write_all(b"\n"))
            .and_then(|_| file.sync_all());
        drop(file);
        let result = result.and_then(|_| std::fs::rename(&temp, &self.path));
        if let Err(e) = result {
            let _ = std::fs::remove_file(&temp);
            return Err(format!("account update failed: {e}"));
        }
        sync_parent(&self.path)
    }
    pub(super) fn remove(&self) -> Result<bool, String> {
        regular(&self.path)?;
        self.changed()?;
        match std::fs::remove_file(&self.path) {
            Ok(()) => {
                sync_parent(&self.path)?;
                Ok(true)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(format!("{}: {e}", self.path.display())),
        }
    }
}
