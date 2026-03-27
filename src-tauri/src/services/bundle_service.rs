use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use chrono::Utc;
use walkdir::WalkDir;
use zip::CompressionMethod;
use zip::ZipWriter;
use zip::write::FileOptions;

use crate::domain::errors::{bad_request, AppError, AppResult};
use crate::domain::models::{SourceEntry, SourceKind};

#[derive(Debug, Clone)]
pub struct BundleArtifact {
    pub archive_path: String,
    pub file_name: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct BundleBuildProgress {
    pub total_files: u64,
    pub processed_files: u64,
    pub total_bytes: u64,
    pub processed_bytes: u64,
    pub current_entry: Option<String>,
}

#[derive(Debug, Clone)]
struct BundleInputFile {
    file_path: PathBuf,
    archive_entry: String,
    size_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct BundleService {
    temp_dir: PathBuf,
}

impl BundleService {
    pub fn new(temp_dir: PathBuf) -> AppResult<Self> {
        fs::create_dir_all(&temp_dir)?;
        Ok(Self { temp_dir })
    }

    pub fn build_bundle(&self, sources: &[SourceEntry]) -> AppResult<BundleArtifact> {
        self.build_bundle_with_progress(sources, |_| Ok(()))
    }

    pub fn cleanup_temp_archives(&self) -> AppResult<()> {
        if !self.temp_dir.exists() {
            return Ok(());
        }

        let entries = fs::read_dir(&self.temp_dir)?;
        for entry in entries {
            let entry = match entry {
                Ok(value) => value,
                Err(_) => continue,
            };
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(name) = path.file_name().and_then(|it| it.to_str()) else {
                continue;
            };
            if name.starts_with("upload_bundle_") && name.ends_with(".zip") {
                let _ = fs::remove_file(path);
            }
        }

        Ok(())
    }

