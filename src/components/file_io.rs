//! Local file access for the native panel. All callers run these operations off the UI thread.
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub const MAX_TEXT_BYTES: u64 = 2 * 1024 * 1024;
static NEXT_SAVE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
pub struct FileEntry {
    pub path: PathBuf,
    pub directory: bool,
}

pub fn read_directory(path: &Path) -> Result<Vec<FileEntry>, String> {
    let mut entries = Vec::new();
    for item in fs::read_dir(path).map_err(|e| e.to_string())? {
        let item = item.map_err(|e| e.to_string())?;
        let kind = item.file_type().map_err(|e| e.to_string())?;
        entries.push(FileEntry {
            path: item.path(),
            directory: kind.is_dir(),
        });
    }
    entries.sort_by(|a, b| {
        b.directory
            .cmp(&a.directory)
            .then_with(|| a.path.file_name().cmp(&b.path.file_name()))
    });
    Ok(entries)
}

#[derive(Clone, Debug)]
pub struct TextFile {
    pub path: PathBuf,
    pub text: String,
    pub bytes: Vec<u8>,
    crlf: bool,
    bom: bool,
}
impl TextFile {
    pub fn read(path: &Path) -> Result<Self, String> {
        let path = path
            .canonicalize()
            .map_err(|e| format!("无法打开文件：{e}"))?;
        let file = fs::File::open(&path).map_err(|e| format!("无法读取文件：{e}"))?;
        if !file.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("仅支持普通文件".into());
        }
        let mut bytes = Vec::new();
        file.take(MAX_TEXT_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_TEXT_BYTES {
            return Err("文件超过 2 MB，请在外部编辑器中打开".into());
        }
        if bytes.contains(&0) {
            return Err("此文件为二进制文件，无法作为文本编辑".into());
        }
        let bom = bytes.starts_with(&[0xef, 0xbb, 0xbf]);
        let raw = std::str::from_utf8(&bytes[if bom { 3 } else { 0 }..])
            .map_err(|_| "此文件不是 UTF-8 文本，请在外部编辑器中打开")?;
        if raw.lines().any(|line| line.len() > 65_536) {
            return Err("单行超过 64 KB，请在外部编辑器中打开".into());
        }
        let crlf = raw.contains("\r\n") && !raw.replace("\r\n", "").contains('\n');
        // Only normalize consistent CRLF files; mixed endings are kept verbatim.
        let text = if crlf {
            raw.replace("\r\n", "\n")
        } else {
            raw.to_string()
        };
        Ok(Self {
            path,
            text,
            bytes,
            crlf,
            bom,
        })
    }
    fn encoded(&self, text: &str) -> Vec<u8> {
        let mut bytes = if self.bom {
            vec![0xef, 0xbb, 0xbf]
        } else {
            Vec::new()
        };
        if self.crlf {
            bytes.extend_from_slice(text.replace('\n', "\r\n").as_bytes());
        } else {
            bytes.extend_from_slice(text.as_bytes());
        }
        bytes
    }
    pub fn save(&self, text: &str) -> Result<Self, String> {
        let metadata = fs::metadata(&self.path).map_err(|e| format!("无法保存：{e}"))?;
        if metadata.permissions().readonly() {
            return Err("文件为只读，无法保存".into());
        }
        let check = || -> Result<(), String> {
            let current = fs::read(&self.path).map_err(|e| format!("无法保存：{e}"))?;
            if current != self.bytes {
                return Err(
                    "文件已被其他程序修改。请复制当前编辑内容后重新加载，以免覆盖外部更改。".into(),
                );
            }
            Ok(())
        };
        check()?;
        let bytes = self.encoded(text);
        let temp = self.path.with_file_name(format!(
            ".gpui-save-{}-{}",
            std::process::id(),
            NEXT_SAVE.fetch_add(1, Ordering::Relaxed)
        ));
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp)
                .map_err(|e| e.to_string())?;
            file.set_permissions(metadata.permissions())
                .map_err(|e| e.to_string())?;
            file.write_all(&bytes).map_err(|e| e.to_string())?;
            file.sync_all().map_err(|e| e.to_string())?;
            check()?;
            fs::rename(&temp, &self.path).map_err(|e| e.to_string())?;
            Ok(Self {
                path: self.path.clone(),
                text: text.to_string(),
                bytes,
                crlf: self.crlf,
                bom: self.bom,
            })
        })();
        if result.is_err() {
            let _ = fs::remove_file(temp);
        }
        result
    }
}

