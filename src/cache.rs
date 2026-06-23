use std::collections::{HashMap, HashSet};
use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::{Context, Result};

use crate::parser::FileVariants;

const HEADER_V1: &[u8] = b"BONSAI_PARSE_CACHE_V1\n";
const HEADER_V2: &[u8] = b"BONSAI_PARSE_CACHE_V2\n";

#[derive(Debug)]
pub struct ParseCache {
    path: PathBuf,
    metadata: Option<CacheMetadata>,
    entries: HashMap<String, CacheEntry>,
    touched: HashSet<String>,
    dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheMetadata {
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub respect_gitignore: bool,
    pub max_file_bytes: Option<u64>,
}

#[derive(Debug, Clone)]
struct CacheEntry {
    size: u64,
    modified_ns: u128,
    variants: FileVariants,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheStatus {
    Added,
    Changed,
    Unchanged,
}

impl ParseCache {
    pub fn load(path: PathBuf) -> Self {
        let parsed = fs::read(&path)
            .ok()
            .and_then(|bytes| parse_cache_bytes(&bytes))
            .unwrap_or_default();

        Self {
            path,
            metadata: parsed.metadata,
            entries: parsed.entries,
            touched: HashSet::new(),
            dirty: false,
        }
    }

    pub fn get(&mut self, path: &Path, metadata: &Metadata) -> Option<FileVariants> {
        let key = cache_key(path);
        self.touched.insert(key.clone());

        self.get_by_key(&key, metadata)
    }

    pub fn status(&self, path: &Path, metadata: &Metadata) -> CacheStatus {
        let key = cache_key(path);
        match self.entries.get(&key) {
            None => CacheStatus::Added,
            Some(entry)
                if entry.size == metadata.len() && entry.modified_ns == modified_ns(metadata) =>
            {
                CacheStatus::Unchanged
            }
            Some(_) => CacheStatus::Changed,
        }
    }

