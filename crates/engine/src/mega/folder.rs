//! Fetch and reconstruct the node tree behind a MEGA *folder* link.
//!
//! Listing is unmetered — it never touches the 5 GB download quota — so this is
//! the foundation that lets us always know the correct hierarchy regardless of
//! where the bytes ultimately come from.
//!
//! Flow:
//!   1. POST `[{"a":"f","c":1,"r":1}]` to the `cs` endpoint with `n=<folderId>`.
//!   2. For each returned node, unwrap its key with the folder master key
//!      (AES-ECB), decrypt its attributes (AES-CBC) to recover the name.
//!   3. Re-link nodes by parent handle and compute on-disk relative paths.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::mega::{crypto, link::MegaLink};
use crate::{Error, Result};

const API_URL: &str = "https://g.api.mega.co.nz/cs";

/// Shared HTTP client for MEGA API calls, with timeouts so a wedged request
/// can never hang a caller indefinitely.
fn http() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(60))
            .build()
            .expect("failed to build HTTP client")
    })
}

/// A node as returned by the MEGA API (encrypted).
#[derive(Debug, Deserialize)]
struct RawNode {
    /// node handle
    h: String,
    /// parent handle
    #[serde(default)]
    p: String,
    /// type: 0 = file, 1 = folder (2..=4 are account roots, not seen in folder links)
    t: i64,
    /// encrypted attributes
    #[serde(default)]
    a: Option<String>,
    /// encrypted key, formatted `<sharehandle>:<base64>`
    #[serde(default)]
    k: Option<String>,
    /// size in bytes (files only)
    #[serde(default)]
    s: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    File,
    Folder,
}

/// A decrypted node in the reconstructed tree.
#[derive(Debug, Clone, Serialize)]
pub struct TreeNode {
    pub handle: String,
    /// Parent handle, or `None` for the shared root.
    pub parent: Option<String>,
    pub kind: NodeKind,
    pub name: String,
    pub size: i64,
    /// Path from the root (inclusive), used to fold files into the right dirs.
    pub rel_path: String,
    /// Full 32-byte decrypted node key (files only): AES key + nonce + meta-MAC.
    /// Needed by the native-MEGA fallback to AES-CTR-decrypt the content.
    #[serde(skip)]
    pub file_key: Option<[u8; 32]>,
}

/// The fully reconstructed folder tree plus summary stats.
#[derive(Debug, Clone, Serialize)]
pub struct Tree {
    pub root_handle: String,
    pub root_name: String,
    pub total_files: usize,
    pub total_folders: usize,
    pub total_bytes: i64,
    pub nodes: Vec<TreeNode>,
}

/// Fetch and decrypt the tree for a folder link.
pub async fn fetch_tree(link: &MegaLink) -> Result<Tree> {
    let (folder_id, folder_key_b64) = match link {
        MegaLink::Folder { id, key, .. } => (id.as_str(), key.as_str()),
        MegaLink::File { .. } => {
            return Err(Error::Other(
                "standalone file links are not supported in Phase 1 (folders only)".into(),
            ))
        }
    };

    let master = master_key(folder_key_b64)?;
    let raw = fetch_raw_nodes(folder_id).await?;
    build_tree(&master, raw)
}

/// Decode the 16-byte folder master key from the link fragment.
fn master_key(folder_key_b64: &str) -> Result<[u8; 16]> {
    let bytes = crypto::b64decode(folder_key_b64)?;
    if bytes.len() < 16 {
        return Err(Error::InvalidLink("folder key too short".into()));
    }
    let mut key = [0u8; 16];
    key.copy_from_slice(&bytes[..16]);
    Ok(key)
}

/// Call the `cs` API and return the raw (still-encrypted) node list.
async fn fetch_raw_nodes(folder_id: &str) -> Result<Vec<RawNode>> {
    let body = serde_json::json!([{ "a": "f", "c": 1, "r": 1 }]);

    let resp = http()
        .post(API_URL)
        .query(&[("id", "0"), ("n", folder_id)])
        .json(&body)
        .send()
        .await?
        .error_for_status()?;

    let val: Value = resp.json().await?;

    // The API returns either a bare/array-wrapped negative error code, or
    // `[{ "f": [ ...nodes... ] }]`.
    let arr = val
        .as_array()
        .ok_or_else(|| Error::Other(format!("unexpected MEGA response: {val}")))?;

    if let Some(code) = arr.first().and_then(Value::as_i64) {
        return Err(Error::Other(format!(
            "MEGA API error {code} (e.g. -9 = not found, -16 = blocked/taken down)"
        )));
    }

    let files = arr
        .first()
        .and_then(|o| o.get("f"))
        .and_then(Value::as_array)
        .ok_or_else(|| Error::Other("MEGA response missing node list".into()))?;

    serde_json::from_value(Value::Array(files.clone()))
        .map_err(|e| Error::Other(format!("failed to parse nodes: {e}")))
}