pub fn search_files(root: &Path, query: &str) -> Result<Vec<FileEntry>, String> {
    // Respect repository ignore rules and include untracked files. Arguments never pass through a shell.
    let output = std::process::Command::new("rg")
        .args(["--files", "--hidden", "-g", "!.git", "-0"])
        .current_dir(root)
        .output();
    let query = query.to_lowercase();
    let mut paths = if let Ok(output) = output
        && output.status.success()
    {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            output
                .stdout
                .split(|c| *c == 0)
                .filter(|p| !p.is_empty())
                .map(|p| root.join(std::ffi::OsStr::from_bytes(p)))
                .collect::<Vec<_>>()
        }
        #[cfg(not(unix))]
        {
            String::from_utf8_lossy(&output.stdout)
                .split('\0')
                .filter(|p| !p.is_empty())
                .map(|p| root.join(p))
                .collect::<Vec<_>>()
        }
    } else {
        // A machine without ripgrep can still browse and search local projects.
        let mut pending = vec![root.to_path_buf()];
        let mut paths = Vec::new();
        let mut visited = 0;
        while let Some(dir) = pending.pop() {
            visited += 1;
            if visited > 20_000 {
                break;
            }
            let entries = if dir == root {
                read_directory(&dir)?
            } else {
                read_directory(&dir).unwrap_or_default()
            };
            for entry in entries {
                if entry.directory {
                    if !matches!(
                        entry.path.file_name().and_then(|n| n.to_str()),
                        Some(".git" | "node_modules" | "target")
                    ) {
                        pending.push(entry.path);
                    }
                } else {
                    paths.push(entry.path);
                }
            }
        }
        paths
    };
    paths.retain(|p| {
        p.strip_prefix(root)
            .unwrap_or(p)
            .to_string_lossy()
            .to_lowercase()
            .contains(&query)
    });
    paths.sort();
    paths.truncate(500);
    Ok(paths
        .into_iter()
        .map(|path| FileEntry {
            path,
            directory: false,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(name: &str, bytes: &[u8]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "gpui-file-test-{}-{}",
            std::process::id(),
            NEXT_SAVE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join(name);
        fs::write(&p, bytes).unwrap();
        p
    }
    #[test]
    fn saves_utf8_bom_crlf_and_empty_files_losslessly() {
        let p = fixture("text.txt", "\u{feff}中文👋\r\nsecond\r\n".as_bytes());
        let f = TextFile::read(&p).unwrap();
        assert_eq!(f.text, "中文👋\nsecond\n");
        let saved = f.save("中文👋\nchanged\n").unwrap();
        assert_eq!(
            fs::read(&p).unwrap(),
            "\u{feff}中文👋\r\nchanged\r\n".as_bytes()
        );
        saved.save("").unwrap();
        assert_eq!(fs::read(&p).unwrap(), [0xef, 0xbb, 0xbf]);
        fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }
    #[test]
    fn rejects_external_changes_and_deleted_files() {
        let p = fixture("text.txt", b"original");
        let f = TextFile::read(&p).unwrap();
        fs::write(&p, "external").unwrap();
        assert!(f.save("editor").unwrap_err().contains("其他程序"));
        assert_eq!(fs::read(&p).unwrap(), b"external");
        fs::remove_file(&p).unwrap();
        assert!(f.save("editor").is_err());
        fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }
    #[test]
    fn rejects_binary_and_oversized_files() {
        let p = fixture("binary", b"a\0b");
        assert!(TextFile::read(&p).is_err());
        fs::write(&p, vec![b'x'; MAX_TEXT_BYTES as usize + 1]).unwrap();
        assert!(TextFile::read(&p).is_err());
        fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn preserves_permissions_and_symlink_target() {
        use std::os::unix::fs::{PermissionsExt, symlink};
        let p = fixture("target", b"old");
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
        let link = p.with_file_name("link");
        symlink(&p, &link).unwrap();
        TextFile::read(&link).unwrap().save("new").unwrap();
        assert!(fs::symlink_metadata(&link).unwrap().is_symlink());
        assert_eq!(
            fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o755
        );
        fs::remove_dir_all(p.parent().unwrap()).unwrap();
    }
}