    pub fn deleted_paths(&self, root: &Path) -> Vec<String> {
        let mut paths = self
            .entries
            .keys()
            .filter(|path| !Path::new(path.as_str()).exists())
            .map(|path| {
                let path = Path::new(path.as_str());
                path.strip_prefix(root)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect::<Vec<_>>();
        paths.sort();
        paths
    }

    pub fn load_required(path: PathBuf) -> Result<Self> {
        let bytes = fs::read(&path)
            .with_context(|| format!("cannot read incremental base cache {}", path.display()))?;
        let parsed = parse_cache_bytes(&bytes)
            .with_context(|| format!("invalid incremental base cache {}", path.display()))?;

        Ok(Self {
            path,
            metadata: parsed.metadata,
            entries: parsed.entries,
            touched: HashSet::new(),
            dirty: false,
        })
    }

    fn get_by_key(&self, key: &str, metadata: &Metadata) -> Option<FileVariants> {
        let entry = self.entries.get(key)?;
        if entry.size == metadata.len() && entry.modified_ns == modified_ns(metadata) {
            return Some(entry.variants.clone());
        }

        None
    }

    pub fn put(&mut self, path: &Path, metadata: &Metadata, variants: FileVariants) {
        let key = cache_key(path);
        self.touched.insert(key.clone());
        self.entries.insert(
            key,
            CacheEntry {
                size: metadata.len(),
                modified_ns: modified_ns(metadata),
                variants,
            },
        );
        self.dirty = true;
    }

    pub fn metadata_matches(&self, metadata: &CacheMetadata) -> bool {
        self.metadata.as_ref() == Some(metadata)
    }

    pub fn set_metadata(&mut self, metadata: CacheMetadata) {
        if self.metadata.as_ref() != Some(&metadata) {
            self.metadata = Some(metadata);
            self.dirty = true;
        }
    }

    pub fn retain_touched(&mut self) {
        let before = self.entries.len();
        self.entries.retain(|key, _| self.touched.contains(key));
        self.dirty |= self.entries.len() != before;
    }

    pub fn save(&self) -> Result<()> {
        if !self.dirty {
            return Ok(());
        }

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("cannot create cache dir {}", parent.display()))?;
        }

        fs::write(
            &self.path,
            format_cache_bytes(&self.metadata, &self.entries),
        )
        .with_context(|| format!("cannot write parse cache {}", self.path.display()))
    }
}

#[derive(Debug, Default)]
struct ParsedCache {
    metadata: Option<CacheMetadata>,
    entries: HashMap<String, CacheEntry>,
}

pub fn cache_path_for_root(root: &Path) -> PathBuf {
    std::env::temp_dir()
        .join("bonsai-parse-cache")
        .join(format!("{:016x}.cache", stable_hash(root)))
}

fn stable_hash(path: &Path) -> u64 {
    let mut hash = 0xcbf29ce484222325;
    for byte in path.to_string_lossy().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn cache_key(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn modified_ns(metadata: &Metadata) -> u128 {
    metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

fn format_cache_bytes(
    metadata: &Option<CacheMetadata>,
    entries: &HashMap<String, CacheEntry>,
) -> Vec<u8> {
    let mut output = HEADER_V2.to_vec();
    push_metadata(&mut output, metadata);
    let mut entries = entries.iter().collect::<Vec<_>>();
    entries.sort_by(|(left, _), (right, _)| left.cmp(right));

    for (path, entry) in entries {
        let full = entry.variants.full.as_deref();
        output.extend_from_slice(
            format!(
                "{} {} {} {} {} {}\n",
                path.len(),
                entry.modified_ns,
                entry.size,
                full.map(str::len)
                    .map(|length| length.to_string())
                    .unwrap_or_else(|| "-1".to_owned()),
                entry.variants.skeleton.len(),
                entry.variants.tree_map.len()
            )
            .as_bytes(),
        );
        push_field(&mut output, path);
        if let Some(full) = full {
            push_field(&mut output, full);
        }
        push_field(&mut output, &entry.variants.skeleton);
        push_field(&mut output, &entry.variants.tree_map);
    }

    output
}

fn push_metadata(output: &mut Vec<u8>, metadata: &Option<CacheMetadata>) {
    let Some(metadata) = metadata else {
        output.extend_from_slice(b"OPTIONS 0 -1 0 0\n");
        return;
    };

    output.extend_from_slice(
        format!(
            "OPTIONS {} {} {} {}\n",
            u8::from(metadata.respect_gitignore),
            metadata
                .max_file_bytes
                .map(|value| value.to_string())
                .unwrap_or_else(|| "-1".to_owned()),
            metadata.include.len(),
            metadata.exclude.len()
        )
        .as_bytes(),
    );

    for pattern in &metadata.include {
        push_sized_field(output, pattern);
    }

    for pattern in &metadata.exclude {
        push_sized_field(output, pattern);
    }
}

fn push_sized_field(output: &mut Vec<u8>, value: &str) {
    output.extend_from_slice(format!("{}\n", value.len()).as_bytes());
    push_field(output, value);
}

fn push_field(output: &mut Vec<u8>, value: &str) {
    output.extend_from_slice(value.as_bytes());
    output.push(b'\n');
}

fn parse_cache_bytes(bytes: &[u8]) -> Option<ParsedCache> {
    if bytes.starts_with(HEADER_V1) {
        let entries = parse_entries(bytes, HEADER_V1.len())?;
        return Some(ParsedCache {
            metadata: None,
            entries,
        });
    }

    if !bytes.starts_with(HEADER_V2) {
        return None;
    }

    let mut cursor = HEADER_V2.len();
    let metadata = parse_metadata(bytes, &mut cursor)?;
    let entries = parse_entries(bytes, cursor)?;

    Some(ParsedCache {
        metadata: Some(metadata),
        entries,
    })
}

fn parse_metadata(bytes: &[u8], cursor: &mut usize) -> Option<CacheMetadata> {
    let line = std::str::from_utf8(read_line(bytes, cursor)?).ok()?;
    let parts = line.split_whitespace().collect::<Vec<_>>();
    if parts.len() != 5 || parts[0] != "OPTIONS" {
        return None;
    }

    let respect_gitignore = match parts[1] {
        "0" => false,
        "1" => true,
        _ => return None,
    };
    let max_file_bytes = match parts[2] {
        "-1" => None,
        value => Some(value.parse::<u64>().ok()?),
    };
    let include_count = parts[3].parse::<usize>().ok()?;
    let exclude_count = parts[4].parse::<usize>().ok()?;

    Some(CacheMetadata {
        include: read_patterns(bytes, cursor, include_count)?,
        exclude: read_patterns(bytes, cursor, exclude_count)?,
        respect_gitignore,
        max_file_bytes,
    })
}

fn read_patterns(bytes: &[u8], cursor: &mut usize, count: usize) -> Option<Vec<String>> {
    let mut patterns = Vec::with_capacity(count);
    for _ in 0..count {
        let length = std::str::from_utf8(read_line(bytes, cursor)?)
            .ok()?
            .parse::<usize>()
            .ok()?;
        patterns.push(read_string(bytes, cursor, length)?);
    }
    Some(patterns)
}

fn parse_entries(bytes: &[u8], mut cursor: usize) -> Option<HashMap<String, CacheEntry>> {
    let mut entries = HashMap::new();

    while cursor < bytes.len() {
        let line = read_line(bytes, &mut cursor)?;
        if line.is_empty() {
            continue;
        }

        let parts = std::str::from_utf8(line)
            .ok()?
            .split_whitespace()
            .collect::<Vec<_>>();
        if parts.len() != 6 {
            return None;
        }

        let path_len = parts[0].parse::<usize>().ok()?;
        let modified_ns = parts[1].parse::<u128>().ok()?;
        let size = parts[2].parse::<u64>().ok()?;
        let full_len = parts[3].parse::<isize>().ok()?;
        let skeleton_len = parts[4].parse::<usize>().ok()?;
        let tree_map_len = parts[5].parse::<usize>().ok()?;

        let path = read_string(bytes, &mut cursor, path_len)?;
        let full = if full_len < 0 {
            None
        } else {
            Some(read_string(bytes, &mut cursor, full_len as usize)?)
        };
        let skeleton = read_string(bytes, &mut cursor, skeleton_len)?;
        let tree_map = read_string(bytes, &mut cursor, tree_map_len)?;

        entries.insert(
            path,
            CacheEntry {
                size,
                modified_ns,
                variants: FileVariants {
                    full,
                    skeleton,
                    tree_map,
                },
            },
        );
    }

    Some(entries)
}

fn read_line<'a>(bytes: &'a [u8], cursor: &mut usize) -> Option<&'a [u8]> {
    let start = *cursor;
    let rest = bytes.get(start..)?;
    let newline = rest.iter().position(|byte| *byte == b'\n')?;
    *cursor = start + newline + 1;
    Some(&rest[..newline])
}

fn read_string(bytes: &[u8], cursor: &mut usize, length: usize) -> Option<String> {
    let start = *cursor;
    let end = start.checked_add(length)?;
    let value = String::from_utf8(bytes.get(start..end)?.to_vec()).ok()?;
    if bytes.get(end) != Some(&b'\n') {
        return None;
    }
    *cursor = end + 1;
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_cache_entries() {
        let mut entries = HashMap::new();
        entries.insert(
            "/repo/src/lib.rs".to_owned(),
            CacheEntry {
                size: 12,
                modified_ns: 34,
                variants: FileVariants {
                    full: Some("fn main() {}\n".to_owned()),
                    skeleton: "fn main() { ... }\n".to_owned(),
                    tree_map: "fn main()\n".to_owned(),
                },
            },
        );

        let metadata = Some(CacheMetadata {
            include: vec!["src/**".to_owned()],
            exclude: vec!["**/generated.rs".to_owned()],
            respect_gitignore: true,
            max_file_bytes: Some(1024),
        });

        let bytes = format_cache_bytes(&metadata, &entries);
        let parsed = parse_cache_bytes(&bytes).unwrap();
        let entry = parsed.entries.get("/repo/src/lib.rs").unwrap();

        assert_eq!(parsed.metadata, metadata);
        assert_eq!(entry.size, 12);
        assert_eq!(entry.modified_ns, 34);
        assert_eq!(entry.variants.full.as_deref(), Some("fn main() {}\n"));
        assert_eq!(entry.variants.skeleton, "fn main() { ... }\n");
        assert_eq!(entry.variants.tree_map, "fn main()\n");
    }
}