    pub fn build_bundle_with_progress<F>(
        &self,
        sources: &[SourceEntry],
        mut on_progress: F,
    ) -> AppResult<BundleArtifact>
    where
        F: FnMut(BundleBuildProgress) -> AppResult<()>,
    {
        if sources.is_empty() {
            return Err(AppError::NoSources);
        }

        let files = collect_bundle_inputs(sources)?;
        let total_files = files.len() as u64;
        let total_input_bytes = files.iter().map(|item| item.size_bytes).sum::<u64>();

        let file_name = format!("upload_bundle_{}.zip", Utc::now().format("%Y%m%d_%H%M%S"));
        let archive_path = self.temp_dir.join(file_name.clone());
        let file = File::create(&archive_path)?;
        let mut writer = ZipWriter::new(file);
        let options = FileOptions::default().compression_method(CompressionMethod::Deflated);
        let mut processed_files = 0_u64;
        let mut processed_input_bytes = 0_u64;

        on_progress(BundleBuildProgress {
            total_files,
            processed_files,
            total_bytes: total_input_bytes,
            processed_bytes: processed_input_bytes,
            current_entry: None,
        })?;

        for input in &files {
            let file_label = Path::new(&input.archive_entry)
                .file_name()
                .and_then(|it| it.to_str())
                .unwrap_or(&input.archive_entry)
                .to_string();

            on_progress(BundleBuildProgress {
                total_files,
                processed_files,
                total_bytes: total_input_bytes,
                processed_bytes: processed_input_bytes,
                current_entry: Some(file_label.clone()),
            })?;

            add_file_entry_with_progress(
                &mut writer,
                &input.file_path,
                &input.archive_entry,
                options,
                |delta| {
                    processed_input_bytes = processed_input_bytes.saturating_add(delta);
                    on_progress(BundleBuildProgress {
                        total_files,
                        processed_files,
                        total_bytes: total_input_bytes,
                        processed_bytes: processed_input_bytes,
                        current_entry: Some(file_label.clone()),
                    })?;
                    Ok(())
                },
            )?;

            processed_files = processed_files.saturating_add(1);
            on_progress(BundleBuildProgress {
                total_files,
                processed_files,
                total_bytes: total_input_bytes,
                processed_bytes: processed_input_bytes,
                current_entry: Some(file_label),
            })?;
        }

        writer
            .finish()
            .map_err(|err| AppError::Internal(err.to_string()))?;
        let size_bytes = fs::metadata(&archive_path)?.len();
        Ok(BundleArtifact {
            archive_path: archive_path.to_string_lossy().to_string(),
            file_name,
            size_bytes,
        })
    }
}

fn collect_bundle_inputs(sources: &[SourceEntry]) -> AppResult<Vec<BundleInputFile>> {
    let mut used_paths = HashSet::new();
    let mut files = Vec::new();

    for source in sources {
        let source_path = PathBuf::from(&source.path);
        if !source_path.exists() {
            continue;
        }

        match source.kind {
            SourceKind::File => {
                let base_name = source_path
                    .file_name()
                    .and_then(|it| it.to_str())
                    .ok_or_else(|| bad_request("invalid source file name"))?;
                let archive_entry = unique_archive_path(base_name, &mut used_paths);
                let size_bytes = fs::metadata(&source_path).map(|m| m.len()).unwrap_or(0);
                files.push(BundleInputFile {
                    file_path: source_path,
                    archive_entry,
                    size_bytes,
                });
            }
            SourceKind::Folder => {
                let root_name = source_path
                    .file_name()
                    .and_then(|it| it.to_str())
                    .ok_or_else(|| bad_request("invalid source folder name"))?;

                let mut walker = WalkDir::new(&source_path).follow_links(false).min_depth(1);
                if !source.recursive {
                    walker = walker.max_depth(1);
                }

                for entry in walker {
                    let entry = match entry {
                        Ok(item) => item,
                        Err(_) => continue,
                    };
                    if !entry.file_type().is_file() {
                        continue;
                    }

                    let relative = entry
                        .path()
                        .strip_prefix(&source_path)
                        .ok()
                        .map(normalize_relative)
                        .unwrap_or_else(|| {
                            entry
                                .path()
                                .file_name()
                                .and_then(|it| it.to_str())
                                .unwrap_or("unknown")
                                .to_string()
                        });
                    let joined = format!("{}/{}", root_name, relative);
                    let archive_entry = unique_archive_path(&joined, &mut used_paths);
                    let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
                    files.push(BundleInputFile {
                        file_path: entry.path().to_path_buf(),
                        archive_entry,
                        size_bytes,
                    });
                }
            }
        }
    }

    Ok(files)
}

fn add_file_entry_with_progress<F>(
    writer: &mut ZipWriter<File>,
    file_path: &Path,
    archive_entry: &str,
    options: FileOptions,
    mut on_bytes: F,
) -> AppResult<()>
where
    F: FnMut(u64) -> AppResult<()>,
{
    let mut src = File::open(file_path)?;
    writer
        .start_file(archive_entry, options)
        .map_err(|err| AppError::Internal(err.to_string()))?;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = src.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        writer.write_all(&buffer[..read])?;
        on_bytes(read as u64)?;
    }
    Ok(())
}

fn normalize_relative(path: &Path) -> String {
    path.components()
        .map(|part| part.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/")
}

fn unique_archive_path(candidate: &str, used: &mut HashSet<String>) -> String {
    if used.insert(candidate.to_string()) {
        return candidate.to_string();
    }

    let (base, ext) = split_ext(candidate);
    let mut idx = 2_u32;
    loop {
        let next = if ext.is_empty() {
            format!("{base}_{idx}")
        } else {
            format!("{base}_{idx}.{ext}")
        };
        if used.insert(next.clone()) {
            return next;
        }
        idx += 1;
    }
}

fn split_ext(path: &str) -> (String, String) {
    match path.rsplit_once('.') {
        Some((base, ext)) if !base.is_empty() && !ext.contains('/') => {
            (base.to_string(), ext.to_string())
        }
        _ => (path.to_string(), String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::tempdir;
    use uuid::Uuid;

    fn source(path: &str, kind: SourceKind) -> SourceEntry {
        SourceEntry {
            id: Uuid::new_v4().to_string(),
            path: path.to_string(),
            kind,
            recursive: true,
            added_at: Utc::now(),
        }
    }

    #[test]
    fn bundles_with_roots_and_file_at_zip_root() {
        let root = tempdir().expect("temp root");
        let tmp = tempdir().expect("temp output");

        let folder_a = root.path().join("folder_a");
        let folder_b = root.path().join("folder_b");
        fs::create_dir_all(folder_a.join("sub")).expect("mkdir a");
        fs::create_dir_all(&folder_b).expect("mkdir b");
        fs::write(folder_a.join("sub").join("a.txt"), b"a").expect("write a");
        fs::write(folder_b.join("b.txt"), b"b").expect("write b");
        let loose_file = root.path().join("loose.txt");
        fs::write(&loose_file, b"c").expect("write c");

        let svc = BundleService::new(tmp.path().to_path_buf()).expect("bundle svc");
        let out = svc
            .build_bundle(&[
                source(
                    folder_a
                        .to_str()
                        .expect("utf8 folder_a"),
                    SourceKind::Folder,
                ),
                source(
                    folder_b
                        .to_str()
                        .expect("utf8 folder_b"),
                    SourceKind::Folder,
                ),
                source(
                    loose_file
                        .to_str()
                        .expect("utf8 loose_file"),
                    SourceKind::File,
                ),
            ])
            .expect("build bundle");

        let file = File::open(out.archive_path).expect("open zip");
        let mut zip = zip::ZipArchive::new(file).expect("zip archive");
        assert!(zip.by_name("folder_a/sub/a.txt").is_ok());
        assert!(zip.by_name("folder_b/b.txt").is_ok());
        assert!(zip.by_name("loose.txt").is_ok());
    }

    #[test]
    fn avoids_name_collisions_for_duplicate_root_files() {
        let root = tempdir().expect("temp root");
        let tmp = tempdir().expect("temp output");

        let first = root.path().join("same.txt");
        let second_dir = root.path().join("extra");
        fs::create_dir_all(&second_dir).expect("mkdir extra");
        let second = second_dir.join("same.txt");

        fs::write(&first, b"first").expect("write first");
        fs::write(&second, b"second").expect("write second");

        let svc = BundleService::new(tmp.path().to_path_buf()).expect("bundle svc");
        let out = svc
            .build_bundle(&[
                source(first.to_str().expect("utf8 first"), SourceKind::File),
                source(second.to_str().expect("utf8 second"), SourceKind::File),
            ])
            .expect("build bundle");

        let file = File::open(out.archive_path).expect("open zip");
        let mut zip = zip::ZipArchive::new(file).expect("zip archive");
        assert!(zip.by_name("same.txt").is_ok());
        assert!(zip.by_name("same_2.txt").is_ok());
    }
}