/// Decrypt every node and assemble them into a tree with relative paths.
fn build_tree(master: &[u8; 16], raw: Vec<RawNode>) -> Result<Tree> {
    let mut decoded: Vec<TreeNode> = Vec::with_capacity(raw.len());
    // Raw parent handle per kept node (before we decide whether it's present).
    let mut parent_of: HashMap<String, String> = HashMap::with_capacity(raw.len());

    for n in &raw {
        let Some(raw_key) = decode_node_key(master, n) else {
            continue;
        };
        let kind = if n.t == 0 {
            NodeKind::File
        } else {
            NodeKind::Folder
        };

        // Attributes are decrypted with the 16-byte key: the folded AES key for
        // files, or the folder key directly.
        let attr_key: [u8; 16] = if kind == NodeKind::File {
            crypto::unpack_file_key(&raw_key)
        } else {
            let mut k = [0u8; 16];
            k.copy_from_slice(&raw_key[..16]);
            k
        };

        let name = n
            .a
            .as_deref()
            .and_then(|a| crypto::decrypt_attributes(&attr_key, a).ok())
            .and_then(|json| serde_json::from_str::<Value>(&json).ok())
            .and_then(|v| v.get("n").and_then(Value::as_str).map(str::to_owned))
            .unwrap_or_else(|| "(undecodable name)".to_string());

        // Files keep the full 32-byte key for native AES-CTR decryption.
        let file_key = if kind == NodeKind::File {
            let mut k = [0u8; 32];
            k.copy_from_slice(&raw_key[..32]);
            Some(k)
        } else {
            None
        };

        parent_of.insert(n.h.clone(), n.p.clone());
        decoded.push(TreeNode {
            handle: n.h.clone(),
            parent: None, // filled in below
            kind,
            name,
            size: n.s.unwrap_or(0),
            rel_path: String::new(), // filled in below
            file_key,
        });
    }

    // De-duplicate sibling names that would collide on disk. MEGA allows
    // same-name siblings, and sanitization can merge distinct names ("a:b" and
    // "a*b" → "a_b") on a case-insensitive filesystem — without this, one file
    // would silently overwrite the other.
    let mut used: HashSet<(String, String)> = HashSet::with_capacity(decoded.len());
    for node in &mut decoded {
        let parent = parent_of.get(&node.handle).cloned().unwrap_or_default();
        let norm = |name: &str| crate::download::sanitize_segment(name).to_lowercase();
        if used.insert((parent.clone(), norm(&node.name))) {
            continue;
        }
        for n in 2u32.. {
            let candidate = dedup_name(&node.name, n);
            if used.insert((parent.clone(), norm(&candidate))) {
                node.name = candidate;
                break;
            }
        }
    }

    // Resolve parents and relative paths (all O(n) map lookups).
    let present: HashSet<String> = decoded.iter().map(|n| n.handle.clone()).collect();
    let info: HashMap<String, (String, String)> = decoded
        .iter()
        .map(|d| {
            let p = parent_of.get(&d.handle).cloned().unwrap_or_default();
            (d.handle.clone(), (d.name.clone(), p))
        })
        .collect();
    let paths = compute_paths(&info);

    for node in &mut decoded {
        let parent = parent_of.get(&node.handle).cloned().unwrap_or_default();
        let parent_present = present.contains(&parent);
        node.parent = parent_present.then_some(parent);
        node.rel_path = paths.get(&node.handle).cloned().unwrap_or_default();
    }

    // The root is the node whose parent isn't in the listing — normally the
    // shared folder itself, so prefer a folder if several qualify.
    let mut root_handle = String::new();
    let mut root_name = String::from("(root)");
    if let Some(root) = decoded
        .iter()
        .find(|n| n.parent.is_none() && n.kind == NodeKind::Folder)
        .or_else(|| decoded.iter().find(|n| n.parent.is_none()))
    {
        root_handle = root.handle.clone();
        root_name = root.name.clone();
    }

    let total_files = decoded.iter().filter(|n| n.kind == NodeKind::File).count();
    let total_folders = decoded
        .iter()
        .filter(|n| n.kind == NodeKind::Folder)
        .count();
    let total_bytes = decoded.iter().map(|n| n.size).sum();

    decoded.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

    Ok(Tree {
        root_handle,
        root_name,
        total_files,
        total_folders,
        total_bytes,
        nodes: decoded,
    })
}

/// Unwrap a node's raw decrypted key with the folder master key. Returns 32
/// bytes for files (AES key + nonce + meta-MAC) and 16 bytes for folders.
fn decode_node_key(master: &[u8; 16], n: &RawNode) -> Option<Vec<u8>> {
    let k_field = n.k.as_deref()?;
    // Format is `<sharehandle>:<base64>`; we want the part after the last ':'.
    let key_b64 = k_field.rsplit(':').next()?;
    let enc = crypto::b64decode(key_b64).ok()?;
    if enc.is_empty() || enc.len() % 16 != 0 {
        return None;
    }
    let need = if n.t == 0 { 32 } else { 16 };
    let dec = crypto::aes_ecb_decrypt(master, &enc);
    (dec.len() >= need).then(|| dec[..need].to_vec())
}

/// Fetch a temporary direct download URL (of the *encrypted* bytes) for a single
/// file node — the native-MEGA fallback used when Real-Debrid can't serve a file.
/// This counts against MEGA's bandwidth quota, so it's a last resort only.
pub async fn fetch_download_url(folder_id: &str, handle: &str) -> Result<(String, i64)> {
    let body = serde_json::json!([{ "a": "g", "g": 1, "n": handle }]);

    let resp = http()
        .post(API_URL)
        .query(&[("id", "0"), ("n", folder_id)])
        .json(&body)
        .send()
        .await?
        .error_for_status()?;

    let val: Value = resp.json().await?;
    let obj = val
        .as_array()
        .and_then(|a| a.first())
        .ok_or_else(|| Error::Other(format!("unexpected MEGA response: {val}")))?;

    if let Some(code) = obj.as_i64() {
        return Err(Error::Other(format!("MEGA API error {code}")));
    }

    let size = obj.get("s").and_then(Value::as_i64).unwrap_or(0);
    match obj.get("g") {
        Some(Value::String(url)) => Ok((url.clone(), size)),
        // A negative `g` is an error code (e.g. -17 = over quota).
        Some(Value::Number(n)) => Err(Error::Other(format!(
            "MEGA download error {n} (e.g. -17 = bandwidth quota exceeded)"
        ))),
        _ => Err(Error::Other("MEGA response missing download url".into())),
    }
}

/// Compute every node's `/`-joined path from the root down to itself, given
/// `handle -> (name, parent_handle)`. Iterative (deep trees can't blow the
/// stack) and cycle-guarded (a corrupt parent loop degrades to extra roots
/// instead of hanging), memoizing each ancestor so the whole pass is O(n).
fn compute_paths(info: &HashMap<String, (String, String)>) -> HashMap<String, String> {
    let mut paths: HashMap<String, String> = HashMap::with_capacity(info.len());
    for start in info.keys() {
        if paths.contains_key(start) {
            continue;
        }
        // Walk up to the root or an already-computed ancestor, then unwind.
        let mut chain: Vec<&String> = Vec::new();
        let mut on_chain: HashSet<&String> = HashSet::new();
        let mut base = String::new();
        let mut cur = start;
        loop {
            if let Some(p) = paths.get(cur) {
                base = p.clone();
                break;
            }
            if !on_chain.insert(cur) {
                break; // parent cycle — treat the repeated node as a root
            }
            chain.push(cur);
            let (_, parent) = &info[cur];
            match info.get_key_value(parent) {
                Some((next, _)) => cur = next,
                None => break, // reached the root
            }
        }
        for handle in chain.iter().rev() {
            let (name, _) = &info[*handle];
            base = if base.is_empty() {
                name.clone()
            } else {
                format!("{base}/{name}")
            };
            paths.insert((*handle).clone(), base.clone());
        }
    }
    paths
}

/// `"name.ext"` → `"name (n).ext"` (or `"name (n)"` without an extension).
fn dedup_name(name: &str, n: u32) -> String {
    match name.rfind('.') {
        Some(i) if i > 0 => format!("{} ({}){}", &name[..i], n, &name[i..]),
        _ => format!("{name} ({n})"),
    }
}

#[cfg(test)]
mod tests {
    use super::{compute_paths, dedup_name};
    use std::collections::HashMap;

    fn info(entries: &[(&str, &str, &str)]) -> HashMap<String, (String, String)> {
        entries
            .iter()
            .map(|(h, name, p)| (h.to_string(), (name.to_string(), p.to_string())))
            .collect()
    }

    #[test]
    fn computes_nested_paths() {
        let map = info(&[
            ("root", "Root", "MISSING"),
            ("a", "Sub", "root"),
            ("f", "file.mp4", "a"),
        ]);
        let paths = compute_paths(&map);
        assert_eq!(paths["root"], "Root");
        assert_eq!(paths["a"], "Root/Sub");
        assert_eq!(paths["f"], "Root/Sub/file.mp4");
    }

    #[test]
    fn parent_cycle_does_not_hang() {
        // a → b → a: corrupt data must degrade gracefully, not recurse forever.
        let map = info(&[("a", "A", "b"), ("b", "B", "a"), ("c", "C", "a")]);
        let paths = compute_paths(&map);
        assert_eq!(paths.len(), 3);
        assert!(paths["c"].ends_with("/C"));
    }

    #[test]
    fn dedup_names() {
        assert_eq!(dedup_name("file.mp4", 2), "file (2).mp4");
        assert_eq!(dedup_name("no-ext", 3), "no-ext (3)");
        assert_eq!(dedup_name(".hidden", 2), ".hidden (2)");
    }
}
